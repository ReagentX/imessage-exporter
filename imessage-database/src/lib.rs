#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

pub mod error;
pub mod message_types;
pub mod tables;
#[cfg(test)]
mod test_support;
pub mod util;
