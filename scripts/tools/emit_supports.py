#!/usr/bin/env python3
"""Operator entry point for the emit path: one STL -> one supported
``.chitubox``, built from an EMPTY template.

    scripts/tools/emit_supports.py <model.stl> <out.chitubox> [options]

This is the whole of what ``scripts/tests/manual_emit_treant.py`` used to do
by hand -- run the engine on the centered mesh, synthesize the support
geometry, write the container -- minus the parts that made it glue: no
/tmp checkpoint pickles, no cloned corpus project, no MST raft, no output
paths pointing outside the repo by construction. The engine call and the emit
call are both library functions (``support_placement.engine.run``,
``support_placement.emit.emit_project``); this file only parses argv, prints
progress to stderr, and prints one JSON report to stdout.

stdout is EXACTLY one line of JSON (the ``EmitReport``) on success and nothing
otherwise, so a batch loop can consume it. Progress and warnings go to stderr.

Exit codes:
  0  wrote the file, all gates passed
  1  usage / bad input (missing STL, unreadable mesh)
  3  support gates failed and --allow-failed-gates was not given
  4  emit failed (container refused the write, or the read-back disagreed)
"""
import argparse
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from support_placement import config, emit, engine, mesh_io  # noqa: E402


def build_parser():
    p = argparse.ArgumentParser(
        prog="emit_supports.py",
        description="Run the support engine on an STL and emit a .chitubox.")
    p.add_argument("stl", help="source STL (binary)")
    p.add_argument("out", help="destination .chitubox")
    p.add_argument("--name", default=None,
                   help="model name inside the project (default: STL basename)")
    p.add_argument("--lift-mm", type=float, default=3.0,
                   help="plate lift; the canonical value is 3.0 (default: %(default)s)")
    p.add_argument("--place", nargs=2, type=float, metavar=("X", "Y"),
                   default=(0.0, 0.0), help="plate translation (default: 0 0)")
    p.add_argument("--allow-failed-gates", action="store_true",
                   help="emit even when the engine's verify gates failed")
    p.add_argument("--report", default=None,
                   help="also write the JSON report to this path")
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)

    if not os.path.isfile(args.stl):
        print(f"error: no such STL: {args.stl}", file=sys.stderr)
        return 1

    t0 = time.time()
    print(f"[emit] loading {args.stl}", file=sys.stderr, flush=True)
    try:
        mesh = mesh_io.load_stl(args.stl)
    except Exception as e:
        print(f"error: could not load {args.stl}: {e}", file=sys.stderr)
        return 1
    # The engine works in the model-centered frame; chitubox_io centers the
    # model the same way, which is what makes the STL PATH the whole
    # model-fidelity contract (see support_placement.emit).
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    print(f"[emit] mesh: {len(mesh.faces)} faces "
          f"bounds={mesh.bounds.tolist()}", file=sys.stderr, flush=True)

    cfg = config.SupportConfig(lift_mm=args.lift_mm)
    log = {}
    result = engine.run(mesh, cfg, log=log)
    print(f"[emit] engine.run {time.time() - t0:.1f}s "
          f"members={len(result['members'])} balls={len(result['balls'])} "
          f"verify.ok={result['verify']['ok']}", file=sys.stderr, flush=True)

    try:
        report = emit.emit_project(
            args.stl, result, args.out, cfg, name=args.name,
            placement_xy=tuple(args.place),
            allow_failed_gates=args.allow_failed_gates)
    except emit.EmitRefused as e:
        print(f"error: {e}", file=sys.stderr)
        return 3
    except emit.EmitError as e:
        print(f"error: {e}", file=sys.stderr)
        return 4

    payload = report.to_dict()
    payload["engine_seconds"] = round(time.time() - t0, 2)
    if args.report:
        with open(args.report, "w") as fh:
            json.dump(payload, fh, indent=2)
    print(json.dumps(payload))
    return 0


if __name__ == "__main__":
    sys.exit(main())
