"""SPEC-V5-PERF sec 5.1.4 (A1): the geometry-keyed verdict cache fuzz suite.

The cache's bit-identity claim (Identity theorem, sec 2.2) rests on: every
cached quantity is a pure function of member content + the immutable mesh +
the config fingerprint, so a content-key replay IS what a re-query returns.
This suite is the executable proof: cache-ON and cache-OFF
(perf_exact_opts_disable) revalidate must return identical violations AND leave
identical certificate state across randomized mutate/rollback sequences,
including negative-verdict replay, config-fingerprint mismatch, forced
eviction, and key hygiene (rollback re-hits, mutation re-keys).
"""
import numpy as np
import trimesh
import pytest

from support_placement import config
from support_placement import tree as tm
from support_placement.geometry_exact import ExactSurface


def _box(size, center=(0, 0, 0)):
    b = trimesh.creation.box(extents=size)
    b.apply_translation(center)
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(),
                           process=False)


def _soup(*meshes):
    m = trimesh.util.concatenate(list(meshes))
    return trimesh.Trimesh(vertices=m.vertices.copy(), faces=m.faces.copy(),
                           process=False)


def _fuzz_mesh():
    """A slab with two thin fins floating below it -- enough geometry that
    random members both clear and violate."""
    slab = _box((30, 30, 4), center=(0, 0, -2))          # top face z=0
    fin1 = _box((8, 8, 0.5), center=(0, 0, -3.0))
    fin2 = _box((8, 8, 0.5), center=(6, 0, -5.0))
    return _soup(slab, fin1, fin2)


def _cert_state(forest):
    return [(m.geom_version, m.cert_version, m.cert_radius,
             round(float(m.r0), 12), round(float(m.r1), 12)) for m in forest.members]


def _random_member(rng, oi):
    x = float(rng.uniform(-6, 6))
    y = float(rng.uniform(-6, 6))
    kind = rng.choice(["stub", "strut", "trunk", "shoot"])
    embed = rng.choice(["p0", "p1", None])
    p0 = np.array([x, y, 0.05])
    p1 = np.array([x + float(rng.uniform(-1, 1)), y + float(rng.uniform(-1, 1)),
                   float(rng.uniform(-8, -2))])
    r0 = float(rng.uniform(0.2, 1.5))
    r1 = float(rng.uniform(0.2, 1.5))
    return tm.Member(p0=p0, p1=p1, r0=r0, r1=r1, kind=kind,
                     owner=("contact", oi), embed_end=embed)


def _apply_op(forest, rng, oi_counter):
    """Apply one random structural op; return the op tag (for parity)."""
    op = rng.integers(0, 5)
    n = len(forest.members)
    if op == 0 or n == 0:
        forest.add_member(_random_member(rng, oi_counter[0]))
        oi_counter[0] += 1
        return "add"
    idx = int(rng.integers(0, n))
    m = forest.members[idx]
    if op == 1:
        m.r0 = float(rng.uniform(0.2, 1.6))
    elif op == 2:
        m.r1 = float(rng.uniform(0.2, 1.6))
    elif op == 3:
        m.p1 = m.p1 + np.array([0.0, 0.0, float(rng.uniform(-0.5, 0.5))])
    else:
        # embed_end / kind flip (A1a invalidating writes)
        m.embed_end = rng.choice(["p0", "p1", None])
    return "mut"


@pytest.mark.parametrize("seed", list(range(12)))
def test_cache_on_off_bit_identical_under_random_mutations(seed):
    """The core fuzz: identical random mutate/checkpoint/rollback/revalidate
    sequences on a cache-ON forest and a cache-OFF forest must produce
    identical violations and identical certificate state at every step."""
    geom = ExactSurface(_fuzz_mesh())
    cfg_on = config.SupportConfig()
    cfg_off = config.SupportConfig()
    cfg_off.perf_exact_opts_disable = True

    rng_on = np.random.default_rng(seed)
    rng_off = np.random.default_rng(seed)
    f_on, f_off = tm.Forest(), tm.Forest()
    oi_on, oi_off = [0], [0]

    for step in range(40):
        # identical op on both forests (same seed -> same rng stream)
        # occasionally wrap in a checkpoint/rollback
        wrap = (rng_on.integers(0, 3) == 0)
        _wrap_off = (rng_off.integers(0, 3) == 0)
        cp_on = f_on.checkpoint() if wrap else None
        cp_off = f_off.checkpoint() if _wrap_off else None
        _apply_op(f_on, rng_on, oi_on)
        _apply_op(f_off, rng_off, oi_off)
        v_on = f_on.revalidate(geom, cfg_on)
        v_off = f_off.revalidate(geom, cfg_off)
        assert v_on == v_off, (seed, step, "violations diverge", v_on, v_off)
        assert _cert_state(f_on) == _cert_state(f_off), (seed, step, "cert state")
        if wrap:
            f_on.rollback(cp_on)
        if _wrap_off:
            f_off.rollback(cp_off)
        # after rollback, another revalidate must still agree
        v_on2 = f_on.revalidate(geom, cfg_on)
        v_off2 = f_off.revalidate(geom, cfg_off)
        assert v_on2 == v_off2, (seed, step, "post-rollback violations")
        assert _cert_state(f_on) == _cert_state(f_off), (seed, step, "post-rollback cert")


def test_negative_verdict_replay_restamps_member_idx():
    """Two members with byte-identical geometry at different indices sharing a
    cached negative verdict each replay carrying their OWN member_idx (the
    geometry-valued fields are cached; member_idx is not)."""
    geom = ExactSurface(_fuzz_mesh())
    cfg = config.SupportConfig()
    f = tm.Forest()
    # a through-fin stub (welds through fin1 at z in [-3.25,-2.75]) at origin
    def mk():
        return tm.Member(p0=np.array([0.0, 0.0, 0.05]), p1=np.array([0.0, 0.0, -6.0]),
                         r0=0.2, r1=0.35, kind="stub", owner=("contact", 0),
                         embed_end="p0")
    f.add_member(mk())
    v1 = f.revalidate(geom, cfg)
    assert [x["member_idx"] for x in v1] == [0]
    f.add_member(mk())  # identical geometry, index 1
    v2 = f.revalidate(geom, cfg)
    # index 1 replays from the cached negative verdict but carries its own idx
    assert 1 in [x["member_idx"] for x in v2]
    rec0 = [x for x in v2 if x["member_idx"] == 0]
    rec1 = [x for x in v2 if x["member_idx"] == 1]
    assert rec1 and rec1[0]["min_clearance_mm"] == 0.0
    # geometry-valued fields identical between the two idx-distinct records
    if rec0:
        assert rec0[0]["min_clearance_mm"] == rec1[0]["min_clearance_mm"]


def test_config_fingerprint_mismatch_hard_fails():
    """A SupportConfig change on the same geom under a live cache is a hard
    failure, not a stale replay."""
    geom = ExactSurface(_fuzz_mesh())
    cfg = config.SupportConfig()
    f = tm.Forest()
    f.add_member(tm.Member(p0=np.array([0.0, 0.0, 0.05]), p1=np.array([2.0, 0.0, -5.0]),
                           r0=0.5, r1=0.5, kind="trunk", owner=("contact", 0)))
    f.revalidate(geom, cfg)
    cfg.standoff_mm = cfg.standoff_mm + 0.3      # mutate config mid-run
    with pytest.raises(AssertionError, match="verdict cache"):
        f.revalidate(geom, cfg)


def test_forced_eviction_preserves_results(monkeypatch):
    """A tiny cap forcing clear-half eviction must not change results (an
    evicted entry is simply recomputed)."""
    monkeypatch.setattr(tm, "_VERDICT_CACHE_CAP", 3)
    geom = ExactSurface(_fuzz_mesh())
    cfg_on = config.SupportConfig()
    cfg_off = config.SupportConfig()
    cfg_off.perf_exact_opts_disable = True
    rng_on = np.random.default_rng(99)
    rng_off = np.random.default_rng(99)
    f_on, f_off = tm.Forest(), tm.Forest()
    oi_on, oi_off = [0], [0]
    for _ in range(30):
        _apply_op(f_on, rng_on, oi_on)
        _apply_op(f_off, rng_off, oi_off)
        assert f_on.revalidate(geom, cfg_on) == f_off.revalidate(geom, cfg_off)
    assert f_on._verdict_cache["evictions"] > 0


def test_rollback_rehits_and_mutation_rekeys():
    """Key hygiene: mutating p0/p1 through __setattr__ makes a NEW key (no
    stale hit); rollback reverts the bytes so the OLD key hits again."""
    geom = ExactSurface(_fuzz_mesh())
    cfg = config.SupportConfig()
    f = tm.Forest()
    f.add_member(tm.Member(p0=np.array([0.0, 0.0, 0.05]), p1=np.array([2.0, 0.0, -5.0]),
                           r0=0.5, r1=0.5, kind="trunk", owner=("contact", 0)))
    f.revalidate(geom, cfg)                      # populate cache
    hits0 = f._verdict_cache["hits"]
    cp = f.checkpoint()
    f.members[0].p1 = np.array([3.0, 0.0, -5.0])  # mutate -> new key
    misses_before = f._verdict_cache["misses"]
    f.revalidate(geom, cfg)
    assert f._verdict_cache["misses"] > misses_before, "mutation must miss"
    f.rollback(cp)                                # revert bytes
    f.revalidate(geom, cfg)
    assert f._verdict_cache["hits"] > hits0, "rolled-back geometry must re-hit"


@pytest.mark.parametrize("threads", ["1", "8"])
def test_cache_thread_invariant(threads, monkeypatch):
    """Cached values are exact kernel outputs, so results are invariant to
    GEOM_KERNEL_THREADS (the kernel's own thread-invariance carries through)."""
    monkeypatch.setenv("GEOM_KERNEL_THREADS", threads)
    geom = ExactSurface(_fuzz_mesh())
    cfg = config.SupportConfig()
    rng = np.random.default_rng(7)
    f = tm.Forest()
    oi = [0]
    outs = []
    for _ in range(25):
        _apply_op(f, rng, oi)
        outs.append(f.revalidate(geom, cfg))
    # baseline (cache off) for the same sequence
    geom2 = ExactSurface(_fuzz_mesh())
    cfg2 = config.SupportConfig()
    cfg2.perf_exact_opts_disable = True
    rng2 = np.random.default_rng(7)
    f2 = tm.Forest()
    oi2 = [0]
    outs2 = []
    for _ in range(25):
        _apply_op(f2, rng2, oi2)
        outs2.append(f2.revalidate(geom2, cfg2))
    assert outs == outs2
