//! Strict, bounded JSON input. Pose is a translation; rotated coupons are not CAL-1.
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlateSpec {
    pub label: String,
    pub band: String,
    pub shuffle_seed: u64,
    pub head: Head,
    pub specimens: Vec<Specimen>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Head {
    pub ball_diameter: f32,
    pub outside: f32,
    pub waist_length: f32,
    pub junction_diameter: f32,
    pub cone_start_diameter: f32,
    pub cone_end_diameter: f32,
    pub cone_length: f32,
    pub column_diameter: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Specimen {
    pub id: String,
    pub body: Body,
    pub pose: [f32; 2],
    pub lift: f32,
    pub contact_sites: Vec<[f32; 2]>,
    pub waist_diameters: Vec<f32>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub enum Body {
    Block,
    Strip,
    Witness,
    K15,
    K60,
}

impl PlateSpec {
    /// Reject drift from the preregistered pieces rather than silently relabel a rung.
    pub fn validate(&self) -> crate::Result<()> {
        let h = &self.head;
        let pairs = [
            (h.ball_diameter, 0.5),
            (h.outside, 0.12),
            (h.waist_length, 0.71),
            (h.junction_diameter, 0.416),
            (h.cone_start_diameter, 0.40),
            (h.cone_end_diameter, 1.0),
            (h.cone_length, 1.5),
            (h.column_diameter, 1.0),
        ];
        if self.label != crate::LABEL
            || self.band.is_empty()
            || self.specimens.len() != 6
            || pairs
                .iter()
                .any(|(a, b)| !a.is_finite() || (a - b).abs() > 1e-6)
        {
            return Err("spec must name the six CAL-1 pieces and the fixed head dimensions".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for s in &self.specimens {
            let (name, body, sites, waists) = expected(&s.id)?;
            if !ids.insert(name)
                || s.body != body
                || s.contact_sites != sites
                || s.waist_diameters != waists
                || !s.pose.iter().all(|x| x.is_finite() && x.abs() < 75.0)
                || !s.lift.is_finite()
                || (s.lift
                    - if matches!(body, Body::K15 | Body::K60) {
                        0.0
                    } else {
                        5.0
                    })
                .abs()
                    > 1e-6
            {
                return Err(format!(
                    "{}: body, lift, contacts or waist rung differ from CAL-1",
                    s.id
                )
                .into());
            }
        }
        Ok(())
    }
}

type Expected<'a> = (&'a str, Body, Vec<[f32; 2]>, Vec<f32>);
fn expected(id: &str) -> crate::Result<Expected<'_>> {
    let square = vec![[-1.25, -1.25], [1.25, -1.25], [1.25, 1.25], [-1.25, 1.25]];
    let row = vec![[-4.5, 0.0], [-1.5, 0.0], [1.5, 0.0], [4.5, 0.0]];
    Ok(match id {
        "B-100" => (id, Body::Block, square, vec![crate::plate::WAIST_100; 4]),
        "S-100" => (id, Body::Strip, row, vec![crate::plate::WAIST_100; 4]),
        "S-324" => (id, Body::Strip, row, vec![crate::plate::WAIST_324; 4]),
        "W" => (
            id,
            Body::Witness,
            (0..6).map(|i| [i as f32 * 2.5 - 6.25, 0.0]).collect(),
            vec![
                crate::plate::WAIST_324,
                crate::plate::WAIST_324,
                crate::plate::WAIST_324,
                crate::plate::WAIST_100,
                crate::plate::WAIST_100,
                crate::plate::WAIST_100,
            ],
        ),
        "K-15" => (id, Body::K15, vec![], vec![]),
        "K-60" => (id, Body::K60, vec![], vec![]),
        _ => return Err(format!("unknown calibration id {id}").into()),
    })
}
