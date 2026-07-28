//! **Offline over-spend containment — the make-or-break measurement.**
//!
//! A frozen research theory (see `docs/research/`) claims that when a bearer
//! token is double-spent offline and a revocation gossips to catch it, the total
//! over-spend `N_win` either grows mildly (`~ (a/d) ln N`) or undergoes a sharp
//! phase transition — depending on a modelling axis an independent review
//! isolated as **transferability**:
//!
//! * **Non-transferable (single agent).** One attacker holds the token and
//!   over-spends fresh verifiers itself. The token does not replicate. This is a
//!   *single* over-spending agent racing a *growing* detection epidemic; the
//!   theory predicts `E[N_win] ≈ (a/d) ln N`, always finite — benign.
//! * **Transferable (spreading prey).** The token is copied and re-transferred, so
//!   the set of over-spending holders itself *spreads* like an epidemic while
//!   detection (trail-confined) chases it. This is the **chase-escape /
//!   predator–prey** process, which has a genuine phase transition: below a
//!   critical speed ratio the fraud is contained; above it, it runs away to `Θ(N)`.
//!
//! This module is a self-contained mean-field (complete-graph) simulator of both,
//! run to *measure which claim the data actually supports* rather than assert it.
//! It is not wired through the full node — it is the abstract dynamical model, so
//! the measurement is cheap, exact-in-distribution, and reproducible (seeded RNG).

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// One realization of the **single-agent** (non-transferable) model on the
/// complete graph. One attacker over-spends fresh verifiers at rate `a`; each
/// over-spend self-seeds a revocation that gossips at per-node rate `d`
/// (immunizing susceptibles). Returns the total over-spend `N_win`.
///
/// Analytically `E[N_win] = Σ_{j=0}^{N-1} a/(a + d·j) ≈ (a/d) ln N` — the theory's
/// R2 law. The simulation checks it.
pub fn run_single_agent(n: u64, a: f64, d: f64, rng: &mut StdRng) -> u64 {
    let mut s = n; // susceptible verifiers
    let mut imm = 0u64; // immunized (revocation reached)
    let mut n_win = 0u64;
    loop {
        let sf = s as f64;
        let nf = n as f64;
        let r_over = a * sf / nf; // agent over-spends a fresh verifier
        let r_gossip = d * imm as f64 * sf / nf; // revocation immunizes a susceptible
        let total = r_over + r_gossip;
        if total <= 0.0 {
            break; // s == 0: nothing fresh left
        }
        // Either way one susceptible is consumed; it is an over-spend with
        // probability r_over/total.
        s -= 1;
        imm += 1;
        if rng.gen::<f64>() * total < r_over {
            n_win += 1;
        }
    }
    n_win
}

/// One realization of the **transferable** (spreading-prey) model on the complete
/// graph: the chase-escape / predator–prey process. Prey (over-spending holders)
/// spread to fresh verifiers at rate `lambda_r`; the predator (trail-confined
/// detection) converts prey to immunized at rate `lambda_b`. Returns total
/// over-spend `N_win` = every verifier ever reached by prey.
pub fn run_chase_escape(n: u64, lambda_r: f64, lambda_b: f64, rng: &mut StdRng) -> u64 {
    if n < 2 {
        return n;
    }
    let mut s = n - 2; // susceptible
    let mut i = 1i64; // prey (over-spent, not yet immunized)
    let mut r = 1i64; // predator (immunized)
    let mut n_win = 1u64; // the seed over-spend
    let nf = n as f64;
    loop {
        let r_infect = lambda_r * i as f64 * s as f64 / nf; // S -> I (over-spend)
        let r_convert = lambda_b * r as f64 * i as f64 / nf; // I -> R (caught)
        let total = r_infect + r_convert;
        if total <= 0.0 || i == 0 {
            break; // prey extinct (contained) or nothing left
        }
        if rng.gen::<f64>() * total < r_infect {
            s -= 1;
            i += 1;
            n_win += 1;
        } else {
            i -= 1;
            r += 1;
        }
    }
    n_win
}

/// Transferable model with **flood** detection (well-mixed): the revocation
/// gossips to *all* verifiers (immunizing susceptibles pre-emptively, `S -> R`),
/// not only along the over-spend trail. Tests whether flood gossip contains a
/// spreading token where trail-confined detection cannot.
pub fn run_chase_escape_flood(n: u64, lambda_r: f64, lambda_b: f64, rng: &mut StdRng) -> u64 {
    if n < 2 {
        return n;
    }
    let mut s = n - 2;
    let mut i = 1i64;
    let mut r = 1i64;
    let mut n_win = 1u64;
    let nf = n as f64;
    loop {
        let r_infect = lambda_r * i as f64 * s as f64 / nf; // S -> I
        let r_convert = lambda_b * r as f64 * i as f64 / nf; // I -> R
        let r_flood = lambda_b * r as f64 * s as f64 / nf; // S -> R (pre-emptive)
        let total = r_infect + r_convert + r_flood;
        if total <= 0.0 || i == 0 {
            break;
        }
        let x = rng.gen::<f64>() * total;
        if x < r_infect {
            s -= 1;
            i += 1;
            n_win += 1;
        } else if x < r_infect + r_convert {
            i -= 1;
            r += 1;
        } else {
            s -= 1;
            r += 1;
        }
    }
    n_win
}

/// Transferable model on a **2-D torus lattice** with trail-confined detection —
/// the actual chase-escape process where a spatial containment transition exists.
/// Prey (red) fire at rate `lambda_r`, converting a random White neighbour to Red
/// (over-spend); predators (blue) fire at rate `lambda_b`, converting a random Red
/// neighbour to Blue. Per-particle firing. Returns total over-spend (sites ever
/// red). `l` is the side length (`n = l*l`).
pub fn run_chase_escape_2d(l: usize, lambda_r: f64, lambda_b: f64, rng: &mut StdRng) -> u64 {
    let n = l * l;
    let mut state = vec![0u8; n]; // 0 White, 1 Red, 2 Blue
    let mut reds: Vec<usize> = Vec::new();
    let mut red_pos: Vec<i64> = vec![-1; n];
    let mut blues: Vec<usize> = Vec::new();
    let mut blue_pos: Vec<i64> = vec![-1; n];

    let add_red = |site: usize, state: &mut [u8], reds: &mut Vec<usize>, red_pos: &mut [i64]| {
        state[site] = 1;
        red_pos[site] = reds.len() as i64;
        reds.push(site);
    };
    // Seed: centre Red, an adjacent site Blue (so detection has prey to chase).
    let origin = (l / 2) * l + l / 2;
    add_red(origin, &mut state, &mut reds, &mut red_pos);
    let mut n_win = 1u64;
    let blue_seed = (l / 2) * l + (l / 2 + 1) % l; // right neighbour on the torus
    state[blue_seed] = 2;
    blue_pos[blue_seed] = 0;
    blues.push(blue_seed);

    let neighbors = |site: usize| -> [usize; 4] {
        let x = site % l;
        let y = site / l;
        [
            y * l + (x + 1) % l,
            y * l + (x + l - 1) % l,
            ((y + 1) % l) * l + x,
            ((y + l - 1) % l) * l + x,
        ]
    };

    let max_iters = 200u64 * n as u64;
    let mut iters = 0u64;
    while !reds.is_empty() && iters < max_iters {
        iters += 1;
        let tr = lambda_r * reds.len() as f64;
        let tb = lambda_b * blues.len() as f64;
        let total = tr + tb;
        if total <= 0.0 {
            break;
        }
        if rng.gen::<f64>() * total < tr {
            // Red fires: convert a random White neighbour to Red.
            let site = reds[rng.gen_range(0..reds.len())];
            let nb = neighbors(site)[rng.gen_range(0..4)];
            if state[nb] == 0 {
                state[nb] = 1;
                red_pos[nb] = reds.len() as i64;
                reds.push(nb);
                n_win += 1;
            }
        } else {
            // Blue fires: convert a random Red neighbour to Blue (remove from reds).
            let site = blues[rng.gen_range(0..blues.len())];
            let nb = neighbors(site)[rng.gen_range(0..4)];
            if state[nb] == 1 {
                // swap-remove nb from reds
                let p = red_pos[nb] as usize;
                let last = *reds.last().unwrap();
                reds.swap_remove(p);
                if p < reds.len() {
                    red_pos[last] = p as i64;
                }
                red_pos[nb] = -1;
                state[nb] = 2;
                blue_pos[nb] = blues.len() as i64;
                blues.push(nb);
            }
        }
    }
    n_win
}

/// Mean over `trials` for the flood-detection transferable model.
pub fn mean_chase_escape_flood(n: u64, lr: f64, lb: f64, trials: u32, seed: u64) -> f64 {
    let mut sum = 0u64;
    for t in 0..trials {
        let mut rng = StdRng::seed_from_u64(seed ^ (n << 20) ^ (t as u64) ^ 0xF100D);
        sum += run_chase_escape_flood(n, lr, lb, &mut rng);
    }
    sum as f64 / trials as f64
}

/// Mean over `trials` for the 2-D lattice chase-escape.
pub fn mean_chase_escape_2d(l: usize, lr: f64, lb: f64, trials: u32, seed: u64) -> f64 {
    let mut sum = 0u64;
    for t in 0..trials {
        let mut rng = StdRng::seed_from_u64(seed ^ ((l as u64) << 20) ^ (t as u64) ^ 0x2D2D);
        sum += run_chase_escape_2d(l, lr, lb, &mut rng);
    }
    sum as f64 / trials as f64
}

/// Mean over `trials` independent realizations (deterministic: seed + trial idx).
pub fn mean_single_agent(n: u64, a: f64, d: f64, trials: u32, seed: u64) -> f64 {
    let mut sum = 0u64;
    for t in 0..trials {
        let mut rng = StdRng::seed_from_u64(seed ^ (n << 20) ^ (t as u64));
        sum += run_single_agent(n, a, d, &mut rng);
    }
    sum as f64 / trials as f64
}

/// Mean over `trials` for the chase-escape (transferable) model.
pub fn mean_chase_escape(n: u64, lambda_r: f64, lambda_b: f64, trials: u32, seed: u64) -> f64 {
    let mut sum = 0u64;
    for t in 0..trials {
        let mut rng = StdRng::seed_from_u64(seed ^ (n << 20) ^ (t as u64) ^ 0xABCD);
        sum += run_chase_escape(n, lambda_r, lambda_b, &mut rng);
    }
    sum as f64 / trials as f64
}

/// Print the two headline measurements as fixed-width tables.
pub fn report() -> String {
    let mut out = String::new();
    let trials = 400u32;
    let seed = 42u64;

    // --- Measurement 1: does the single-agent (non-transferable) N_win grow as
    // (a/d) ln N?  If E[N_win]/ln N is ~constant = a/d, R2 holds. ---
    out.push_str("== Measurement 1: single-agent (non-transferable), a=1, d=1 ==\n");
    out.push_str("  expect E[N_win] ~ (a/d) ln N = ln N  =>  E[N_win]/ln N ~ 1\n");
    out.push_str(&format!(
        "  {:>8} {:>12} {:>12} {:>12}\n",
        "N", "E[N_win]", "ln N", "ratio"
    ));
    for &nn in &[100u64, 300, 1000, 3000, 10_000, 30_000, 100_000] {
        let m = mean_single_agent(nn, 1.0, 1.0, trials, seed);
        let lnn = (nn as f64).ln();
        out.push_str(&format!(
            "  {:>8} {:>12.2} {:>12.2} {:>12.3}\n",
            nn,
            m,
            lnn,
            m / lnn
        ));
    }

    // --- Measurement 2: transferable token, TRAIL-confined detection, WELL-MIXED
    // (complete graph). Sweep rho = lambda_r/lambda_b; over-spend fraction. ---
    out.push_str("\n== Measurement 2: transferable + trail detection, WELL-MIXED (N=20000) ==\n");
    out.push_str("  sweep rho=lambda_r/lambda_b (rho<1 => detection faster); fraction N_win/N\n");
    out.push_str(&format!(
        "  {:>8} {:>14} {:>12}\n",
        "rho", "E[N_win]", "N_win/N"
    ));
    let big_n = 20_000u64;
    for &rho in &[0.1f64, 0.2, 0.4, 0.8, 1.0, 2.0, 5.0] {
        let m = mean_chase_escape(big_n, rho, 1.0, 120, seed);
        out.push_str(&format!(
            "  {:>8.2} {:>14.1} {:>12.4}\n",
            rho,
            m,
            m / big_n as f64
        ));
    }

    // --- Measurement 3: transferable token, FLOOD detection, WELL-MIXED. Does
    // flooding gossip (immunize susceptibles ahead) contain what trail cannot? ---
    out.push_str("\n== Measurement 3: transferable + FLOOD detection, WELL-MIXED (N=20000) ==\n");
    out.push_str(&format!(
        "  {:>8} {:>14} {:>12}\n",
        "rho", "E[N_win]", "N_win/N"
    ));
    for &rho in &[0.1f64, 0.2, 0.4, 0.8, 1.0, 2.0, 5.0] {
        let m = mean_chase_escape_flood(big_n, rho, 1.0, 120, seed);
        out.push_str(&format!(
            "  {:>8.2} {:>14.1} {:>12.4}\n",
            rho,
            m,
            m / big_n as f64
        ));
    }

    // --- Measurement 4: transferable token, trail detection, on a 2-D LATTICE.
    // This is the real chase-escape process; look for the spatial containment
    // transition in rho. ---
    let l = 160usize;
    let n2 = (l * l) as f64;
    out.push_str(&format!(
        "\n== Measurement 4: transferable + trail detection, 2-D LATTICE ({}x{}={}) ==\n",
        l,
        l,
        l * l
    ));
    out.push_str("  sweep rho; fraction of the grid over-spent (spatial containment?)\n");
    out.push_str(&format!(
        "  {:>8} {:>14} {:>12}\n",
        "rho", "E[N_win]", "N_win/N"
    ));
    for &rho in &[0.3f64, 0.5, 0.7, 0.9, 1.0, 1.1, 1.3, 1.6, 2.0] {
        let m = mean_chase_escape_2d(l, rho, 1.0, 40, seed);
        out.push_str(&format!("  {:>8.2} {:>14.1} {:>12.4}\n", rho, m, m / n2));
    }

    out.push_str(
        "\nInterpretation: M1 ratio ~const => R2's (a/d)ln N holds for the\n\
         NON-transferable case (benign, bounded). M2 => on a well-mixed graph,\n\
         trail-confined detection CANNOT contain a transferable token (runaway at\n\
         all rho) -- containment is not a well-mixed phenomenon. M3 => whether\n\
         FLOOD gossip is the lever that restores containment when well-mixed. M4 =>\n\
         whether a 2-D LATTICE (spatial locality) gives trail detection a genuine\n\
         containment transition, and where the critical rho* sits.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_agent_grows_but_stays_far_below_n() {
        // With a=d, E[N_win] ~ ln N: small relative to N, and increasing in N.
        let m_small = mean_single_agent(1000, 1.0, 1.0, 200, 1);
        let m_big = mean_single_agent(100_000, 1.0, 1.0, 200, 1);
        assert!(m_big > m_small, "over-spend should increase with N");
        assert!(
            m_big < 100_000 as f64 * 0.05,
            "single-agent over-spend must stay far below N (got {m_big})"
        );
    }

    #[test]
    fn single_agent_matches_the_harmonic_law() {
        // E[N_win] = sum_{j=0}^{N-1} a/(a+d j); for a=d=1 this is H_N ~ ln N + gamma.
        let n = 5000u64;
        let measured = mean_single_agent(n, 1.0, 1.0, 2000, 7);
        let mut expected = 0.0f64;
        for j in 0..n {
            expected += 1.0 / (1.0 + j as f64);
        }
        let rel = (measured - expected).abs() / expected;
        assert!(
            rel < 0.05,
            "single-agent mean {measured} should match harmonic sum {expected} (rel {rel:.3})"
        );
    }

    #[test]
    fn well_mixed_trail_detection_cannot_contain_a_transferable_token() {
        // Finding: on a complete (well-mixed) graph, trail-confined detection
        // runs away even when it is much faster than the adversary — a single
        // detection seed is diluted 1/N and cannot catch an exponentially
        // spreading prey. Over-spend is near-total across ratios.
        let n = 20_000u64;
        let fast_detect = mean_chase_escape(n, 0.2, 1.0, 60, 3) / n as f64; // detector 5x faster
        assert!(
            fast_detect > 0.6,
            "well-mixed trail detection should still run away (fraction {fast_detect:.3})"
        );
    }

    #[test]
    fn flood_detection_restores_containment_when_well_mixed() {
        // Flood gossip immunizes susceptibles pre-emptively, so it grows as an
        // independent epidemic and contains a fast-enough race.
        let n = 20_000u64;
        let contained = mean_chase_escape_flood(n, 0.2, 1.0, 60, 3) / n as f64;
        let runaway = mean_chase_escape_flood(n, 5.0, 1.0, 60, 3) / n as f64;
        assert!(
            contained < 0.2,
            "flood detection should contain when much faster (fraction {contained:.3})"
        );
        assert!(
            runaway > contained,
            "a faster adversary must over-spend more"
        );
    }

    #[test]
    fn lattice_gives_trail_detection_a_containment_transition() {
        // Finding: on a 2-D lattice, spatial locality lets the trail-confined
        // detection front keep pace, so there IS a containment transition —
        // contained (small fraction) when detection is faster, runaway when the
        // adversary is faster.
        let l = 120usize;
        let n = (l * l) as f64;
        let contained = mean_chase_escape_2d(l, 0.5, 1.0, 25, 3) / n; // detector 2x faster
        let runaway = mean_chase_escape_2d(l, 2.0, 1.0, 25, 3) / n; // adversary 2x faster
        assert!(
            contained < 0.25,
            "lattice should contain when detection is faster (fraction {contained:.3})"
        );
        assert!(
            runaway > 0.5,
            "lattice should run away when adversary is faster (fraction {runaway:.3})"
        );
    }
}
