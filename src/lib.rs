#![doc = include_str!("../README.md")]

#[deny(
    clippy::all,
    clippy::pedantic,
    rustdoc::broken_intra_doc_links,
    missing_docs
)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "The configuration struct has many boolean fields, this is intentional"
)]
pub(crate) mod exporter;
pub(crate) mod resource_selector;
pub(crate) mod serialize;

pub use self::exporter::{ExporterBuilder, PrometheusExporter};
pub use self::resource_selector::ResourceSelector;
