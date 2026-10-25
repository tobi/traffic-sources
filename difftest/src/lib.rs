//! Compiles the crate's internal regex engine and URL parser on their own so the tests in
//! `tests/` can compare them with the `regex` and `url` crates.
#![allow(dead_code)]

#[path = "../../src/pattern.rs"]
pub mod pattern;
#[path = "../../src/url.rs"]
pub mod url;

#[cfg(test)]
mod reference;
