//! Finite-size-scaling sweep for the planar chase-escape containment exponent.
//!
//! Drives the fast compiled 2-D torus chase-escape (`run_chase_escape_2d`) across
//! system sizes L and speed ratios rho = lambda_r/lambda_b, recording enough per
//! (L,rho) to extract (rho*, nu, beta, gamma) two independent ways:
//!
//!   order parameter    m   = <x>            x = N_win / N
//!   susceptibility     chi = N * Var(x)      (chi_max ~ L^{gamma/nu})
//!   Binder cumulant    U   = 1 - <x^4>/(3<x^2>^2)   (curves cross at rho*)
//!   contained mean     <N_win | contained>  and containment fraction
//!
//! Route A (percolation FSS): rho* from the Binder crossing; gamma/nu from the
//! susceptibility peak; nu from data collapse; gamma = (gamma/nu)*nu.
//! Route B (theory's literal definition, Thm. "Critical divergence"): the CONTAINED
//! over-spend diverges as <N_win|contained> ~ (rho* - rho)^{-gamma} as rho -> rho*-.
//! gamma = 1 exactly in mean-field/1-D; the planar value is open.
//!
//! Run (from repo root):
//!   cargo run -p lifeline-sim --release --example fss > gamma-scaling.csv
//! Optional first arg `quick` for a fast smaller sweep.

use lifeline_sim::containment::run_chase_escape_2d;
use rand::rngs::StdRng;
use rand::SeedableRng;

const RUNAWAY_CUT: f64 = 0.05; // x > cut counts as runaway (not contained)

fn main() {
    let quick = std::env::args().nth(1).as_deref() == Some("quick");

    // (L, trials): more trials at small L (cheap), fewer at large L (costly).
    let plan: Vec<(usize, u32)> = if quick {
        vec![(48, 600), (64, 600), (96, 400)]
    } else {
        vec![(48, 1500), (64, 1500), (96, 1000), (128, 700), (160, 500)]
    };

    // rho grid: brackets the contained side, the critical point, and enough of the
    // runaway side to capture the susceptibility peak (near p_runaway~0.5, seen
    // around rho~0.60 in pilot runs) and the Binder crossing.
    let (rho0, rho1, step): (f64, f64, f64) = (0.440, 0.660, 0.005);
    let seed: u64 = 42;

    println!("# planar chase-escape finite-size scaling (over-spent count N_win)");
    println!(
        "# rho in [{:.3},{:.3}] step {:.4}; seed={}; runaway_cut={}",
        rho0, rho1, step, seed, RUNAWAY_CUT
    );
    println!("# x=N_win/N; m=<x>; chi=N*Var(x); U=1-<x^4>/(3<x^2>^2)");
    println!("L,N,rho,m,chi,binder,frac_contained,mean_contained,p_runaway");

    for (l, trials) in plan {
        let n = (l * l) as f64;
        let mut rho = rho0;
        while rho <= rho1 + 1e-9 {
            let (mut s1, mut s2, mut s4) = (0.0f64, 0.0f64, 0.0f64);
            let mut n_cont = 0u32;
            let mut sum_cont = 0.0f64;
            let rk = (rho * 1000.0).round() as u64;
            for t in 0..trials {
                let s = seed ^ ((l as u64) << 32) ^ (rk << 12) ^ (t as u64) ^ 0x2D2D_C0DE;
                let mut rng = StdRng::seed_from_u64(s);
                let nw = run_chase_escape_2d(l, rho, 1.0, &mut rng) as f64;
                let x = nw / n;
                s1 += x;
                s2 += x * x;
                s4 += x * x * x * x;
                if x <= RUNAWAY_CUT {
                    n_cont += 1;
                    sum_cont += nw;
                }
            }
            let tf = trials as f64;
            let m = s1 / tf;
            let m2 = s2 / tf;
            let m4 = s4 / tf;
            let var = (m2 - m * m).max(0.0);
            let chi = n * var;
            let binder = if m2 > 0.0 {
                1.0 - m4 / (3.0 * m2 * m2)
            } else {
                0.0
            };
            let frac_cont = n_cont as f64 / tf;
            let mean_cont = if n_cont > 0 {
                sum_cont / n_cont as f64
            } else {
                0.0
            };
            let p_run = 1.0 - frac_cont;
            println!(
                "{},{},{:.3},{:.6e},{:.6e},{:.6e},{:.4},{:.4},{:.4}",
                l,
                l * l,
                rho,
                m,
                chi,
                binder,
                frac_cont,
                mean_cont,
                p_run
            );
            rho += step;
        }
        eprintln!("  L={l} done ({trials} trials/point)");
    }
}
