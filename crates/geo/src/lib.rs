//! **Geospatial primitives** for Project Lifeline.
//!
//! GPS gives every device a free, global, offline **position**. This crate turns
//! that position into a *routing and addressing* primitive so Lifeline can do the
//! disaster-native thing: address a **region** ("anyone within 2 km of here")
//! instead of an identity — geographic routing and **geocast**.
//!
//! It is pure math with no I/O and (besides `serde` for the wire types) no
//! dependencies — the foundation the router/engine build geo-routing and geocast
//! on, and which the differential-time-sync anchors reference. See
//! [`docs/geo-and-differential.md`](https://github.com/nometria/project-lifeline/blob/main/docs/geo-and-differential.md).
//!
//! **Geohash** encodes a latitude/longitude into a short base-32 string whose
//! *prefix* is a hierarchical cell: every point sharing a prefix is inside the
//! same box, so region membership is a `starts_with`, and neighbouring cells tile
//! the plane — exactly what geocast needs.

use serde::{Deserialize, Serialize};

/// Geohash base-32 alphabet (note: no `a`, `i`, `l`, `o`).
const BASE32: &[u8] = b"0123456789bcdefghjkmnpqrstuvwxyz";

/// A WGS-84 latitude/longitude in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

impl GeoPoint {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

/// An axis-aligned lat/lon bounding box — the area a geohash cell covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
}

impl BBox {
    /// The cell centre.
    pub fn center(&self) -> GeoPoint {
        GeoPoint::new(
            (self.lat_min + self.lat_max) / 2.0,
            (self.lon_min + self.lon_max) / 2.0,
        )
    }
    /// Approximate `(width, height)` of the cell in metres (at its centre lat).
    pub fn dims_m(&self) -> (f64, f64) {
        let mid_lat = (self.lat_min + self.lat_max) / 2.0;
        let w = haversine_m(
            GeoPoint::new(mid_lat, self.lon_min),
            GeoPoint::new(mid_lat, self.lon_max),
        );
        let h = haversine_m(
            GeoPoint::new(self.lat_min, self.lon_min),
            GeoPoint::new(self.lat_max, self.lon_min),
        );
        (w, h)
    }
}

/// Encode a point to a geohash of `len` characters (higher `len` = finer cell).
pub fn encode(p: GeoPoint, len: usize) -> String {
    let mut lat = (-90.0f64, 90.0f64);
    let mut lon = (-180.0f64, 180.0f64);
    let mut hash = String::with_capacity(len);
    let mut even = true; // even bit → longitude
    let mut bit = 0usize;
    let mut ch = 0u8;
    while hash.len() < len {
        if even {
            let mid = (lon.0 + lon.1) / 2.0;
            if p.lon >= mid {
                ch |= 1 << (4 - bit);
                lon.0 = mid;
            } else {
                lon.1 = mid;
            }
        } else {
            let mid = (lat.0 + lat.1) / 2.0;
            if p.lat >= mid {
                ch |= 1 << (4 - bit);
                lat.0 = mid;
            } else {
                lat.1 = mid;
            }
        }
        even = !even;
        if bit < 4 {
            bit += 1;
        } else {
            hash.push(BASE32[ch as usize] as char);
            bit = 0;
            ch = 0;
        }
    }
    hash
}

/// Decode a geohash to the bounding box it covers. `None` if any char is invalid.
pub fn decode_bbox(hash: &str) -> Option<BBox> {
    let mut lat = (-90.0f64, 90.0f64);
    let mut lon = (-180.0f64, 180.0f64);
    let mut even = true;
    for c in hash.chars() {
        let idx = BASE32
            .iter()
            .position(|&b| b == c.to_ascii_lowercase() as u8)?;
        for bit in (0..5).rev() {
            let set = (idx >> bit) & 1 == 1;
            if even {
                let mid = (lon.0 + lon.1) / 2.0;
                if set {
                    lon.0 = mid;
                } else {
                    lon.1 = mid;
                }
            } else {
                let mid = (lat.0 + lat.1) / 2.0;
                if set {
                    lat.0 = mid;
                } else {
                    lat.1 = mid;
                }
            }
            even = !even;
        }
    }
    Some(BBox {
        lat_min: lat.0,
        lat_max: lat.1,
        lon_min: lon.0,
        lon_max: lon.1,
    })
}

/// Decode a geohash to its centre point.
pub fn decode(hash: &str) -> Option<GeoPoint> {
    decode_bbox(hash).map(|b| b.center())
}

/// **Geocast membership**: is `p` inside the `region` geohash cell? This is the
/// core delivery test — "deliver to anyone whose position is in region R".
pub fn contains(region: &str, p: GeoPoint) -> bool {
    match decode_bbox(region) {
        Some(b) => {
            p.lat >= b.lat_min && p.lat <= b.lat_max && p.lon >= b.lon_min && p.lon <= b.lon_max
        }
        None => false,
    }
}

/// Compass direction for adjacency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    N,
    S,
    E,
    W,
}

// Neighbour/border lookup tables (standard geohash adjacency), indexed by parity
// of the hash length. `NEIGHBOR[dir]` maps a last-char to its neighbour's
// last-char; `BORDER[dir]` is the set of last-chars that force a parent carry.
fn neighbor_table(dir: Dir, odd: bool) -> (&'static str, &'static str) {
    // (neighbor, border) for the "even-length" case per direction.
    let even = match dir {
        Dir::N => ("p0r21436x8zb9dcf5h7kjnmqesgutwvy", "prxz"),
        Dir::S => ("14365h7k9dcfesgujnmqp0r2twvyx8zb", "028b"),
        Dir::E => ("bc01fg45238967deuvhjyznpkmstqrwx", "bcfguvyz"),
        Dir::W => ("238967debc01fg45kmstqrwxuvhjyznp", "0145hjnp"),
    };
    if !odd {
        return even;
    }
    // For odd-length hashes the axes swap: N↔E, S↔W tables are reused.
    match dir {
        Dir::N => neighbor_table(Dir::E, false),
        Dir::S => neighbor_table(Dir::W, false),
        Dir::E => neighbor_table(Dir::N, false),
        Dir::W => neighbor_table(Dir::S, false),
    }
}

/// The geohash cell adjacent to `hash` in direction `dir` (same length).
pub fn adjacent(hash: &str, dir: Dir) -> String {
    if hash.is_empty() {
        return String::new();
    }
    let hash = hash.to_ascii_lowercase();
    let last = hash.chars().last().unwrap();
    let odd = hash.len() % 2 == 1;
    let (neighbor, border) = neighbor_table(dir, odd);
    let mut base: String = hash[..hash.len() - 1].to_string();
    if border.contains(last) {
        base = adjacent(&base, dir);
    }
    if let Some(idx) = neighbor.chars().position(|c| c == last) {
        base.push(BASE32[idx] as char);
    }
    base
}

/// The 8 geohash cells surrounding `hash` (N, NE, E, SE, S, SW, W, NW).
pub fn neighbors(hash: &str) -> Vec<String> {
    let n = adjacent(hash, Dir::N);
    let s = adjacent(hash, Dir::S);
    vec![
        n.clone(),
        adjacent(&n, Dir::E),
        adjacent(hash, Dir::E),
        adjacent(&s, Dir::E),
        s.clone(),
        adjacent(&s, Dir::W),
        adjacent(hash, Dir::W),
        adjacent(&n, Dir::W),
    ]
}

/// Geohash cells whose union covers a circle of `radius_m` around `center` — the
/// set to geocast to. Picks the finest precision whose cell is at least the
/// radius across, then returns that centre cell plus its 8 neighbours (a 3×3
/// block that fully contains the circle).
pub fn cells_for_radius(center: GeoPoint, radius_m: f64) -> Vec<String> {
    let mut chosen = 1usize;
    for len in 1..=12 {
        let bbox = decode_bbox(&encode(center, len)).unwrap();
        let (w, h) = bbox.dims_m();
        if w.min(h) >= radius_m {
            chosen = len;
        } else {
            break;
        }
    }
    let h = encode(center, chosen);
    let mut cells = vec![h.clone()];
    cells.extend(neighbors(&h));
    cells
}

/// All geohash cells **of a fixed `precision`** whose union covers a circle of
/// `radius_m` around `center`. Unlike [`cells_for_radius`] (which varies the
/// precision), this keeps precision fixed so a receiver can match *its own* cell
/// against a geocast without any wire field — it just encodes its position at the
/// same precision. Expands outward in neighbour rings until the radius is covered.
pub fn cells_covering(center: GeoPoint, radius_m: f64, precision: usize) -> Vec<String> {
    use std::collections::HashSet;
    let center_cell = encode(center, precision);
    let (w, h) = decode_bbox(&center_cell).unwrap().dims_m();
    let cell_min = w.min(h).max(1.0);
    let rings = (radius_m / cell_min).ceil() as usize;
    let mut set: HashSet<String> = HashSet::new();
    set.insert(center_cell.clone());
    let mut frontier = vec![center_cell];
    for _ in 0..rings {
        let mut next = Vec::new();
        for c in &frontier {
            for n in neighbors(c) {
                if set.insert(n.clone()) {
                    next.push(n);
                }
            }
        }
        frontier = next;
    }
    let mut out: Vec<String> = set.into_iter().collect();
    out.sort();
    out
}

/// Great-circle distance between two points in metres (haversine).
pub fn haversine_m(a: GeoPoint, b: GeoPoint) -> f64 {
    const R: f64 = 6_371_000.0; // mean Earth radius, metres
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let x = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * R * x.sqrt().atan2((1.0 - x).sqrt())
}

/// Initial great-circle **bearing** from `a` to `b`, in degrees clockwise from
/// true north (0 = N, 90 = E, 180 = S, 270 = W). This is the "which way do I
/// walk to reach my contact" heading behind the *find-each-other* view: distance
/// (`haversine_m`) says how far, bearing says which way. Returns 0 when the two
/// points coincide.
pub fn bearing_deg(a: GeoPoint, b: GeoPoint) -> f64 {
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    let deg = y.atan2(x).to_degrees();
    (deg + 360.0) % 360.0
}

/// The nearest 8-point compass label (`"N"`, `"NE"`, `"E"`, …) to a bearing in
/// degrees. Human-readable direction for the UI: "Maya · 120 m · NE". Accepts
/// any real bearing (normalizes out-of-range and negative values).
pub fn compass_8(bearing_deg: f64) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let norm = (bearing_deg % 360.0 + 360.0) % 360.0;
    let idx = (norm / 45.0).round() as usize % 8;
    POINTS[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_canonical_geohash() {
        // The textbook example: (42.6, -5.6) → "ezs42" at precision 5.
        assert_eq!(encode(GeoPoint::new(42.6, -5.6), 5), "ezs42");
    }

    #[test]
    fn decode_then_encode_round_trips_within_the_cell() {
        let p = GeoPoint::new(37.7749, -122.4194); // San Francisco
        let h = encode(p, 9);
        let c = decode(&h).unwrap();
        // The centre re-encodes to the same cell, and is close to the original.
        assert_eq!(encode(c, 9), h);
        assert!(haversine_m(p, c) < 5.0, "9-char cell is a few metres");
    }

    #[test]
    fn contains_is_geocast_membership() {
        let region = encode(GeoPoint::new(40.0, -74.0), 5); // a ~5 km cell
                                                            // A point at the cell centre is inside; a point far away is not.
        assert!(contains(&region, decode(&region).unwrap()));
        assert!(!contains(&region, GeoPoint::new(0.0, 0.0)));
        // A finer point sharing the region's prefix is inside it.
        let fine = encode(GeoPoint::new(40.0, -74.0), 8);
        assert!(fine.starts_with(&region));
        assert!(contains(&region, decode(&fine).unwrap()));
    }

    #[test]
    fn neighbors_tile_around_the_cell() {
        let h = encode(GeoPoint::new(51.5074, -0.1278), 6); // London
        let ns = neighbors(&h);
        assert_eq!(ns.len(), 8);
        assert!(!ns.contains(&h), "the centre is not its own neighbour");
        assert!(ns.iter().all(|n| n.len() == h.len()), "same precision");
        // Distinct cells.
        let mut sorted = ns.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 8);
        // A point just east of the centre cell lands in one of the neighbours.
        let b = decode_bbox(&h).unwrap();
        let just_east = GeoPoint::new(b.center().lat, b.lon_max + (b.lon_max - b.lon_min) * 0.25);
        assert!(!contains(&h, just_east));
        assert!(ns.iter().any(|n| contains(n, just_east)));
    }

    #[test]
    fn haversine_matches_known_distances() {
        // 1° of longitude at the equator ≈ 111.32 km.
        let d = haversine_m(GeoPoint::new(0.0, 0.0), GeoPoint::new(0.0, 1.0));
        assert!((d - 111_195.0).abs() < 500.0, "got {d}");
        // Zero distance.
        assert_eq!(
            haversine_m(GeoPoint::new(12.0, 34.0), GeoPoint::new(12.0, 34.0)),
            0.0
        );
    }

    #[test]
    fn bearing_points_the_right_way() {
        let here = GeoPoint::new(0.0, 0.0);
        // Due north, east, south, west from the origin (small steps).
        assert!((bearing_deg(here, GeoPoint::new(1.0, 0.0)) - 0.0).abs() < 1.0);
        assert!((bearing_deg(here, GeoPoint::new(0.0, 1.0)) - 90.0).abs() < 1.0);
        assert!((bearing_deg(here, GeoPoint::new(-1.0, 0.0)) - 180.0).abs() < 1.0);
        assert!((bearing_deg(here, GeoPoint::new(0.0, -1.0)) - 270.0).abs() < 1.0);
        // Coincident points are well-defined (no NaN).
        assert_eq!(bearing_deg(here, here), 0.0);
    }

    #[test]
    fn compass_labels_the_eight_points() {
        assert_eq!(compass_8(0.0), "N");
        assert_eq!(compass_8(45.0), "NE");
        assert_eq!(compass_8(90.0), "E");
        assert_eq!(compass_8(225.0), "SW");
        // Wrap-around: 359° rounds back to N, and negatives normalize.
        assert_eq!(compass_8(359.0), "N");
        assert_eq!(compass_8(-90.0), "W");
    }

    #[test]
    fn radius_coverage_contains_nearby_points() {
        let center = GeoPoint::new(34.0522, -118.2437); // Los Angeles
        let radius = 2000.0; // 2 km
        let cells = cells_for_radius(center, radius);
        assert_eq!(cells.len(), 9, "centre + 8 neighbours");
        // Every point within the radius must fall in at least one covering cell.
        for (dlat, dlon) in [
            (0.0, 0.0),
            (0.01, 0.0),
            (-0.01, 0.0),
            (0.0, 0.012),
            (0.0, -0.012),
        ] {
            let p = GeoPoint::new(center.lat + dlat, center.lon + dlon);
            if haversine_m(center, p) <= radius {
                assert!(
                    cells.iter().any(|c| contains(c, p)),
                    "point {p:?} within radius not covered"
                );
            }
        }
    }
}
