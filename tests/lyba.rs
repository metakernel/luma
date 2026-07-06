#![cfg(feature = "lyba")]

#[path = "conformance/lyba/mod.rs"]
mod lyba_conformance;

use lyma::{LymaValue, lyba};
use lyma_syntax::LymaNull;

#[test]
fn lyba_facade_decodes_checked_in_level1_fixture() {
    let values = lyba::try_from_lyba_value_image(lyba_conformance::checked_in_level1_fixture())
        .expect("checked-in fixture should decode through the facade");

    assert_eq!(values[0], LymaValue::Null(LymaNull));
    assert_eq!(values[5], LymaValue::String(String::from("hi")));
}

#[test]
fn lyba_fixture_catalog_covers_levels_0_through_5() {
    let mut positives = [0_usize; 6];
    let mut negatives = [0_usize; 6];

    for (relative, polarity) in lyba_conformance::LEVEL_MANIFESTS {
        let text = lyba_conformance::manifest_text(relative);
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
fn lyba_conformance_levels_0_through_5_cover_positive_and_negative_cases() {
    let level0 = lyba_conformance::level0_positive_bytes();
    lyba_conformance::assert_level0_positive(&level0);
    assert_eq!(
        lyba_conformance::level0_negative_error().code().as_str(),
        "LB0007"
    );

    let level1 = lyba::try_from_lyba_value_image(lyba_conformance::checked_in_level1_fixture())
        .expect("level1 checked-in fixture should decode");
    assert_eq!(level1.len(), 6);
    let level1_negative = lyba::try_from_lyba_value_image(include_bytes!(
        "fixtures/lyba/level1/invalid-varints.lyba"
    ))
    .expect_err("invalid varint fixture should fail");
    assert_eq!(level1_negative.code().as_str(), "LB0012");

    let level2 = lyba_conformance::level2_positive_bytes();
    lyba_conformance::assert_level2_positive(&level2);
    assert_eq!(
        lyba_conformance::level2_negative_error().code().as_str(),
        "LB0019"
    );

    let level3 = lyba_conformance::level3_positive_bytes();
    lyba_conformance::assert_level3_positive(&level3);
    assert_eq!(
        lyba_conformance::level3_negative_error().code().as_str(),
        "LB0019"
    );

    let level4 = lyba_conformance::level4_positive_bytes();
    lyba_conformance::assert_level4_positive(&level4);
    assert_eq!(
        lyba_conformance::level4_negative_error().code().as_str(),
        "LB0014"
    );

    let level5 = lyba_conformance::level5_positive_bytes();
    lyba_conformance::assert_level5_positive(&level5);
    assert_eq!(
        lyba_conformance::level5_negative_error().code().as_str(),
        "LB0019"
    );
}

#[test]
fn lyba_canonical_verification_accepts_strict_bytes_and_rejects_noncanonical_bytes() {
    lyma::lyba::verify::Verifier::new()
        .verify_canonical(&lyba_conformance::level0_positive_bytes())
        .expect("strict canonical bytes should verify");
    assert_eq!(
        lyba_conformance::canonical_negative_error()
            .code()
            .as_str(),
        "LB0017"
    );
}

#[test]
fn lyba_malformed_security_cases_report_expected_codes() {
    let codes = [
        lyba_conformance::level0_negative_error()
            .code()
            .as_str()
            .to_owned(),
        lyba::try_from_lyba_value_image(include_bytes!(
            "fixtures/lyba/level1/invalid-refs.lyba"
        ))
        .expect_err("invalid refs fixture should fail")
        .code()
        .as_str()
        .to_owned(),
        lyba_conformance::level4_negative_error()
            .code()
            .as_str()
            .to_owned(),
        lyba_conformance::level5_negative_error()
            .code()
            .as_str()
            .to_owned(),
    ];

    assert_eq!(codes, ["LB0007", "LB0014", "LB0014", "LB0019"]);
}

#[test]
fn lyba_round_trip_invariants_hold_for_checked_in_and_generated_positive_cases() {
    let checked_in =
        lyba::try_from_lyba_value_image(lyba_conformance::checked_in_level1_fixture())
            .expect("checked-in level1 fixture should decode");
    let reencoded = lyba::try_to_lyba_value_image(&checked_in)
        .expect("checked-in level1 fixture should reencode canonically");
    assert_eq!(
        reencoded.as_slice(),
        lyba_conformance::checked_in_level1_fixture()
    );

    let level0 = lyba_conformance::level0_positive_bytes();
    let level0_round_trip = lyma::lyba::Writer::new(
        lyma::lyba::WriteOptions::new()
            .with_mode(lyma::lyba::WriterMode::Canonical(
                lyma::lyba::CanonicalMode::Strict,
            ))
            .with_header_crc_mode(lyma::lyba::container::HeaderCrcMode::Disabled),
    )
    .write(
        &lyma::lyba::Reader::new(lyma::lyba::ReadOptions::new())
            .read(&level0)
            .expect("level0 bytes should decode"),
    )
    .expect("level0 bytes should reencode");
    assert_eq!(level0_round_trip, level0);

    let level2 = lyba_conformance::level2_positive_bytes();
    let level2_round_trip = lyma::lyba::Writer::new(
        lyma::lyba::WriteOptions::new()
            .with_mode(lyma::lyba::WriterMode::Canonical(
                lyma::lyba::CanonicalMode::Strict,
            ))
            .with_header_crc_mode(lyma::lyba::container::HeaderCrcMode::Disabled),
    )
    .write(
        &lyma::lyba::Reader::new(lyma::lyba::ReadOptions::new())
            .read(&level2)
            .expect("level2 bytes should decode"),
    )
    .expect("level2 bytes should reencode");
    assert_eq!(level2_round_trip, level2);

    let level3 = lyba_conformance::level3_positive_bytes();
    let level3_round_trip = lyma::lyba::Writer::new(lyma::lyba::WriteOptions::new())
        .write(
            &lyma::lyba::Reader::new(lyma::lyba::ReadOptions::new())
                .read(&level3)
                .expect("level3 bytes should decode"),
        )
        .expect("level3 bytes should reencode");
    lyba_conformance::assert_level3_positive(&level3_round_trip);

    let level4 = lyba_conformance::level4_positive_bytes();
    let level4_round_trip = lyma::lyba::Writer::new(lyma::lyba::WriteOptions::new())
        .write(
            &lyma::lyba::Reader::new(
                lyma::lyba::ReadOptions::new().with_limits(lyma::lyba::Limits::trusted()),
            )
            .read(&level4)
            .expect("level4 bytes should decode"),
        )
        .expect("level4 bytes should reencode");
    lyba_conformance::assert_level4_positive(&level4_round_trip);

    let level5 = lyba_conformance::level5_positive_bytes();
    let level5_round_trip = lyma::lyba::Writer::new(lyma::lyba::WriteOptions::new())
        .write(
            &lyma::lyba::Reader::new(
                lyma::lyba::ReadOptions::new().with_limits(lyma::lyba::Limits::trusted()),
            )
            .read(&level5)
            .expect("level5 bytes should decode"),
        )
        .expect("level5 bytes should reencode");
    lyba_conformance::assert_level5_positive(&level5_round_trip);
}

#[test]
fn lyba_cli_fixture_verification_accepts_canonical_editor_cache_and_fixture_outputs() {
    lyba_conformance::run_cli_fixture_flow();
}
