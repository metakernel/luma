#![cfg(feature = "serde")]

use lyma::LymaValue;

#[test]
fn serde_facade_exposes_to_value_without_runtime_or_eval() {
    assert!(cfg!(feature = "syntax"));

    let value = lyma::serde::to_value("example").expect("serde facade should serialize");

    assert_eq!(value, LymaValue::String("example".to_owned()));
}
