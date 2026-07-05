#![cfg(feature = "lumba")]

use luma::{LumaNull, LumaValue, lumba};

#[test]
fn root_facade_round_trips_portable_values_through_lumba() {
    let values = vec![
        LumaValue::Null(LumaNull),
        LumaValue::Boolean(true),
        LumaValue::String(String::from("portable")),
    ];

    let encoded = lumba::try_to_lumba_value_image(&values)
        .expect("portable root-facade values should encode");
    let decoded = lumba::try_from_lumba_value_image(&encoded)
        .expect("encoded root-facade values should decode");

    assert_eq!(decoded, values);
}

#[test]
fn root_facade_decodes_checked_in_fixture() {
    let values = lumba::try_from_lumba_value_image(include_bytes!(
        "fixtures/lumba/level1/minimal-values.lumba"
    ))
    .expect("checked-in fixture should decode through the root facade");

    assert_eq!(values[0], LumaValue::Null(LumaNull));
    assert_eq!(values[5], LumaValue::String(String::from("hi")));
}
