use plinth_core::Finite;
use plinth_support::SphereGeometry;

fn main() {
    let center = [Finite::new(0.0).unwrap(); 3];
    let axis = [Finite::new(0.0).unwrap(); 3];
    let _ = SphereGeometry::Tip {
        center_mm: center,
        outward_axis_unit: axis,
    };
}
