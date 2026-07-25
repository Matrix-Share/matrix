//! The **routing policy** seam.
//!
//! [`DtnRouter`](crate::DtnRouter) owns the *mechanism* of forwarding — iterating
//! candidate bundles in priority order, mutating copy budgets, marking peers
//! offered, counting stats. *Which* bundles to hand a given peer, and how many
//! copies, is the *policy* — and it lives behind [`RoutingPolicy`] so a
//! deployment can swap binary spray-and-wait for epidemic, PRoPHET, a
//! Reticulum-style transport-node strategy, etc. without touching the router.
//!
//! The default is [`SprayAndWaitPolicy`], the binary spray-and-wait +
//! gateway-gradient behaviour Lifeline ships (FR-24, §12.1). A policy sees only
//! scalar context ([`OfferContext`]) — no store internals — so it stays a pure
//! decision function that is trivial to unit-test.

use lifeline_proto::Priority;

/// Everything a [`RoutingPolicy`] needs to decide one bundle↔peer offer, as plain
/// values (so the decision never borrows router internals).
#[derive(Debug, Clone, Copy)]
pub struct OfferContext {
    /// This peer *is* the bundle's final destination.
    pub peer_is_dst: bool,
    /// This peer is a demoted (suspected black-hole) relay (FR-47).
    pub peer_demoted: bool,
    /// Handing to this peer moves the bundle "downhill" toward a gateway (or the
    /// peer is itself a gateway) — the DTN escape hatch for the last copy.
    pub peer_helps_gateway: bool,
    /// The bundle's priority class.
    pub priority: Priority,
    /// Approximate stored size in bytes (for bandwidth fit).
    pub size: u64,
    /// Spray copy budget the bundle currently has.
    pub copies_left: u16,
    /// The peer's low-bandwidth soft cap, if any (`None` = fat link).
    pub soft_max_bytes: Option<u64>,
}

/// What to do with one candidate bundle for one peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferAction {
    /// Hand `give` copies to the peer and keep `keep` locally. (The router marks
    /// the peer offered so it isn't re-sent the same bundle this contact.)
    Forward { give: u16, keep: u16 },
    /// Don't forward to this peer now. Not marked offered — a later or fatter
    /// contact may still carry it (e.g. a bulky bundle held off a thin bearer).
    Hold,
}

/// Pluggable forwarding strategy. Must be `Send` because the router is owned by
/// the engine thread.
pub trait RoutingPolicy: Send {
    /// Decide what to do with one candidate bundle for one peer.
    fn decide(&self, ctx: &OfferContext) -> OfferAction;
}

/// Lifeline's default: **binary spray-and-wait** with the gateway-gradient escape
/// hatch, reputation route-around, and bandwidth-adaptive hold-back (FR-24/47,
/// "straw, not a firehose").
#[derive(Debug, Clone, Copy, Default)]
pub struct SprayAndWaitPolicy;

impl RoutingPolicy for SprayAndWaitPolicy {
    fn decide(&self, ctx: &OfferContext) -> OfferAction {
        // Route around demoted (black-hole) relays for normal traffic — never for
        // a direct delivery to the recipient, and never for an SOS (an emergency
        // must not be blocked on a reputation heuristic). FR-47.
        if ctx.peer_demoted && !ctx.peer_is_dst && ctx.priority != Priority::Sos {
            return OfferAction::Hold;
        }
        // Bearer fit: hold a bulky NORMAL/BULK bundle back from a low-bandwidth
        // link so it waits for a fatter bearer. Emergencies (SOS/ALERT) and the
        // final hop always pass, and holding does not mark it offered.
        if let Some(cap) = ctx.soft_max_bytes {
            let heavy = ctx.size > cap;
            let emergency = ctx.priority == Priority::Sos || ctx.priority == Priority::Alert;
            if heavy && !ctx.peer_is_dst && !emergency {
                return OfferAction::Hold;
            }
        }
        let copies = ctx.copies_left;
        if copies > 1 {
            // Spray phase: give half the budget away, keep the rest.
            let give = copies / 2;
            OfferAction::Forward {
                give,
                keep: copies - give,
            }
        } else if ctx.peer_is_dst || ctx.peer_helps_gateway {
            // Wait phase: pass the last copy toward the destination or downhill
            // toward a gateway, *without* depleting our own budget (redundant
            // delivery is the goal).
            OfferAction::Forward {
                give: 1,
                keep: copies,
            }
        } else {
            OfferAction::Hold
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> OfferContext {
        OfferContext {
            peer_is_dst: false,
            peer_demoted: false,
            peer_helps_gateway: false,
            priority: Priority::Normal,
            size: 100,
            copies_left: 4,
            soft_max_bytes: None,
        }
    }

    #[test]
    fn spray_gives_half() {
        let p = SprayAndWaitPolicy;
        assert_eq!(p.decide(&ctx()), OfferAction::Forward { give: 2, keep: 2 });
    }

    #[test]
    fn last_copy_only_flows_toward_dst_or_gateway() {
        let p = SprayAndWaitPolicy;
        let mut c = ctx();
        c.copies_left = 1;
        assert_eq!(p.decide(&c), OfferAction::Hold);
        c.peer_is_dst = true;
        assert_eq!(p.decide(&c), OfferAction::Forward { give: 1, keep: 1 });
        c.peer_is_dst = false;
        c.peer_helps_gateway = true;
        assert_eq!(p.decide(&c), OfferAction::Forward { give: 1, keep: 1 });
    }

    #[test]
    fn demoted_relay_is_skipped_except_dst_and_sos() {
        let p = SprayAndWaitPolicy;
        let mut c = ctx();
        c.peer_demoted = true;
        assert_eq!(p.decide(&c), OfferAction::Hold);
        // SOS ignores demotion.
        c.priority = Priority::Sos;
        assert!(matches!(p.decide(&c), OfferAction::Forward { .. }));
        // A direct delivery to the recipient ignores demotion.
        c.priority = Priority::Normal;
        c.peer_is_dst = true;
        assert!(matches!(p.decide(&c), OfferAction::Forward { .. }));
    }

    #[test]
    fn heavy_bundle_held_off_thin_link_but_emergency_passes() {
        let p = SprayAndWaitPolicy;
        let mut c = ctx();
        c.soft_max_bytes = Some(50); // link cap below the 100-byte bundle
        assert_eq!(p.decide(&c), OfferAction::Hold);
        c.priority = Priority::Alert; // emergency bypasses the cap
        assert!(matches!(p.decide(&c), OfferAction::Forward { .. }));
    }
}
