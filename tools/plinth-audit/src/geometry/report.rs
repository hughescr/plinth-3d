//! Drive the four reports over a file and render them: a human summary, a JSON
//! document and the one-line-per-file baseline table.

use super::contacts::{self, ContactReport};
use super::heads::{self, HeadRow};
use super::intrusion::{self, IntrusionReport, COUNT_ABOVE_MM};
use super::topology::{self, TopologyReport, PROUD_LEVELS_MM, RING_MISMATCH_MM};
use super::{median, GeoCtx, Params};
use crate::json::Json;
use crate::reader::ChituboxFile;
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

/// How many of the deepest intrusions the summary lists.
pub const WORST_LISTED: usize = 20;
/// Heads whose cone-start ring is closer than this to the model are counted
/// as flush (the figure the v19 diagnosis reported).
pub const FLUSH_MM: f64 = 0.01;

pub struct ModelGeometry {
    pub name: String,
    pub records: usize,
    pub heads: Vec<HeadRow>,
    pub intrusion: IntrusionReport,
    pub topology: TopologyReport,
    pub contacts: ContactReport,
    pub seconds: f64,
}

pub struct FileGeometry {
    pub path: String,
    pub sha256: String,
    pub models: Vec<ModelGeometry>,
    pub seconds: f64,
}

/// Open a file and run all four reports on every model.
pub fn analyse_file(path: &Path, params: Params) -> Result<FileGeometry, String> {
    let file = ChituboxFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(analyse(&file, params))
}

#[must_use]
pub fn analyse(file: &ChituboxFile, params: Params) -> FileGeometry {
    let t_file = Instant::now();
    let mut models = Vec::new();
    for m in &file.models {
        let t0 = Instant::now();
        let ctx = GeoCtx::build(file, m, params);
        let walks = heads::walk_heads(&ctx);
        let head_rows = heads::measure_heads(&ctx, &walks);
        let tips: Vec<usize> = walks.iter().map(|w| w.tip).collect();
        models.push(ModelGeometry {
            name: m.name.clone(),
            records: m.records.len(),
            heads: head_rows,
            intrusion: intrusion::measure(&ctx),
            topology: topology::measure(&ctx),
            contacts: contacts::measure(&ctx, &tips),
            seconds: t0.elapsed().as_secs_f64(),
        });
    }
    FileGeometry {
        path: file.path.display().to_string(),
        sha256: file.sha256.clone(),
        models,
        seconds: t_file.elapsed().as_secs_f64(),
    }
}

fn jn(v: f64) -> Json {
    Json::num_or_null(Some(v))
}

impl ModelGeometry {
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("name", Json::str(self.name.clone()));
        o.set("records", Json::usize(self.records));
        o.set("seconds", jn(self.seconds));

        let mut heads = Json::Arr(Vec::new());
        for h in &self.heads {
            let mut r = Json::obj();
            r.set("tip", Json::usize(h.tip));
            r.set("ball_r_mm", jn(h.ball_r_mm));
            r.set("ball_height_mm", Json::num_or_null(h.ball_height_mm));
            r.set("link_mm", jn(h.link_mm));
            r.set("ring_clearance_mm", jn(h.ring_clearance_mm));
            r.set("ring_points_inside", Json::usize(h.ring_points_inside));
            r.set("joint", Json::usize(h.joint));
            r.set("joint_r_mm", jn(h.joint_r_mm));
            r.set("post", h.post.map_or(Json::Null, Json::usize));
            r.set("post_lean_deg", Json::num_or_null(h.post_lean_deg));
            r.set("phi_deg", Json::num_or_null(h.phi_deg));
            r.set("frusta", Json::Arr(h.frusta.iter().map(|&i| Json::usize(i)).collect()));
            heads.push(r);
        }
        o.set("heads", heads);

        let mut intr = Json::obj();
        intr.set("records_checked", Json::usize(self.intrusion.records_checked));
        intr.set("samples", Json::usize(self.intrusion.samples));
        intr.set("indeterminate_samples", Json::usize(self.intrusion.indeterminate));
        intr.set("count_above_threshold", Json::usize(self.intrusion.count_above));
        intr.set("threshold_mm", jn(COUNT_ABOVE_MM));
        let mut ranked = Json::Arr(Vec::new());
        for r in &self.intrusion.ranked {
            let mut e = Json::obj();
            e.set("record", Json::usize(r.record));
            e.set("kind", Json::str(r.kind.label()));
            e.set("depth_mm", jn(r.depth_mm));
            e.set("depth_with_ball_mm", jn(r.depth_with_ball_mm));
            e.set("worst_point", Json::v3(r.worst_point));
            ranked.push(e);
        }
        intr.set("ranked", ranked);
        o.set("intrusion", intr);

        let t = &self.topology;
        let mut top = Json::obj();
        top.set("records_with_mesh", Json::usize(t.mesh.records));
        top.set("open_edges", Json::usize(t.mesh.open_edges));
        top.set("open_edge_records", Json::usize(t.mesh.open_records));
        top.set("nonmanifold_edges", Json::usize(t.mesh.nonmanifold_edges));
        top.set("nonmanifold_edge_records", Json::usize(t.mesh.nonmanifold_records));
        top.set("inconsistent_edges", Json::usize(t.mesh.inconsistent_edges));
        top.set("degenerate_triangles", Json::usize(t.mesh.degenerate_triangles));
        top.set("degenerate_triangle_records", Json::usize(t.mesh.degenerate_records));
        top.set(
            "nonpositive_volume_records",
            Json::Arr(t.mesh.nonpositive_volume.iter().map(|&i| Json::usize(i)).collect()),
        );
        top.set("ring_pairs_checked", Json::usize(t.ring_pairs_checked));
        let mut rm = Json::Arr(Vec::new());
        for m in &t.ring_mismatches {
            let mut e = Json::obj();
            e.set("a", Json::usize(m.a));
            e.set("b", Json::usize(m.b));
            e.set("mismatch_mm", jn(m.mismatch_mm));
            rm.push(e);
        }
        top.set("ring_mismatches", rm);
        let sl = |v: &[(usize, f64)]| {
            Json::Arr(
                v.iter()
                    .map(|&(i, l)| {
                        let mut e = Json::obj();
                        e.set("record", Json::usize(i));
                        e.set("length_mm", jn(l));
                        e
                    })
                    .collect(),
            )
        };
        top.set("slivers_under_one_pixel", sl(&t.slivers_pixel));
        top.set("slivers_under_0p01", sl(&t.slivers_tiny));
        let shoulder_json = |list: Vec<&topology::Shoulder>| {
            let mut sh = Json::Arr(Vec::new());
            for s in list {
                let mut e = Json::obj();
                e.set("a", Json::usize(s.a));
                e.set("b", Json::usize(s.b));
                e.set("step_mm", jn(s.step_mm));
                e.set("bend_deg", jn(s.bend_deg));
                sh.push(e);
            }
            sh
        };
        top.set("flat_shoulders", shoulder_json(t.flat_shoulders()));
        top.set("radius_steps", shoulder_json(t.shoulders.iter().collect()));
        let mut ld = Json::Arr(Vec::new());
        for l in &t.landed {
            let mut e = Json::obj();
            e.set("record", Json::usize(l.record));
            e.set("slot", Json::usize(l.slot));
            e.set("ring_vertices", Json::usize(l.ring_vertices));
            e.set("outside_vertices", Json::usize(l.outside_vertices));
            e.set("proud_mm", jn(l.proud_mm));
            e.set("host", l.host.map_or(Json::Null, Json::usize));
            e.set("proud_host_mm", jn(l.proud_host_mm));
            ld.push(e);
        }
        top.set("landed_ends", ld);
        let mut hl = Json::Arr(Vec::new());
        for h in &t.hulls {
            let mut e = Json::obj();
            e.set("record", Json::usize(h.record));
            e.set("facets", Json::usize(h.facets));
            e.set("min_lateral_nz", Json::num_or_null(h.min_lateral_nz));
            hl.push(e);
        }
        top.set("non_analytic_members", hl);
        o.set("topology", top);

        let mut con = Json::obj();
        con.set("straight", Json::usize(self.contacts.straight));
        con.set(
            "off_normal_beyond_skim",
            Json::usize(self.contacts.off_normal_beyond_skim),
        );
        let mut rows = Json::Arr(Vec::new());
        for r in &self.contacts.rows {
            let mut e = Json::obj();
            e.set("tip", Json::usize(r.tip));
            e.set("ball_r_mm", jn(r.ball_r_mm));
            e.set("tip_r_mm", jn(r.tip_r_mm));
            e.set("ball_height_mm", Json::num_or_null(r.ball_height_mm));
            e.set("off_normal_deg", Json::num_or_null(r.off_normal_deg));
            e.set("skims", Json::Bool(r.skims));
            e.set("shaft_clearance_mm", Json::num_or_null(r.shaft_clearance_mm));
            rows.push(e);
        }
        con.set("rows", rows);
        o.set("contacts", con);
        o
    }

    /// The headline numbers, for the baseline table and the tests.
    #[must_use]
    pub fn headline(&self) -> Headline {
        let rings: Vec<f64> = self.heads.iter().map(|h| h.ring_clearance_mm).collect();
        let t = &self.topology;
        Headline {
            heads: self.heads.len(),
            link_zero: self.heads.iter().filter(|h| h.link_mm <= 0.0).count(),
            ring_min: rings.iter().copied().fold(f64::INFINITY, f64::min),
            ring_median: median(&rings),
            ring_max: rings.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            ring_flush: rings.iter().filter(|&&r| r < FLUSH_MM).count(),
            intrusion_count: self.intrusion.count_above,
            intrusion_max: self.intrusion.ranked.first().map_or(0.0, |r| r.depth_mm),
            open_edges: t.mesh.open_edges,
            nonmanifold_edges: t.mesh.nonmanifold_edges,
            degenerate_triangles: t.mesh.degenerate_triangles,
            nonpositive_volume: t.mesh.nonpositive_volume.len(),
            ring_mismatches: t.ring_mismatches.len(),
            slivers_pixel: t.slivers_pixel.len(),
            slivers_tiny: t.slivers_tiny.len(),
            shoulders: t.flat_shoulders().len(),
            landed: t.landed.len(),
            landed_host_03: t.landed_host_proud_at_least(PROUD_LEVELS_MM[1]),
            landed_host_02: t.landed_host_proud_at_least(PROUD_LEVELS_MM[0]),
            landed_proud_03: t.landed_proud_at_least(PROUD_LEVELS_MM[1]),
            landed_proud_02: t.landed_proud_at_least(PROUD_LEVELS_MM[0]),
            straight: self.contacts.straight,
            off_normal_skim: self.contacts.off_normal_beyond_skim,
            hulls: t.hulls.len(),
            seconds: self.seconds,
        }
    }

    #[must_use]
    pub fn summary(&self) -> String {
        let h = self.headline();
        let mut s = String::new();
        let _ = writeln!(s, "model {:?}: {} records, {:.1} s", self.name, self.records, self.seconds);

        let _ = writeln!(s, "\n[1] heads: {} (link 0 mm on {})", h.heads, h.link_zero);
        if !self.heads.is_empty() {
            let _ = writeln!(
                s,
                "    cone-start ring to model: min {:.3} / median {:.3} / max {:.3} mm; {} under {:.2} mm",
                h.ring_min,
                h.ring_median.unwrap_or(f64::NAN),
                h.ring_max,
                h.ring_flush,
                FLUSH_MM
            );
            let links: Vec<f64> = self.heads.iter().map(|x| x.link_mm).collect();
            let _ = writeln!(
                s,
                "    link: max {:.3} mm, median {:.3} mm",
                links.iter().copied().fold(0.0, f64::max),
                median(&links).unwrap_or(f64::NAN)
            );
            let _ = writeln!(
                s,
                "    tip  ball_r  ball_h  link  ring_min  joint  joint_r  post  lean   phi"
            );
            for r in &self.heads {
                let _ = writeln!(
                    s,
                    "    {:>4} {:>6.3} {:>7} {:>5.3} {:>8.3} {:>6} {:>7.4} {:>5} {:>6} {:>6}",
                    r.tip,
                    r.ball_r_mm,
                    r.ball_height_mm.map_or("n/a".to_string(), |x| format!("{x:.3}")),
                    r.link_mm,
                    r.ring_clearance_mm,
                    r.joint,
                    r.joint_r_mm,
                    r.post.map_or("-".to_string(), |p| p.to_string()),
                    r.post_lean_deg.map_or("n/a".to_string(), |x| format!("{x:.1}")),
                    r.phi_deg.map_or("n/a".to_string(), |x| format!("{x:.1}")),
                );
            }
        }

        let _ = writeln!(
            s,
            "\n[2] intrusion beyond the contact ball (record headers, pre-trim): {} of {} records deeper than {} mm; {} samples, {} indeterminate",
            self.intrusion.count_above,
            self.intrusion.records_checked,
            COUNT_ABOVE_MM,
            self.intrusion.samples,
            self.intrusion.indeterminate
        );
        for r in self.intrusion.ranked.iter().take(WORST_LISTED) {
            let _ = writeln!(
                s,
                "    record {:>5} {:<6} {:.3} mm beyond the ball ({:.3} mm counting the ball sphere)",
                r.record,
                r.kind.label(),
                r.depth_mm,
                r.depth_with_ball_mm
            );
        }

        let t = &self.topology;
        let _ = writeln!(s, "\n[3] junction topology ({} record meshes)", t.mesh.records);
        let _ = writeln!(
            s,
            "    open edges {} ({} records); non-manifold edges {} ({} records); inconsistent {}; degenerate triangles {} ({} records); volume <= 0: {}",
            t.mesh.open_edges,
            t.mesh.open_records,
            t.mesh.nonmanifold_edges,
            t.mesh.nonmanifold_records,
            t.mesh.inconsistent_edges,
            t.mesh.degenerate_triangles,
            t.mesh.degenerate_records,
            t.mesh.nonpositive_volume.len()
        );
        let _ = writeln!(
            s,
            "    end rings that should coincide: {} pairs checked, {} off by more than {} mm",
            t.ring_pairs_checked,
            t.ring_mismatches.len(),
            RING_MISMATCH_MM
        );
        let _ = writeln!(
            s,
            "    frusta shorter than one pixel: {} (under 0.01 mm: {})",
            t.slivers_pixel.len(),
            t.slivers_tiny.len()
        );
        let _ = writeln!(
            s,
            "    flat shoulders (radius step over one pixel on a straight chain, no cone): {}; radius steps of any size or bend: {}",
            t.flat_shoulders().len(),
            t.shoulders.len()
        );
        let _ = writeln!(
            s,
            "    landed branch ends: {}; rim proud of the host >= {} mm: {}, >= {} mm: {}; proud of every other solid >= {} mm: {}, >= {} mm: {}",
            t.landed.len(),
            PROUD_LEVELS_MM[1],
            h.landed_host_03,
            PROUD_LEVELS_MM[0],
            h.landed_host_02,
            PROUD_LEVELS_MM[1],
            h.landed_proud_03,
            PROUD_LEVELS_MM[0],
            h.landed_proud_02
        );
        let mut worst: Vec<_> = t.landed.iter().filter(|l| l.proud_host_mm > 0.0).collect();
        worst.sort_by(|a, b| {
            b.proud_host_mm
                .partial_cmp(&a.proud_host_mm)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for l in worst.iter().take(10) {
            let _ = writeln!(
                s,
                "      record {:>5} end {} host {} proud of host {:.3} mm, of every other solid {:.3} mm ({} of {} ring vertices outside all)",
                l.record,
                l.slot,
                l.host.map_or("-".to_string(), |h| h.to_string()),
                l.proud_host_mm,
                l.proud_mm,
                l.outside_vertices,
                l.ring_vertices
            );
        }
        if t.hulls.is_empty() {
            let _ = writeln!(s, "    non-analytic (hull) members: none");
        } else {
            let nz = t
                .hulls
                .iter()
                .filter_map(|x| x.min_lateral_nz)
                .fold(f64::INFINITY, f64::min);
            let _ = writeln!(
                s,
                "    non-analytic (hull) members: {}; smallest lateral facet n_z {:.3}",
                t.hulls.len(),
                nz
            );
        }

        let c = &self.contacts;
        let _ = writeln!(
            s,
            "\n[4] straight contacts: {}; off the normal beyond the skim law: {}",
            c.straight, c.off_normal_beyond_skim
        );
        let mut rows: Vec<_> = c.rows.iter().filter(|r| r.skims).collect();
        rows.sort_by(|a, b| {
            b.off_normal_deg
                .partial_cmp(&a.off_normal_deg)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for r in rows.iter().take(10) {
            let _ = writeln!(
                s,
                "      tip {:>5} off normal {:>5.1} deg, ball height {}, shaft clearance {}",
                r.tip,
                r.off_normal_deg.unwrap_or(f64::NAN),
                r.ball_height_mm.map_or("n/a".to_string(), |x| format!("{x:.3}")),
                r.shaft_clearance_mm.map_or("n/a".to_string(), |x| format!("{x:.3}")),
            );
        }
        s
    }
}

/// The numbers of one model that the baseline table and the tests read.
#[derive(Clone, Copy, Debug)]
pub struct Headline {
    pub heads: usize,
    pub link_zero: usize,
    pub ring_min: f64,
    pub ring_median: Option<f64>,
    pub ring_max: f64,
    pub ring_flush: usize,
    pub intrusion_count: usize,
    pub intrusion_max: f64,
    pub open_edges: usize,
    pub nonmanifold_edges: usize,
    pub degenerate_triangles: usize,
    pub nonpositive_volume: usize,
    pub ring_mismatches: usize,
    pub slivers_pixel: usize,
    pub slivers_tiny: usize,
    pub shoulders: usize,
    pub landed: usize,
    /// Landed ends whose rim stands proud of the HOST by at least 0.3 / 0.2 mm.
    pub landed_host_03: usize,
    pub landed_host_02: usize,
    /// The same against the union of every other solid.
    pub landed_proud_03: usize,
    pub landed_proud_02: usize,
    pub straight: usize,
    pub off_normal_skim: usize,
    pub hulls: usize,
    pub seconds: f64,
}

impl FileGeometry {
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("file", Json::str(self.path.clone()));
        o.set("sha256", Json::str(self.sha256.clone()));
        o.set("seconds", jn(self.seconds));
        o.set(
            "models",
            Json::Arr(self.models.iter().map(ModelGeometry::to_json).collect()),
        );
        o
    }

    #[must_use]
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{}\nsha256 {}\n{:.1} s\n",
            self.path, self.sha256, self.seconds
        );
        for m in &self.models {
            s.push_str(&m.summary());
        }
        s
    }
}

/// One row per file (first model), the v20 baseline table.
#[must_use]
pub fn table(files: &[FileGeometry]) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{:<22} {:>5} {:>5} {:>6} {:>6} {:>6} {:>4} {:>11} {:>4} {:>4} {:>4} {:>3} {:>4} {:>6} {:>5} {:>6} {:>7} {:>7} {:>4} {:>5}",
        "file", "heads", "link0", "rMin", "rMed", "rMax", "flsh", "intr/max", "open", "nman", "degn", "v<0",
        "ringX", "sliv", "shldr", "landed", "pu.3/.2", "ph.3/.2", "str", "skim"
    );
    for f in files {
        for m in &f.models {
            let h = m.headline();
            let stem = Path::new(&f.path)
                .file_stem()
                .map_or_else(String::new, |x| x.to_string_lossy().into_owned());
            let f3 = |x: f64| if x.is_finite() { format!("{x:.3}") } else { "-".to_string() };
            let _ = writeln!(
                s,
                "{:<22} {:>5} {:>5} {:>6} {:>6} {:>6} {:>4} {:>11} {:>4} {:>4} {:>4} {:>3} {:>4} {:>6} {:>5} {:>6} {:>7} {:>7} {:>4} {:>5}",
                stem,
                h.heads,
                h.link_zero,
                f3(h.ring_min),
                h.ring_median.map_or("-".to_string(), f3),
                f3(h.ring_max),
                h.ring_flush,
                format!("{}/{:.3}", h.intrusion_count, h.intrusion_max),
                h.open_edges,
                h.nonmanifold_edges,
                h.degenerate_triangles,
                h.nonpositive_volume,
                h.ring_mismatches,
                format!("{}/{}", h.slivers_pixel, h.slivers_tiny),
                h.shoulders,
                h.landed,
                format!("{}/{}", h.landed_proud_03, h.landed_proud_02),
                format!("{}/{}", h.landed_host_03, h.landed_host_02),
                h.straight,
                h.off_normal_skim,
            );
        }
    }
    let _ = writeln!(
        s,
        "columns: heads; link0 = heads with a 0 mm link; rMin/rMed/rMax = cone-start ring to model (mm); flsh = heads with ring under {FLUSH_MM} mm; intr = records deeper than {COUNT_ABOVE_MM} mm / deepest (mm); open = open edges; nman = non-manifold edges; degn = degenerate triangles; v<0 = records with volume <= 0; ringX = collinear equal-radius end rings off by > {RING_MISMATCH_MM} mm; sliv = frusta under one pixel / under 0.01 mm; shldr = flat shoulders (radius step over one pixel on a straight chain); landed = landed branch ends; pu = of those, rim proud of every other solid by at least 0.3 / 0.2 mm; ph = the same against the landing host alone; str = straight contacts; skim = of those, off normal beyond the skim law"
    );
    s
}
