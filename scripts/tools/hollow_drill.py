#!/usr/bin/env python3
"""Operator CLI for supervised hollowing + drain drilling. STL in, STL out.

    scripts/tools/hollow_drill.py in.stl out.stl [--wall 2.5] [--no-orient]
                                  [--report r.json] [--quiet]

The algorithm, its thresholds and its doctrine live in
``support_placement.hollow`` -- read that module's docstring before changing
any number. This file is argv, reporting, and the exit-code contract.

Runtime ~20s at 0.5mm pitch on a 500k-face model: run it in the FOREGROUND of
a single shell call. Needs no Blender.

EXIT CODES (this is the whole point of the split)
-------------------------------------------------
  0  wrote out.stl -- either hollowed and every step-8 check passed, or a
     legitimate SOLID decision, which is a success and exports the oriented
     mesh
  1  bad input: no such file, unreadable, not a watertight volume, or
     ALREADY HOLLOWED (this tool refuses to re-hollow -- it would discard the
     existing cavity; such a file needs the drill-only path). The JSON
     report's ``error_type`` distinguishes them: ``PreHollowedError`` is the
     already-hollowed case.
  2  hollowing produced a mesh that FAILED self-verification. NOTHING was
     written. stderr names each failed check and what it measured.

Before the split this tool exited 0 on a verification failure and said so
only inside a JSON blob on stdout, so an operator or a batch loop saw success
and no output file. That is the failure mode the exit codes above remove.

The JSON report (the same dict ``support_placement.hollow.run`` returns) goes
to stdout in every case, and additionally to ``--report PATH`` when given, so
the machine-readable record survives a failure too.
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from support_placement import hollow  # noqa: E402

EXIT_OK = 0
EXIT_INPUT = 1
EXIT_VERIFY_FAILED = 2


def build_parser():
    ap = argparse.ArgumentParser(
        prog="hollow_drill.py",
        description="Hollow (or decide not to), drill drains + a vent, verify, export.")
    ap.add_argument("input", help="source STL (watertight)")
    ap.add_argument("output", help="destination STL")
    ap.add_argument("--wall", type=float, default=2.5,
                    help="wall thickness in mm (default: %(default)s)")
    ap.add_argument("--no-orient", action="store_true",
                    help="skip the orientation search (keep the input's orientation)")
    ap.add_argument("--report", default=None, help="also write the JSON report here")
    ap.add_argument("--quiet", action="store_true",
                    help="do not print the JSON report to stdout")
    return ap


def main(argv=None):
    a = build_parser().parse_args(argv)
    log = {}
    code = EXIT_OK
    try:
        if not os.path.isfile(a.input):
            raise hollow.HollowError(f"no such STL: {a.input}")
        hollow.run(a.input, a.output, wall=a.wall, orient=not a.no_orient, log=log)
        if log.get("status") != hollow.STATUS_OK:
            code = EXIT_VERIFY_FAILED
            print(f"hollow_drill: FAILED self-verification on {a.input} — "
                  f"NOTHING was written to {a.output}", file=sys.stderr)
            for line in hollow.describe_failure(log):
                print(f"  FAILED CHECK {line}", file=sys.stderr)
            print(f"hollow_drill: failed checks: "
                  f"{', '.join(hollow.failed_checks(log)) or 'unknown'}", file=sys.stderr)
    except hollow.HollowError as e:
        code = EXIT_INPUT
        log.update(status="ERROR", error=str(e), error_type=type(e).__name__)
        print(f"hollow_drill: {e}", file=sys.stderr)
    except Exception as e:  # unreadable STL, boolean-engine blowup, ...
        code = EXIT_INPUT
        log.update(status="ERROR", error=str(e), error_type=type(e).__name__)
        print(f"hollow_drill: {type(e).__name__}: {e}", file=sys.stderr)
    finally:
        if not a.quiet:
            print(json.dumps(log, indent=2))
        if a.report:
            with open(a.report, "w") as fh:
                json.dump(log, fh, indent=2)
    return code


if __name__ == "__main__":
    sys.exit(main())
