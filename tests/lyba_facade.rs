#![cfg(feature = "lyba")]

use lyma::{LymaNull, LymaValue, lyba};

#[test]
fn root_facade_round_trips_portable_values_through_lyba() {
    let values = vec![
        LymaValue::Null(LymaNull),
        LymaValue::Boolean(true),
        LymaValue::String(String::from("portable")),
    ];

    let encoded = lyba::try_to_lyba_value_image(&values)
        .expect("portable root-facade values should encode");
    let decoded = lyba::try_from_lyba_value_image(&encoded)
        .expect("encoded root-facade values should decode");

    assert_eq!(decoded, values);
}

#[test]
fn root_facade_decodes_checked_in_fixture() {
    let values = lyba::try_from_lyba_value_image(include_bytes!(
        "fixtures/lyba/level1/minimal-values.lyba"
    ))
    .expect("checked-in fixture should decode through the root facade");

    assert_eq!(values[0], LymaValue::Null(LymaNull));
    assert_eq!(values[5], LymaValue::String(String::from("hi")));
}
