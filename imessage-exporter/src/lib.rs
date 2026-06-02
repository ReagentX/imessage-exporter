#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

//! In addition to powering the `imessage-exporter` binary, this crate exposes
//! its application runtime as a library so other front-ends (for example, a
//! GUI) can reuse the exact same data pipeline, options, and exporters without
//! reimplementing any logic.

pub mod app;
pub mod exporters;

pub use app::{options::Options, runtime::Config};
pub use exporters::{html::HTML, txt::TXT};
