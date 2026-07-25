#!/usr/bin/env python3
"""Operator entry point for the whole pipeline: STL in, ``.chitubox`` out.

    scripts/tools/prep_one.py <model.stl> <out.chitubox> [options]
    scripts/tools/prep_one.py --out-dir DIR <model.stl|dir> ... [options]

One mini at a time, hollow/drill -> support engine -> emit, composed by
``support_placement.pipeline``. The second form runs the same thing over
several models (a directory expands to its top-level ``*.stl``) so a whole
folder is one process instead of one process per file; pair it with
``--skip-existing`` to make re-running the folder cheap and idempotent.

Orientation search is ON by default. Models arrive head-up but not
support-optimised, and orientation must be final before drilling because
drain placement is gravity-dependent. ``--no-orient`` keeps the input's
orientation; ``--no-hollow`` skips the whole stage (and with it the
orientation search).

OUTPUT CONTRACT
---------------
stdout: exactly one line of JSON per input, ALWAYS -- success, skip or
failure. The exit code carries the verdict; the JSON carries the detail. A
batch driver can read stdout as JSONL and never has to parse prose. Progress
and failure messages go to stderr.

Each run also leaves ``<out stem>.pipeline.json`` beside its output (disable
with ``--no-report-file``, relocate with ``--report`` in single-model mode)
and ``<out stem>.prepped.stl``, the oriented/hollowed/drilled mesh the
supports were actually computed against.

EXIT CODES (the batch verdict is the worst code over all inputs)
----------------------------------------------------------------
  0  wrote the file (or skipped it because it already existed)
  1  bad input / usage: no such STL, unreadable mesh, bad arguments
  2  hollow/drill stage failed -- self-verification failed, nothing written
  3  support gates failed and --allow-failed-gates was not given
  4  emit failed -- the container refused, or the read-back disagreed
  5  the support engine itself raised

These agree with scripts/tools/emit_supports.py (0/1/3/4) and
scripts/tools/hollow_drill.py (0/1/2) so no number means two things.
"""
import argparse
import glob
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from support_placement import config, pipeline  # noqa: E402


def build_parser():
    p = argparse.ArgumentParser(
        prog="prep_one.py",
        description="Hollow/drill, support and emit one mini (or a folder of them).",
        epilog="Give <stl> <out.chitubox> for one model, or --out-dir DIR with "
               "any number of STL files and/or directories.")
    p.add_argument("inputs", nargs="+", metavar="INPUT",
                   help="source STL(s), or directories of them with --out-dir; "
                        "without --out-dir, exactly two: <model.stl> <out.chitubox>")
    p.add_argument("--out-dir", default=None,
                   help="write <stem>.chitubox here for every input (batch mode)")
    p.add_argument("--wall", type=float, default=2.5,
                   help="hollowing wall thickness in mm (default: %(default)s)")
    p.add_argument("--no-orient", action="store_true",
                   help="skip the orientation search inside the hollow stage")
    p.add_argument("--no-hollow", action="store_true",
                   help="skip hollow/drill entirely (also skips orientation)")
    p.add_argument("--lift-mm", type=float, default=3.0,
                   help="plate lift; the canonical value is 3.0 (default: %(default)s)")
    p.add_argument("--p8-relocate", action="store_true",
                   help="enable the Phase-8 relocation stage (cfg.p8_relocate_enable)")
    p.add_argument("--name", default=None,
                   help="model name inside the project (single-model mode only)")
    p.add_argument("--place", nargs=2, type=float, metavar=("X", "Y"),
                   default=(0.0, 0.0), help="plate translation (default: 0 0)")
    p.add_argument("--allow-failed-gates", action="store_true",
                   help="emit even when the engine's verify gates failed")
    p.add_argument("--skip-existing", action="store_true",
                   help="do no work when the .chitubox already exists (idempotent re-runs)")
    p.add_argument("--report", default=None,
                   help="write the pipeline JSON report here (single-model mode only)")
    p.add_argument("--no-report-file", action="store_true",
                   help="do not write the <stem>.pipeline.json sidecar")
    p.add_argument("--quiet", action="store_true", help="no progress on stderr")
    return p


def expand_inputs(inputs):
    """Files stay as they are; directories expand to their top-level *.stl,
    sorted. Deduplicated, order preserved."""
    out = []
    for item in inputs:
        if os.path.isdir(item):
            out.extend(sorted(glob.glob(os.path.join(item, "*.stl"))))
        else:
            out.append(item)
    seen = set()
    return [p for p in out if not (p in seen or seen.add(p))]


def main(argv=None):
    args = build_parser().parse_args(argv)
    batch = args.out_dir is not None
    if not batch and len(args.inputs) != 2:
        print("error: without --out-dir, give exactly two positionals: "
              "<model.stl> <out.chitubox>", file=sys.stderr)
        return pipeline.EXIT_INPUT
    if batch and (args.name or args.report):
        print("error: --name and --report are single-model options; "
              "they cannot be combined with --out-dir", file=sys.stderr)
        return pipeline.EXIT_INPUT

    say = (lambda _m: None) if args.quiet else (
        lambda m: print(f"[prep] {m}", file=sys.stderr, flush=True))

    cfg = config.SupportConfig(lift_mm=args.lift_mm,
                               p8_relocate_enable=args.p8_relocate)
    common = dict(
        cfg=cfg, wall_mm=args.wall, orient=not args.no_orient,
        hollow=not args.no_hollow, placement_xy=tuple(args.place),
        allow_failed_gates=args.allow_failed_gates,
        skip_existing=args.skip_existing, progress=say)

    if batch:
        models = expand_inputs(args.inputs)
        if not models:
            print(f"error: no STL files found in {args.inputs}", file=sys.stderr)
            return pipeline.EXIT_INPUT
        report_kw = {"report_path": None} if args.no_report_file else {}
        reports = pipeline.prep_batch(models, args.out_dir, **common, **report_kw)
    else:
        stl, out = args.inputs
        if args.no_report_file:
            report_kw = {"report_path": None}
        elif args.report:
            report_kw = {"report_path": args.report}
        else:
            report_kw = {}
        reports = [pipeline.prep_one(stl, out, name=args.name, **common, **report_kw)]

    code = pipeline.EXIT_OK
    for rep in reports:
        print(json.dumps(rep.to_dict()), flush=True)
        if not rep.ok:
            print(f"[prep] FAILED {rep.input_stl} in stage '{rep.failed_stage}' "
                  f"(exit {rep.exit_code}): {rep.error}", file=sys.stderr, flush=True)
        say(rep.summary_line())
        code = max(code, rep.exit_code)
    if len(reports) > 1:
        n_ok = sum(1 for r in reports if r.status == pipeline.STATUS_OK)
        n_skip = sum(1 for r in reports if r.status == pipeline.STATUS_SKIPPED)
        say(f"batch: {n_ok} written, {n_skip} skipped, "
            f"{len(reports) - n_ok - n_skip} failed -> exit {code}")
    return code


if __name__ == "__main__":
    sys.exit(main())
