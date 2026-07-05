#[path = "harness/mod.rs"]
mod harness;

#[test]
fn conformance_level0() {
    harness::run_level("level0");
}

#[test]
fn conformance_level1() {
    harness::run_level("level1");
}

#[cfg(feature = "eval")]
#[test]
fn conformance_level2() {
    harness::run_level("level2");
}

#[cfg(feature = "eval")]
#[test]
fn conformance_level3() {
    harness::run_level("level3");
}

#[test]
fn conformance_level4() {
    harness::run_level("level4");
}
