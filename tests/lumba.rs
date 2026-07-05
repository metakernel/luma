#![cfg(feature = "lumba")]

#[path = "conformance/lumba/mod.rs"]
mod lumba_conformance;

use luma::{LumaValue, lumba};
use luma_syntax::LumaNull;

#[test]
fn lumba_facade_decodes_checked_in_level1_fixture() {
    let values = lumba::try_from_lumba_value_image(lumba_conformance::checked_in_level1_fixture())
        .expect("checked-in fixture should decode through the facade");

    assert_eq!(values[0], LumaValue::Null(LumaNull));
    assert_eq!(values[5], LumaValue::String(String::from("hi")));
}

#[test]
fn lumba_fixture_catalog_covers_levels_0_through_5() {
    let mut positives = [0_usize; 6];
    let mut negatives = [0_usize; 6];

    for (relative, polarity) in lumba_conformance::LEVEL_MANIFESTS {
        let text = lumba_conformance::manifest_text(relative);
        let level = relative
            .strip_prefix("level")
            .and_then(|rest| rest.chars().next())
            .and_then(|ch| ch.to_digit(10))
            .expect("manifest path should encode level") as usize;
        assert!(
            text.contains("\"name\""),
            "fixture {relative} should declare a name"
        );
        match polarity {
            "positive" => positives[level] += 1,
            "negative" => negatives[level] += 1,
            other => panic!("unexpected polarity {other}"),
        }
    }

    for level in 0..=5 {
        assert!(
            positives[level] >= 1,
            "level {level} missing positive fixture"
        );
        assert!(
            negatives[level] >= 1,
            "level {level} missing negative fixture"
        );
    }
}

#[test]
fn lumba_conformance_levels_0_through_5_cover_positive_and_negative_cases() {
    let level0 = lumba_conformance::level0_positive_bytes();
    lumba_conformance::assert_level0_positive(&level0);
    assert_eq!(
        lumba_conformance::level0_negative_error().code().as_str(),
        "LB0007"
    );

    let level1 = lumba::try_from_lumba_value_image(lumba_conformance::checked_in_level1_fixture())
        .expect("level1 checked-in fixture should decode");
    assert_eq!(level1.len(), 6);
    let level1_negative = lumba::try_from_lumba_value_image(include_bytes!(
        "fixtures/lumba/level1/invalid-varints.lumba"
    ))
    .expect_err("invalid varint fixture should fail");
    assert_eq!(level1_negative.code().as_str(), "LB0012");

    let level2 = lumba_conformance::level2_positive_bytes();
    lumba_conformance::assert_level2_positive(&level2);
    assert_eq!(
        lumba_conformance::level2_negative_error().code().as_str(),
        "LB0019"
    );

    let level3 = lumba_conformance::level3_positive_bytes();
    lumba_conformance::assert_level3_positive(&level3);
    assert_eq!(
        lumba_conformance::level3_negative_error().code().as_str(),
        "LB0019"
    );

    let level4 = lumba_conformance::level4_positive_bytes();
    lumba_conformance::assert_level4_positive(&level4);
    assert_eq!(
        lumba_conformance::level4_negative_error().code().as_str(),
        "LB0014"
    );

    let level5 = lumba_conformance::level5_positive_bytes();
    lumba_conformance::assert_level5_positive(&level5);
    assert_eq!(
        lumba_conformance::level5_negative_error().code().as_str(),
        "LB0019"
    );
}

#[test]
fn lumba_canonical_verification_accepts_strict_bytes_and_rejects_noncanonical_bytes() {
    luma::lumba::verify::Verifier::new()
        .verify_canonical(&lumba_conformance::level0_positive_bytes())
        .expect("strict canonical bytes should verify");
    assert_eq!(
        lumba_conformance::canonical_negative_error()
            .code()
            .as_str(),
        "LB0017"
    );
}

#[test]
fn lumba_malformed_security_cases_report_expected_codes() {
    let codes = [
        lumba_conformance::level0_negative_error()
            .code()
            .as_str()
            .to_owned(),
        lumba::try_from_lumba_value_image(include_bytes!(
            "fixtures/lumba/level1/invalid-refs.lumba"
        ))
        .expect_err("invalid refs fixture should fail")
        .code()
        .as_str()
        .to_owned(),
        lumba_conformance::level4_negative_error()
            .code()
            .as_str()
            .to_owned(),
        lumba_conformance::level5_negative_error()
            .code()
            .as_str()
            .to_owned(),
    ];

    assert_eq!(codes, ["LB0007", "LB0014", "LB0014", "LB0019"]);
}

#[test]
fn lumba_round_trip_invariants_hold_for_checked_in_and_generated_positive_cases() {
    let checked_in =
        lumba::try_from_lumba_value_image(lumba_conformance::checked_in_level1_fixture())
            .expect("checked-in level1 fixture should decode");
    let reencoded = lumba::try_to_lumba_value_image(&checked_in)
        .expect("checked-in level1 fixture should reencode canonically");
    assert_eq!(
        reencoded.as_slice(),
        lumba_conformance::checked_in_level1_fixture()
    );

    let level0 = lumba_conformance::level0_positive_bytes();
    let level0_round_trip = luma::lumba::Writer::new(
        luma::lumba::WriteOptions::new()
            .with_mode(luma::lumba::WriterMode::Canonical(
                luma::lumba::CanonicalMode::Strict,
            ))
            .with_header_crc_mode(luma::lumba::container::HeaderCrcMode::Disabled),
    )
    .write(
        &luma::lumba::Reader::new(luma::lumba::ReadOptions::new())
            .read(&level0)
            .expect("level0 bytes should decode"),
    )
    .expect("level0 bytes should reencode");
    assert_eq!(level0_round_trip, level0);

    let level2 = lumba_conformance::level2_positive_bytes();
    let level2_round_trip = luma::lumba::Writer::new(
        luma::lumba::WriteOptions::new()
            .with_mode(luma::lumba::WriterMode::Canonical(
                luma::lumba::CanonicalMode::Strict,
            ))
            .with_header_crc_mode(luma::lumba::container::HeaderCrcMode::Disabled),
    )
    .write(
        &luma::lumba::Reader::new(luma::lumba::ReadOptions::new())
            .read(&level2)
            .expect("level2 bytes should decode"),
    )
    .expect("level2 bytes should reencode");
    assert_eq!(level2_round_trip, level2);

    let level3 = lumba_conformance::level3_positive_bytes();
    let level3_round_trip = luma::lumba::Writer::new(luma::lumba::WriteOptions::new())
        .write(
            &luma::lumba::Reader::new(luma::lumba::ReadOptions::new())
                .read(&level3)
                .expect("level3 bytes should decode"),
        )
        .expect("level3 bytes should reencode");
    lumba_conformance::assert_level3_positive(&level3_round_trip);

    let level4 = lumba_conformance::level4_positive_bytes();
    let level4_round_trip = luma::lumba::Writer::new(luma::lumba::WriteOptions::new())
        .write(
            &luma::lumba::Reader::new(
                luma::lumba::ReadOptions::new().with_limits(luma::lumba::Limits::trusted()),
            )
            .read(&level4)
            .expect("level4 bytes should decode"),
        )
        .expect("level4 bytes should reencode");
    lumba_conformance::assert_level4_positive(&level4_round_trip);

    let level5 = lumba_conformance::level5_positive_bytes();
    let level5_round_trip = luma::lumba::Writer::new(luma::lumba::WriteOptions::new())
        .write(
            &luma::lumba::Reader::new(
                luma::lumba::ReadOptions::new().with_limits(luma::lumba::Limits::trusted()),
            )
            .read(&level5)
            .expect("level5 bytes should decode"),
        )
        .expect("level5 bytes should reencode");
    lumba_conformance::assert_level5_positive(&level5_round_trip);
}

#[test]
fn lumba_cli_fixture_verification_accepts_canonical_editor_cache_and_fixture_outputs() {
    lumba_conformance::run_cli_fixture_flow();
}
