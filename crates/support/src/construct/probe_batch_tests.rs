use super::*;
use crate::construct::tests::{builder, candidate, plate};

#[test]
fn early_private_rejections_preserve_public_admission_and_patch_indices() {
    let plan = builder(&plate());
    let mut bad_geometry = candidate(0, -10.);
    bad_geometry.standoff = 0.01;
    let mut bad_fields = candidate(1, -5.);
    bad_fields.ball_r = f32::NAN;
    let inputs = [
        bad_geometry,
        bad_fields,
        candidate(2, 0.),
        candidate(3, 10.),
    ];
    let mut public = Vec::new();
    let mut private = Vec::new();
    plan.probe(&inputs, &mut public);
    plan.probe_alternatives(&inputs, &mut private);
    assert!(matches!(private[0], Err(Reject::HeadTouchesModel)));
    assert!(matches!(private[1], Err(Reject::NonFinite)));
    let mut compared = 0;
    for (public, private) in public.iter().zip(&private) {
        assert_eq!(public.is_ok(), private.is_ok());
        if let (Ok(a), Ok(b)) = (public, private) {
            assert_eq!(
                a.contact().patch().lower().to_bits(),
                b.contact().patch().lower().to_bits()
            );
            assert_eq!(
                a.contact().patch().upper().to_bits(),
                b.contact().patch().upper().to_bits()
            );
            assert_eq!(
                a.contact().centre().map(f32::to_bits),
                b.contact().centre().map(f32::to_bits)
            );
            compared += 1;
        }
    }
    assert_eq!(
        compared, 2,
        "two surviving heads exercise the compact patch batch"
    );
}
