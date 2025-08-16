#![doc = include_str!("../README.md")]

#[deny(clippy::all, clippy::pedantic)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "The configuration struct has many boolean fields, this is intentional"
)]
pub(crate) mod exporter;
pub(crate) mod serialize;

pub use self::exporter::{ExporterBuilder, ExporterConfig, PrometheusExporter};
pub use self::serialize::PrometheusSerializer;
