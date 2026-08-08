"""ContainmentBench — a benchmark for containment / mitigation policies on
competing-diffusion (chase-escape) processes. Pure numpy; the Rust simulator in
`crates/sim` is the reference implementation these are validated against.

See docs/research/neurips/PLAN.md for the research/venue plan.
"""
from . import dynamics, topologies, metrics  # noqa: F401

__all__ = ["dynamics", "topologies", "metrics"]
__version__ = "0.0.1"
