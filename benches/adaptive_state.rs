//! Fixed adaptive panel over the original state harness.
#[allow(dead_code)]
#[path = "state.rs"]
mod harness;
fn main() {
    harness::run_adaptive();
}
