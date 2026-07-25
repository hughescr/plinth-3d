"""One mini, end to end: source STL -> printable ``.chitubox``.

    hollow/drill  ->  support engine  ->  emit

Each stage already exists as a library function with its own report
(:func:`support_placement.hollow.run`, :func:`support_placement.engine.run`,
:func:`support_placement.emit.emit_project`). This module is what composes
them, and -- more importantly -- what makes the composition's outcome
impossible to misread.

WHAT THIS MODULE IS FOR (the failure mode it removes)
-----------------------------------------------------
Every stage here can decline to produce output: hollowing can fail its own
self-verification and write nothing; the support engine can fail its gates;
the emit boundary can refuse a gate-failing forest or a frame mismatch.
Run by hand, each of those used to end as a line of prose in a log somewhere
and an exit status of 0 -- a run that looked like it worked and left no file.
So:

* :func:`prep_one` **always returns a** :class:`PipelineReport`, and that
  report always carries ``status``, ``exit_code`` and (on failure)
  ``failed_stage`` + ``error``. There is no code path that returns nothing.
* The report is **always written to disk** (default: beside the output as
  ``<stem>.pipeline.json``) -- including when the run failed, which is the
  case where you most need it.
* ``exit_code`` is a stable small integer per failure class, so a shell loop
  can branch on it, and :meth:`PipelineReport.raise_for_status` exists for
  callers that would rather have an exception than a return value to check.

THE ORIENTATION ORDER (why hollowing runs first)
------------------------------------------------
The orientation search is ON by default. Models arrive head-up but not
support-optimised, and orientation must be FINAL before drilling because
drain placement is gravity-dependent (holes go at each cavity lobe's lowest
point, along a downward exit ray). The support engine then runs on the
hollowed, drilled, re-rested mesh -- so it supports the geometry that will
actually be printed, drain holes and all -- and the emit boundary is handed
that same on-disk STL, which is what makes "the model in the file is the
model the supports were computed for" a shared variable rather than a claim.

THE INTERMEDIATE IS AN OUTPUT, NOT A TEMP FILE
----------------------------------------------
The oriented/hollowed/drilled STL is written next to the ``.chitubox`` as
``<stem>.prepped.stl`` and reported as ``prepped_stl``. It is not scratch:
it is the mesh the supports were computed against and the mesh embedded in
the project, so keeping it is what makes a result reproducible and
inspectable. (Generated output never belongs in the repo -- pick an output
directory outside it.)
"""
import json
import os
import time
from dataclasses import dataclass, field, asdict

from . import config as config_mod
from . import emit as emit_mod
from . import engine as engine_mod
from . import hollow as hollow_mod
from . import mesh_io

#: Stage names, in execution order. ``hollow`` is skipped (not failed) when
#: ``hollow=False``.
STAGES = ("hollow", "supports", "emit")

# --- exit codes -----------------------------------------------------------
# Chosen to agree with scripts/tools/emit_supports.py (0/1/3/4) so the two
# operator tools cannot mean different things by the same number.
EXIT_OK = 0
#: bad input: no such STL, unreadable mesh, bad arguments
EXIT_INPUT = 1
#: the hollow/drill stage failed (self-verification, or a bad mesh)
EXIT_HOLLOW = 2
#: the support engine's verify gates failed and --allow-failed-gates was not given
EXIT_GATES = 3
#: the emit boundary refused or the written file disagreed on read-back
EXIT_EMIT = 4
#: the support engine itself raised
EXIT_SUPPORTS = 5

STATUS_OK = "ok"
STATUS_SKIPPED = "skipped"
STATUS_FAILED = "failed"

#: Sentinel for "derive the report path from the output path". Passing
#: ``report_path=None`` explicitly disables writing one.
DEFAULT_REPORT = object()


class PipelineError(RuntimeError):
    """A stage declined to produce output. Carries the stage and exit code."""

    def __init__(self, stage, message, exit_code):
        super().__init__(f"[{stage}] {message}")
        self.stage = stage
        self.exit_code = exit_code


@dataclass(frozen=True)
class StageResult:
    """One stage's outcome plus whatever that stage measured about itself."""
    name: str
    status: str                      # ok | skipped | failed
    seconds: float = 0.0
    report: dict = field(default_factory=dict)
    error: str = None

    def to_dict(self):
        d = asdict(self)
        d["seconds"] = round(float(self.seconds), 2)
        return d


@dataclass(frozen=True)
class PipelineReport:
    """The whole run: what was asked, what each stage did, and the verdict.

    ``status`` is one of ``ok`` / ``skipped`` / ``failed``; ``exit_code`` is
    the number a CLI should exit with and is 0 for both ``ok`` and
    ``skipped``. On failure, ``failed_stage`` names the stage and ``error``
    carries its message.
    """
    input_stl: str
    out_path: str
    prepped_stl: str
    status: str
    exit_code: int
    stages: tuple
    seconds: float
    settings: dict
    failed_stage: str = None
    error: str = None
    report_path: str = None

    @property
    def ok(self):
        return self.status in (STATUS_OK, STATUS_SKIPPED)

    def stage(self, name):
        """The :class:`StageResult` for ``name``, or None if it never ran."""
        for s in self.stages:
            if s.name == name:
                return s
        return None

    def raise_for_status(self):
        """Raise :class:`PipelineError` unless the run succeeded or was
        skipped. For callers that would rather not check a return value."""
        if not self.ok:
            raise PipelineError(self.failed_stage or "pipeline",
                                self.error or "failed", self.exit_code)
        return self

    def to_dict(self):
        """JSON-safe dict. Every stage's own report is nested under
        ``stages[i].report`` exactly as that stage produced it."""
        return {
            "input_stl": self.input_stl,
            "out_path": self.out_path,
            "prepped_stl": self.prepped_stl,
            "status": self.status,
            "exit_code": int(self.exit_code),
            "failed_stage": self.failed_stage,
            "error": self.error,
            "seconds": round(float(self.seconds), 2),
            "settings": self.settings,
            "report_path": self.report_path,
            "stages": [s.to_dict() for s in self.stages],
        }

    def summary_line(self):
        """One line an operator can read: verdict, stage timings, headline
        numbers."""
        bits = [f"{self.status.upper()}", os.path.basename(self.input_stl)]
        if self.failed_stage:
            bits.append(f"failed_stage={self.failed_stage}")
        for s in self.stages:
            bits.append(f"{s.name}={s.status}/{s.seconds:.1f}s")
        bits.append(f"total={self.seconds:.1f}s")
        return "  ".join(bits)


def default_prepped_path(out_path):
    """``<out stem>.prepped.stl`` beside the ``.chitubox``."""
    stem, _ = os.path.splitext(os.fspath(out_path))
    return stem + ".prepped.stl"


def default_report_path(out_path):
    """``<out stem>.pipeline.json`` beside the ``.chitubox``."""
    stem, _ = os.path.splitext(os.fspath(out_path))
    return stem + ".pipeline.json"


def default_out_path(stl_path, out_dir):
    """``<out_dir>/<stl stem>.chitubox``."""
    stem = os.path.splitext(os.path.basename(os.fspath(stl_path)))[0]
    return os.path.join(os.fspath(out_dir), stem + ".chitubox")


def _gate_counts(verify):
    """Every list-valued gate in the engine's verify dict, as counts, plus
    ``ok``. Generic on purpose: a new gate shows up in the report without
    anyone remembering to add it here."""
    counts = {k: len(v) for k, v in verify.items() if isinstance(v, (list, tuple))}
    counts["ok"] = bool(verify.get("ok", False))
    return counts


def _write_report(path, payload):
    d = os.path.dirname(os.path.abspath(path))
    if d:
        os.makedirs(d, exist_ok=True)
    with open(path, "w") as fh:
        json.dump(payload, fh, indent=2)


def prep_one(stl_path, out_path, *, cfg=None, wall_mm=2.5, orient=True,
             hollow=True, prepped_stl=None, name=None, placement_xy=(0.0, 0.0),
             lift_mm=None, allow_failed_gates=False, skip_existing=False,
             report_path=DEFAULT_REPORT, progress=None):
    """Take one source STL all the way to a supported ``.chitubox``.

    Parameters
    ----------
    stl_path : str
        Source mesh. Must be a watertight volume (run the prep-mini repair
        playbook first if it is not).
    out_path : str
        Destination ``.chitubox``.
    cfg : config.SupportConfig, optional
        Engine + emission config. Defaults to ``SupportConfig(lift_mm=3.0)``,
        matching ``scripts/tools/emit_supports.py``.
    wall_mm : float
        Hollowing wall thickness. Ignored when ``hollow=False``.
    orient : bool
        Run the orientation search. ON by default: models arrive head-up but
        not support-optimised, and orientation must be final before drilling.
    hollow : bool
        Run the hollow/drill stage at all. With ``hollow=False`` the source
        STL is used unchanged and the stage reports ``skipped`` -- note that
        this also skips the orientation search, since that lives inside it.
    prepped_stl : str, optional
        Where to write the oriented/hollowed/drilled mesh. Default:
        :func:`default_prepped_path`. This file is an output, not a temp: it
        is the mesh the supports were computed against.
    name, placement_xy, lift_mm, allow_failed_gates
        Passed through to :func:`support_placement.emit.emit_project`.
    skip_existing : bool
        If ``out_path`` already exists and is non-empty, do no work and
        return a ``skipped`` report (``exit_code`` 0). This is what makes
        re-running over a folder cheap and idempotent.
    report_path : str or None
        Where to write the JSON report. Default: beside the output as
        ``<stem>.pipeline.json``. Pass ``None`` to write no file. The report
        is written on failure too -- that is the point of it.
    progress : callable, optional
        Called with one string per milestone. The library does not print.

    Returns
    -------
    PipelineReport
        Always. Stage failures are reported, not raised; call
        :meth:`PipelineReport.raise_for_status` if you want the exception.
    """
    say = progress or (lambda _msg: None)
    t_all = time.time()
    stl_path = os.fspath(stl_path)
    out_path = os.fspath(out_path)
    prepped = os.fspath(prepped_stl) if prepped_stl else default_prepped_path(out_path)
    if report_path is DEFAULT_REPORT:
        report_path = default_report_path(out_path)
    cfg = cfg if cfg is not None else config_mod.SupportConfig(lift_mm=3.0)
    settings = {
        "wall_mm": float(wall_mm), "orient": bool(orient), "hollow": bool(hollow),
        "lift_mm": float(cfg.lift_mm if lift_mm is None else lift_mm),
        "cfg_lift_mm": float(cfg.lift_mm),
        "p8_relocate_enable": bool(getattr(cfg, "p8_relocate_enable", False)),
        "allow_failed_gates": bool(allow_failed_gates),
        "skip_existing": bool(skip_existing),
    }
    stages = []

    def finish(status, exit_code, failed_stage=None, error=None):
        rep = PipelineReport(
            input_stl=stl_path, out_path=out_path, prepped_stl=prepped,
            status=status, exit_code=exit_code, stages=tuple(stages),
            seconds=time.time() - t_all, settings=settings,
            failed_stage=failed_stage, error=error, report_path=report_path)
        if report_path:
            _write_report(report_path, rep.to_dict())
        return rep

    if skip_existing and os.path.isfile(out_path) and os.path.getsize(out_path) > 0:
        say(f"skip: {out_path} already exists")
        return finish(STATUS_SKIPPED, EXIT_OK)

    if not os.path.isfile(stl_path):
        return finish(STATUS_FAILED, EXIT_INPUT, "input", f"no such STL: {stl_path}")

    out_dir = os.path.dirname(os.path.abspath(out_path))
    if out_dir:
        os.makedirs(out_dir, exist_ok=True)

    # ---- stage 1: hollow / drill (and orient) -----------------------------
    t0 = time.time()
    if hollow:
        say(f"hollow: {stl_path} -> {prepped} (wall={wall_mm}mm orient={orient})")
        hlog = {}
        try:
            hollow_mod.run(stl_path, prepped, wall=wall_mm, orient=orient, log=hlog)
        except Exception as e:
            stages.append(StageResult("hollow", STATUS_FAILED, time.time() - t0, hlog,
                                      f"{type(e).__name__}: {e}"))
            return finish(STATUS_FAILED, EXIT_HOLLOW, "hollow", f"{type(e).__name__}: {e}")
        if hlog.get("status") != hollow_mod.STATUS_OK:
            msg = ("hollowing failed self-verification (nothing written); failed checks: "
                   + (", ".join(hollow_mod.failed_checks(hlog)) or "unknown")
                   + ". " + " | ".join(hollow_mod.describe_failure(hlog)))
            stages.append(StageResult("hollow", STATUS_FAILED, time.time() - t0, hlog, msg))
            return finish(STATUS_FAILED, EXIT_HOLLOW, "hollow", msg)
        stages.append(StageResult("hollow", STATUS_OK, time.time() - t0, hlog))
        say(f"hollow: {hlog.get('decision')} solid={hlog.get('solid_ml')}ml "
            f"final={hlog.get('final_ml', hlog.get('solid_ml'))}ml "
            f"{time.time() - t0:.1f}s")
    else:
        prepped = stl_path
        stages.append(StageResult("hollow", STATUS_SKIPPED, 0.0,
                                  {"reason": "hollow=False (orientation search skipped too)"}))
        say("hollow: skipped (hollow=False)")

    # ---- stage 2: support engine ------------------------------------------
    t0 = time.time()
    say(f"supports: engine.run on {prepped}")
    try:
        mesh = mesh_io.load_stl(prepped)
        # The engine works in the model-centered frame; emit re-derives the
        # same center from the same STL, which is the model-fidelity contract.
        mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
        elog = {}
        result = engine_mod.run(mesh, cfg, log=elog)
    except Exception as e:
        stages.append(StageResult("supports", STATUS_FAILED, time.time() - t0, {},
                                  f"{type(e).__name__}: {e}"))
        return finish(STATUS_FAILED, EXIT_SUPPORTS, "supports", f"{type(e).__name__}: {e}")
    verify = result.get("verify") or {}
    srep = {
        "faces": int(len(mesh.faces)),
        "n_members": int(len(result["members"])),
        "n_balls": int(len(result["balls"])),
        "n_unrouted": int(len(result.get("unrouted", []))),
        "n_contacts": int(len(result.get("contacts", []))),
        "plate_z": float(result["plate_z"]),
        "gates": _gate_counts(verify),
        "volume_ml": {k: round(float(v), 5)
                      for k, v in (result.get("volume") or {}).items()
                      if isinstance(v, (int, float))},
        "stage_timings": {k: elog[k] for k in elog if k.startswith("t_")},
    }
    stages.append(StageResult("supports", STATUS_OK, time.time() - t0, srep))
    say(f"supports: members={srep['n_members']} balls={srep['n_balls']} "
        f"unrouted={srep['n_unrouted']} verify.ok={srep['gates']['ok']} "
        f"{time.time() - t0:.1f}s")

    # ---- stage 3: emit -----------------------------------------------------
    t0 = time.time()
    say(f"emit: {prepped} + forest -> {out_path}")
    # Name the model after the SOURCE mini, not after the intermediate --
    # "Morel Mudskipper", not "Morel Mudskipper.prepped".
    model_name = (name if name is not None
                  else os.path.splitext(os.path.basename(stl_path))[0][:63])
    try:
        erep = emit_mod.emit_project(prepped, result, out_path, cfg, name=model_name,
                                     placement_xy=tuple(placement_xy),
                                     lift_mm=lift_mm,
                                     allow_failed_gates=allow_failed_gates)
    except emit_mod.EmitRefused as e:
        stages.append(StageResult("emit", STATUS_FAILED, time.time() - t0,
                                  {"gates": srep["gates"]}, str(e)))
        return finish(STATUS_FAILED, EXIT_GATES, "emit", str(e))
    except emit_mod.EmitError as e:
        stages.append(StageResult("emit", STATUS_FAILED, time.time() - t0, {}, str(e)))
        return finish(STATUS_FAILED, EXIT_EMIT, "emit", str(e))
    except Exception as e:
        stages.append(StageResult("emit", STATUS_FAILED, time.time() - t0, {},
                                  f"{type(e).__name__}: {e}"))
        return finish(STATUS_FAILED, EXIT_EMIT, "emit", f"{type(e).__name__}: {e}")
    stages.append(StageResult("emit", STATUS_OK, time.time() - t0, erep.to_dict()))
    say(f"emit: {erep.n_records} records, {erep.bytes_written} bytes "
        f"{time.time() - t0:.1f}s")

    return finish(STATUS_OK, EXIT_OK)


def prep_batch(stl_paths, out_dir, *, progress=None, **kwargs):
    """Run :func:`prep_one` over several models into one output directory.

    Returns the list of :class:`PipelineReport`, in input order. One model's
    failure never stops the others -- the caller decides what to do with the
    mix, and ``max(r.exit_code for r in reports)`` is the batch verdict.

    ``out_path`` is derived per model by :func:`default_out_path`; every other
    keyword goes straight through to :func:`prep_one` (notably
    ``skip_existing``, which is what makes re-running a folder cheap).
    """
    kwargs.pop("out_path", None)
    reports = []
    for p in stl_paths:
        reports.append(prep_one(p, default_out_path(p, out_dir),
                                progress=progress, **kwargs))
    return reports
