//! Small synthetic shapes for unit tests (design section 6, T0).

pub(crate) type Soup = (Vec<f64>, Vec<f64>, Vec<f64>);

/// A unit cube `[0,1]^3` as a 36-corner soup (12 triangles), outward winding.
pub(crate) fn cube_soup() -> Soup {
    let c: [[f64; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    let quads = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [1, 2, 6, 5],
        [3, 0, 4, 7],
    ];
    soup_from_triangles(
        &quads
            .iter()
            .flat_map(|q| [[c[q[0]], c[q[1]], c[q[2]]], [c[q[0]], c[q[2]], c[q[3]]]])
            .collect::<Vec<_>>(),
    )
}

/// A triangle list as a soup.
pub(crate) fn soup_from_triangles(tris: &[[[f64; 3]; 3]]) -> Soup {
    let mut s = (Vec::new(), Vec::new(), Vec::new());
    for p in tris.iter().flatten() {
        s.0.push(p[0]);
        s.1.push(p[1]);
        s.2.push(p[2]);
    }
    s
}

/// A flat square plate `[0,4]^2` at z = 0 made of two triangles.
pub(crate) fn plate_soup() -> Soup {
    soup_from_triangles(&[
        [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 4.0, 0.0]],
        [[0.0, 0.0, 0.0], [4.0, 4.0, 0.0], [0.0, 4.0, 0.0]],
    ])
}

/// Assert two points are equal to within 1e-12 per coordinate (clippy denies `==` on
/// floats; these known-answer values are exact up to rounding).
#[track_caller]
pub(crate) fn assert_pt(got: [f64; 3], want: [f64; 3]) {
    let ok = got.iter().zip(&want).all(|(g, w)| (g - w).abs() <= 1e-12);
    assert!(ok, "got {got:?}, want {want:?}");
}
