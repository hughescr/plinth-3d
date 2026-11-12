//! Runs every rule measurement on one file and returns the nine rule results.

use crate::chitubox::ChituFile;
use crate::json::Json;
use crate::limits::{self, Limits};
use crate::outcome::Outcome;
use crate::report::{RULES, RuleResult, rules_unverified};
use crate::scene::Scene;
use crate::stl::Stl;
use crate::{h3, h4, h5, h6, h7, h9, h12, patch};

const H8_REASON: &str =
    "H8 is a record that ChituBox opened and sliced this exact file; the checker cannot measure it";

fn result(id: &'static str, name: &'static str, o: Outcome) -> RuleResult {
    RuleResult {
        id,
        name,
        status: o.status,
        reason: o.reason,
        located: o.located,
        detail: o.detail,
    }
}

/// The nine rule results for model 0 of `file`, or all unverified with the reason when the
/// file is not one this checker measures.
#[must_use]
pub fn run(
    limits: &Limits,
    threads: usize,
    file: &ChituFile,
    buf: &[u8],
    stl: Option<&Stl>,
    pose: &h7::Pose,
) -> Vec<RuleResult> {
    run_full(limits, threads, file, buf, stl, pose).0
}

/// The rule results and the load audit (a report, never a rule; `Json::Null` when the file is
/// not measured).
#[must_use]
pub fn run_full(
    limits: &Limits,
    threads: usize,
    file: &ChituFile,
    buf: &[u8],
    stl: Option<&Stl>,
    pose: &h7::Pose,
) -> (Vec<RuleResult>, Json) {
    run_full_declared(
        limits,
        Options {
            threads,
            declared_hollow: false,
        },
        file,
        buf,
        stl,
        pose,
    )
}

/// Explicit measurement applicability and execution settings.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub threads: usize,
    pub declared_hollow: bool,
}

/// Run with explicit hollow applicability, preserving the undeclared API's old behaviour.
#[must_use]
pub fn run_full_declared(
    limits: &Limits,
    options: Options,
    file: &ChituFile,
    buf: &[u8],
    stl: Option<&Stl>,
    pose: &h7::Pose,
) -> (Vec<RuleResult>, Json) {
    if file.models.len() != 1 {
        return (
            rules_unverified(&format!(
                "the file holds {} models; the checker measures files with exactly one",
                file.models.len()
            )),
            Json::Null,
        );
    }
    let mut scene = match Scene::build(limits, options.threads, file, buf, 0) {
        Ok(s) => s,
        Err(e) => {
            return (
                rules_unverified(&format!("the model cannot be read: {e}")),
                Json::Null,
            );
        }
    };
    scene.declared_hollow = options.declared_hollow;
    if scene.model.is_empty() || scene.translate.iter().any(|x| !x.is_finite()) {
        return (
            rules_unverified("the model has no triangles or a translate that is not finite"),
            Json::Null,
        );
    }
    // Every vertex, in plate coordinates, must be finite and inside the build volume: the
    // bounds this checker states hold for a file of that size and no larger.
    let outside = scene
        .model
        .iter()
        .flatten()
        .map(|p| ("model", *p))
        .chain(
            scene
                .parts
                .iter()
                .flat_map(|p| p.tris.iter().flatten())
                .map(|p| ("support", *p)),
        )
        .map(|(what, p)| (what, scene.world(p)))
        .find(|(_, w)| !limits::in_build_volume(*w));
    if let Some((what, w)) = outside {
        return (
            rules_unverified(&format!(
                "a {what} coordinate ({:.3}, {:.3}, {:.3}) mm is not finite or lies outside the printer's build volume (|x| <= {}, |y| <= {}, {} <= z <= {} mm in plate coordinates); the file is refused, not measured",
                w[0],
                w[1],
                w[2],
                limits::BUILD_X_MM,
                limits::BUILD_Y_MM,
                limits::BUILD_Z_MM.0,
                limits::BUILD_Z_MM.1
            )),
            Json::Null,
        );
    }
    run_scene_full(&scene, stl, pose)
}

/// The nine rule results for a built scene, with no recorded rotation.
#[must_use]
pub fn run_scene(scene: &Scene, stl: Option<&Stl>) -> Vec<RuleResult> {
    run_scene_with(scene, stl, &h7::Pose::default())
}

/// The nine rule results for a built scene, with the STL under the recorded pose.
#[must_use]
pub fn run_scene_with(scene: &Scene, stl: Option<&Stl>, pose: &h7::Pose) -> Vec<RuleResult> {
    run_scene_full(scene, stl, pose).0
}

/// The rule results and the load audit for a built scene.
#[must_use]
pub fn run_scene_full(
    scene: &Scene,
    stl: Option<&Stl>,
    pose: &h7::Pose,
) -> (Vec<RuleResult>, Json) {
    let patches = patch::measure(scene);
    let audit = crate::audit::run(scene, &patches);
    let h7 = stl.map_or_else(
        || Outcome::unverified("no input STL was given, so the model cannot be compared with one"),
        |s| h7::measure_with(scene, s, pose),
    );
    let outcomes = [
        h12::h1(scene, &patches),
        h12::h2(scene, &patches),
        h3::measure(scene, &patches),
        h4::measure(scene),
        h5::measure(scene),
        h6::measure(scene, &patches),
        h7,
        Outcome::unverified(H8_REASON),
        h9::measure(scene),
    ];
    let rules = RULES
        .iter()
        .zip(outcomes)
        .map(|((id, name), o)| result(id, name, o))
        .collect();
    (rules, audit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Status;
    use crate::testkit::{self as kit, test_limits};
    use crate::tri::Tri;

    #[test]
    fn a_model_only_scene_has_h5_not_applicable_h4_pass_and_nothing_passing_without_a_measurement()
    {
        // A tetrahedron-like closed model lifted 5 mm.
        let (o, x, y, z) = (
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [0.0, 6.0, 0.0],
            [0.0, 0.0, 8.0],
        );
        let model: Vec<Tri> = vec![[o, y, x], [o, x, z], [o, z, y], [x, y, z]];
        let scene = Scene::from_parts(&test_limits(), model, [0.0, 0.0, 5.0], Vec::new());
        let r = run_scene(&scene, None);
        let st = |id: &str| r.iter().find(|x| x.id == id).map(|x| x.status);
        assert_eq!(st("H4"), Some(Status::Pass));
        assert_eq!(st("H5"), Some(Status::NotApplicable));
        for id in ["H1", "H2", "H6", "H9"] {
            assert_eq!(st(id), Some(Status::NotApplicable), "{id}");
        }
        for id in ["H3", "H7", "H8"] {
            assert_eq!(st(id), Some(Status::Unverified), "{id}");
        }
        assert!(r.iter().all(|x| !x.reason.is_empty()));
        let _ = kit::test_limits();
    }
}
