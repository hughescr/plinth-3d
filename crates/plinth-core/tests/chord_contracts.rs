use plinth_core::{
    canon_chord_micron, chord_parity_report, chords_match_micron, ChordError, ChordParityError,
    ChordSource, MicronGranularity, MicronGranularityError,
};
use std::collections::BTreeMap;

fn micron(value: u32) -> MicronGranularity {
    MicronGranularity::new(value).unwrap()
}

fn map(entries: &[(u32, &[f64])]) -> BTreeMap<u32, Vec<f64>> {
    entries
        .iter()
        .map(|&(member_idx, chords)| (member_idx, chords.to_vec()))
        .collect()
}

#[test]
fn current_oracle_vectors_and_pairwise_matching() {
    let one = MicronGranularity::ONE_MICRON;
    for (chord, expected) in [
        (0.080_000_0, 80),
        (0.080_000_4, 80),
        (0.0806, 81),
        (0.0794, 79),
    ] {
        assert_eq!(canon_chord_micron(chord, one), Ok(expected));
    }
    for (chord, expected) in [(0.0800, 80), (0.0813, 80), (0.0824, 80), (0.0826, 85)] {
        assert_eq!(canon_chord_micron(chord, micron(5)), Ok(expected));
    }
    assert_eq!(chords_match_micron(0.25310, 0.25314, one), Ok(true));
    assert_eq!(chords_match_micron(0.25310, 0.25360, one), Ok(false));
}

#[test]
fn source_order_ties_are_even_and_bit_neighbors_pin_the_boundary() {
    let one = MicronGranularity::ONE_MICRON;
    for (chord, expected) in [
        (0.0005, 0),
        (0.0015, 2),
        (0.0025, 2),
        (0.0035, 4),
        (0.0805, 80),
    ] {
        assert_eq!(canon_chord_micron(chord, one), Ok(expected));
    }
    for (chord, expected) in [(0.003, 4), (0.005, 4), (0.007, 8)] {
        assert_eq!(canon_chord_micron(chord, micron(2)), Ok(expected));
    }
    for (chord, expected) in [(0.0825, 80), (0.0875, 90), (0.2725, 270)] {
        assert_eq!(canon_chord_micron(chord, micron(5)), Ok(expected));
    }
    for (chord, expected) in [(0.0045, 6), (0.0075, 6)] {
        assert_eq!(canon_chord_micron(chord, micron(3)), Ok(expected));
    }

    assert_eq!(canon_chord_micron(0.0805_f64.next_down(), one), Ok(80));
    assert_eq!(canon_chord_micron(0.0805, one), Ok(80));
    assert_eq!(canon_chord_micron(0.0805_f64.next_up(), one), Ok(81));
}

#[test]
fn negative_chords_and_signed_zero_follow_the_executable_oracle() {
    let one = MicronGranularity::ONE_MICRON;
    for (chord, expected) in [
        (-0.0005, 0),
        (-0.0015, -2),
        (-0.0025, -2),
        (-0.0805, -80),
        (-0.0, 0),
    ] {
        assert_eq!(canon_chord_micron(chord, one), Ok(expected));
    }
}

#[test]
fn canonicalization_is_comparison_only() {
    let one = MicronGranularity::ONE_MICRON;
    assert_eq!(canon_chord_micron(0.0796, one), Ok(80));
    assert_eq!(canon_chord_micron(0.0800, one), Ok(80));
}

#[test]
fn granularity_is_positive_integral_and_strict_at_the_wire_boundary() {
    assert_eq!(MicronGranularity::default(), MicronGranularity::ONE_MICRON);
    assert_eq!(MicronGranularity::new(0), Err(MicronGranularityError::Zero));
    assert_eq!(
        MicronGranularity::try_from(-1_i64),
        Err(MicronGranularityError::Negative { value: -1 })
    );
    assert!(serde_json::from_str::<MicronGranularity>("0")
        .unwrap_err()
        .to_string()
        .contains("must be greater than zero"));
    assert!(serde_json::from_str::<MicronGranularity>("-1")
        .unwrap_err()
        .to_string()
        .contains("must be positive"));
    for wire in ["true", "\"5\"", "1.0"] {
        assert!(
            serde_json::from_str::<MicronGranularity>(wire).is_err(),
            "accepted {wire}"
        );
    }
}

#[test]
fn canonicalization_errors_are_concrete() {
    let one = MicronGranularity::ONE_MICRON;
    for nonfinite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            canon_chord_micron(nonfinite, one),
            Err(ChordError::NonFiniteChord)
        );
    }
    assert_eq!(
        canon_chord_micron(f64::MAX, one),
        Err(ChordError::ScaledNonFinite)
    );
    let out_of_range = (-(i64::MIN as f64)) / 1000.0;
    assert_eq!(
        canon_chord_micron(out_of_range, one),
        Err(ChordError::RoundedBucketOutOfRange)
    );
    let overflow = (i64::MAX as f64) / 1000.0;
    assert!(matches!(
        canon_chord_micron(overflow, micron(2)),
        Err(ChordError::CanonicalMicronOverflow { .. })
    ));
}

#[test]
fn parity_gathers_every_member_retains_duplicates_and_is_order_independent() {
    let engine = map(&[(8, &[0.2501, 0.06, 0.2500]), (2, &[0.1204])]);
    let census = map(&[(2, &[0.12]), (8, &[0.0599, 0.2500, 0.2501])]);
    let report = chord_parity_report(&engine, &census, MicronGranularity::ONE_MICRON).unwrap();
    assert!(report.matches());
    assert!(report.only_engine().is_empty());
    assert!(report.only_census().is_empty());
    assert!(report.mismatches().is_empty());
    assert_eq!(
        (report.n_engine(), report.n_census(), report.n_shared()),
        (2, 2, 2)
    );

    let engine_reordered = map(&[(2, &[0.1204]), (8, &[0.2500, 0.2501, 0.06])]);
    let census_reordered = map(&[(8, &[0.2501, 0.2500, 0.0599]), (2, &[0.12])]);
    let reordered = chord_parity_report(
        &engine_reordered,
        &census_reordered,
        MicronGranularity::ONE_MICRON,
    )
    .unwrap();
    assert_eq!(report, reordered);
    assert_eq!(
        serde_json::to_vec(&report).unwrap(),
        serde_json::to_vec(&reordered).unwrap()
    );
    let wire = serde_json::to_value(report).unwrap();
    assert_eq!(wire["matches"], true);
    assert!(wire.get("ok").is_none());
}

#[test]
fn parity_reports_population_and_chord_mismatches_without_claiming_acceptance() {
    let population = chord_parity_report(
        &map(&[(100, &[0.25]), (445, &[0.2531])]),
        &map(&[(100, &[0.25]), (474, &[0.30])]),
        MicronGranularity::ONE_MICRON,
    )
    .unwrap();
    assert!(!population.matches());
    assert_eq!(population.only_engine(), [445]);
    assert_eq!(population.only_census(), [474]);
    assert_eq!(
        (
            population.n_engine(),
            population.n_census(),
            population.n_shared()
        ),
        (2, 2, 1)
    );

    let chords = chord_parity_report(
        &map(&[(100, &[0.25, 0.25])]),
        &map(&[(100, &[0.40])]),
        MicronGranularity::ONE_MICRON,
    )
    .unwrap();
    assert!(!chords.matches());
    assert!(chords.only_engine().is_empty());
    assert!(chords.only_census().is_empty());
    assert_eq!(chords.mismatches().len(), 1);
    let mismatch = &chords.mismatches()[0];
    assert_eq!(mismatch.member_idx(), 100);
    assert_eq!(mismatch.engine_micron(), [250, 250]);
    assert_eq!(mismatch.census_micron(), [400]);
    let wire = serde_json::to_value(chords).unwrap();
    assert_eq!(wire["matches"], false);
    assert!(wire.get("ok").is_none());
}

#[test]
fn parity_validates_unshared_members_empty_lists_and_disjoint_input() {
    let invalid_unshared = chord_parity_report(
        &map(&[(1, &[0.25]), (9, &[f64::NAN])]),
        &map(&[(1, &[0.25])]),
        MicronGranularity::ONE_MICRON,
    );
    assert!(matches!(
        invalid_unshared,
        Err(ChordParityError::InvalidChord {
            population: ChordSource::Engine,
            member_idx: 9,
            chord_index: 0,
            error: ChordError::NonFiniteChord
        })
    ));
    let empty = chord_parity_report(
        &map(&[(1, &[])]),
        &BTreeMap::new(),
        MicronGranularity::ONE_MICRON,
    );
    assert_eq!(
        empty,
        Err(ChordParityError::EmptyChordList {
            population: ChordSource::Engine,
            member_idx: 1
        })
    );
    let disjoint = chord_parity_report(
        &map(&[(1, &[0.25])]),
        &map(&[(2, &[0.25])]),
        MicronGranularity::ONE_MICRON,
    )
    .unwrap();
    assert!(!disjoint.matches());
    assert_eq!(disjoint.only_engine(), [1]);
    assert_eq!(disjoint.only_census(), [2]);
}
