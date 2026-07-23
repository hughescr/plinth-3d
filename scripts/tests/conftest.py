import sys
import os
import glob
import hashlib
import struct

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import pytest
from support_placement import mesh_io, config


# --------------------------------------------------------------------------
# geom_kernel gate (spec SPEC-RUST-KERNEL.md sec 1.3). Applies ONLY to tests
# marked @pytest.mark.kernel; the rest of the suite is untouched. A collection
# hook (not an autouse fixture) attaches the gate outcome to exactly the marked
# items so a missing/stale kernel FAILS those tests loudly instead of skipping.
# --------------------------------------------------------------------------
_CRATE = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..",
                                      "rust", "geom_kernel"))


def _kernel_source_digest():
    """Reproduce build.rs's SOURCE_DIGEST: SHA-256 over sorted src/*.rs then
    Cargo.toml, Cargo.lock, build.rs, pyproject.toml — each framed as
    basename + b'\\0' + u64-le(len) + bytes."""
    files = sorted(glob.glob(os.path.join(_CRATE, "src", "*.rs")))
    for name in ("Cargo.toml", "Cargo.lock", "build.rs", "pyproject.toml"):
        p = os.path.join(_CRATE, name)
        if os.path.exists(p):
            files.append(p)
    h = hashlib.sha256()
    for f in files:
        with open(f, "rb") as fh:
            data = fh.read()
        h.update(os.path.basename(f).encode())
        h.update(b"\x00")
        h.update(struct.pack("<Q", len(data)))
        h.update(data)
    return h.hexdigest()


def _kernel_gate_reason():
    """Return (ok, reason). ok=True means the marked tests may run."""
    if os.environ.get("GEOM_KERNEL_OPTIONAL") == "1":
        try:
            import geom_kernel._native  # noqa: F401
        except ImportError:
            return None, ("geom_kernel not built (GEOM_KERNEL_OPTIONAL=1 -> skip)")
    try:
        import geom_kernel._native as _native
    except ImportError:
        return False, ("geom_kernel._native not built. Build it with:\n"
                       "  scripts/build_kernel.sh\n"
                       "  (or cd rust/geom_kernel && ../../.venv/bin/maturin develop --release)")
    from support_placement import geometry_kernel as GK
    if _native.KERNEL_API_VERSION != GK.EXPECTED_API_VERSION:
        return False, (f"kernel API version {_native.KERNEL_API_VERSION} != "
                       f"expected {GK.EXPECTED_API_VERSION} — rebuild: scripts/build_kernel.sh")
    try:
        cur = _kernel_source_digest()
    except OSError:
        cur = None
    if cur is not None and cur != _native.SOURCE_DIGEST:
        if os.environ.get("GEOM_KERNEL_ALLOW_STALE") == "1":
            print("WARNING: geom_kernel binary older than rust sources "
                  "(GEOM_KERNEL_ALLOW_STALE=1)", file=sys.stderr)
        else:
            return False, ("kernel binary older than rust sources — rebuild: "
                           "scripts/build_kernel.sh  (dev escape: GEOM_KERNEL_ALLOW_STALE=1)")
    return True, None


_GATE_CACHE = None


def _gate():
    global _GATE_CACHE
    if _GATE_CACHE is None:
        _GATE_CACHE = _kernel_gate_reason()
    return _GATE_CACHE


def pytest_configure(config):  # noqa: F811 - pytest hook
    config.addinivalue_line("markers", "kernel: differential/parity tests against "
                            "the geom_kernel Rust extension (spec SPEC-RUST-KERNEL.md)")


def pytest_runtest_setup(item):
    if item.get_closest_marker("kernel") is None:
        return
    ok, reason = _gate()
    if ok is True:
        return
    if ok is None:  # optional-skip (GEOM_KERNEL_OPTIONAL=1)
        pytest.skip(reason)
    else:  # missing/stale/version-mismatch => FAIL, never silently skip
        pytest.fail(reason)


@pytest.fixture
def cfg():
    return config.SupportConfig()


@pytest.fixture
def box_mesh():
    return mesh_io.make_box(size=(10, 10, 10))


@pytest.fixture
def table_mesh():
    return mesh_io.make_overhang_table()


@pytest.fixture
def fin_mesh():
    return mesh_io.make_thin_fin()


@pytest.fixture
def toed_slab_mesh():
    return mesh_io.make_toed_slab()


@pytest.fixture
def raking_blade_mesh():
    return mesh_io.make_raking_blade()
