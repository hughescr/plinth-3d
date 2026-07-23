"""geom_kernel — the Rust ExactSurface hot core (de-Python stage 1).

The compiled extension is injected by maturin as ``geom_kernel._native``.
Production code never imports this package directly; it goes through
``support_placement.geometry_kernel.KernelSurface`` (spec sec 1.1).
"""
from . import _native  # noqa: F401  (re-export so `import geom_kernel` warms the ext)

__all__ = ["_native"]
