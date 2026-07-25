import sys
import os
import glob
import hashlib
import struct

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import pytest
from support_placement import mesh_io, config


# --------------------------------------------------------------------------
# Rust-extension staleness gates (spec SPEC-RUST-KERNEL.md sec 1.3). Each
# applies ONLY to tests carrying its marker; the rest of the suite is
# untouched. A runtest hook (not an autouse fixture) applies the gate to
# exactly the marked items so a missing/stale extension FAILS those tests
# loudly instead of skipping.
#
#   @pytest.mark.kernel    -> rust/geom_kernel  -> geom_kernel._native
#   @pytest.mark.chitubox  -> rust/chitubox_io  -> chitubox_io._native
# --------------------------------------------------------------------------
_RUST_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "rust"))
_CRATE = os.path.join(_RUST_DIR, "geom_kernel")
_CHITUBOX_CRATE = os.path.join(_RUST_DIR, "chitubox_io")


def _crate_source_digest(crate_dir):
    """Reproduce a crate's build.rs SOURCE_DIGEST: SHA-256 over sorted
    src/*.rs then Cargo.toml, Cargo.lock, build.rs, pyproject.toml — each
    framed as basename + b'\\0' + u64-le(len) + bytes. Both crates' build.rs
    files run byte-for-byte the same algorithm."""
    files = sorted(glob.glob(os.path.join(crate_dir, "src", "*.rs")))
    for name in ("Cargo.toml", "Cargo.lock", "build.rs", "pyproject.toml"):
        p = os.path.join(crate_dir, name)
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


def _kernel_source_digest():
    return _crate_source_digest(_CRATE)


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


def _chitubox_gate_reason():
    """Same contract as _kernel_gate_reason for the chitubox_io extension:
    missing, API-mismatched, or older-than-source => FAIL the chitubox-marked
    tests. No silent skip — a stale binding is exactly how a wrong-model emit
    would sneak past a green suite."""
    if os.environ.get("CHITUBOX_IO_OPTIONAL") == "1":
        try:
            import chitubox_io._native  # noqa: F401
        except ImportError:
            return None, "chitubox_io not built (CHITUBOX_IO_OPTIONAL=1 -> skip)"
    try:
        import chitubox_io._native as _cnative
    except ImportError:
        return False, ("chitubox_io._native not built. Build it with:\n"
                       "  scripts/build_kernel.sh chitubox_io\n"
                       "  (or cd rust/chitubox_io && ../../.venv/bin/maturin develop --release)")
    from support_placement import emit as _emit
    if _cnative.API_VERSION != _emit.EXPECTED_CHITUBOX_API_VERSION:
        return False, (f"chitubox_io API version {_cnative.API_VERSION} != "
                       f"expected {_emit.EXPECTED_CHITUBOX_API_VERSION} — rebuild: "
                       "scripts/build_kernel.sh chitubox_io")
    try:
        cur = _crate_source_digest(_CHITUBOX_CRATE)
    except OSError:
        cur = None
    if cur is not None and cur != _cnative.SOURCE_DIGEST:
        if os.environ.get("CHITUBOX_IO_ALLOW_STALE") == "1":
            print("WARNING: chitubox_io binding older than rust sources "
                  "(CHITUBOX_IO_ALLOW_STALE=1)", file=sys.stderr)
        else:
            return False, ("chitubox_io binding older than rust sources — rebuild: "
                           "scripts/build_kernel.sh chitubox_io  "
                           "(dev escape: CHITUBOX_IO_ALLOW_STALE=1)")
    return True, None


_GATE_CACHE = {}
_GATES = {"kernel": _kernel_gate_reason, "chitubox": _chitubox_gate_reason}


def _gate(name="kernel"):
    if name not in _GATE_CACHE:
        _GATE_CACHE[name] = _GATES[name]()
    return _GATE_CACHE[name]


def pytest_addoption(parser):  # noqa: F811 - pytest hook
    # Anti-drift frozen parity baseline (spec sec 6 item 6, Amendment A20).
    # The frozen oracle hashes in fixtures/kernel_parity.json are NEVER
    # auto-created: "first freeze" (or an intentional re-freeze in its own
    # commit with a written rationale) happens only via this explicit flag.
    parser.addoption(
        "--freeze-kernel-baseline", action="store_true", default=False,
        help="Recompute and WRITE scripts/tests/fixtures/kernel_parity.json "
             "(frozen oracle-side answer hashes + env stamp). Use only for an "
             "intentional first-freeze/re-freeze, in its own commit.")


def pytest_configure(config):  # noqa: F811 - pytest hook
    config.addinivalue_line("markers", "kernel: differential/parity tests against "
                            "the geom_kernel Rust extension (spec SPEC-RUST-KERNEL.md)")
    config.addinivalue_line("markers", "chitubox: tests against the chitubox_io "
                            "Rust extension (.chitubox container read/write)")


def pytest_runtest_setup(item):
    for name in _GATES:
        if item.get_closest_marker(name) is None:
            continue
        ok, reason = _gate(name)
        if ok is True:
            continue
        if ok is None:  # optional-skip (<EXT>_OPTIONAL=1)
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
