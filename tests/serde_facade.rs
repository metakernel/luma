#![cfg(feature = "serde")]

use luma::LumaValue;

#[test]
fn serde_facade_exposes_to_value_without_runtime_or_eval() {
    assert!(cfg!(feature = "syntax"));
    assert!(!cfg!(feature = "runtime"));
    assert!(!cfg!(feature = "eval"));

    let value = luma::serde::to_value("example").expect("serde facade should serialize");

    assert_eq!(value, LumaValue::String("example".to_owned()));
}
