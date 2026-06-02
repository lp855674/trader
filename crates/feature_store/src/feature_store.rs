#![forbid(unsafe_code)]

pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}
