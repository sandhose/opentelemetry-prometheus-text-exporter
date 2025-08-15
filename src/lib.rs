//! OpenTelemetry Prometheus Exporter
//!
//! This crate provides a Prometheus exporter for OpenTelemetry metrics that follows
//! the [OpenTelemetry specification for Prometheus compatibility].
//!
//! [OpenTelemetry specification for Prometheus compatibility]: https://opentelemetry.io/docs/specs/otel/compatibility/prometheus_and_openmetrics/
//!
//! # Features
//!
//! - **Memory optimized**: Uses `Cow<str>` to avoid unnecessary string allocations
//! - **Specification compliant**: Properly transforms metric names, units, and labels
//! - **Type mapping**: Correctly maps OTLP metric types to Prometheus types
//! - **Scope support**: Includes instrumentation scope information as labels
//! - **Resource attributes**: Converts resource to `target_info` metric
//!
//! # Example
//!
//! ```rust
//! use opentelemetry::{KeyValue, metrics::MeterProvider};
//! use opentelemetry_sdk::metrics::SdkMeterProvider;
//! use opentelemetry_prometheus_exporter::PrometheusExporter;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let exporter = PrometheusExporter::new();
//! let provider = SdkMeterProvider::builder()
//!     .with_reader(exporter.clone())
//!     .build();
//!
//! let meter = provider.meter("example");
//!
//! // Create metrics following OpenTelemetry semantic conventions
//! let counter = meter
//!     .u64_counter("http.server.requests")
//!     .with_description("Number of HTTP server requests")
//!     .with_unit("{request}")
//!     .build();
//!
//! let histogram = meter
//!     .f64_histogram("http.server.duration")
//!     .with_description("Duration of HTTP server requests")
//!     .with_unit("s")
//!     .build();
//!
//! // Record some data
//! counter.add(1, &[KeyValue::new("method", "GET"), KeyValue::new("status", "200")]);
//! histogram.record(0.1, &[KeyValue::new("method", "GET")]);
//!
//! // Export to Prometheus format
//! let mut buffer = Vec::new();
//! exporter.export(&mut buffer)?;
//!
//! let prometheus_output = String::from_utf8(buffer)?;
//! println!("{}", prometheus_output);
//!
//! // Output includes:
//! // - Sanitized metric names: http.server.requests -> http_server_requests_total
//! // - Unit conversion and suffixes: s -> seconds -> http_server_duration_seconds
//! // - Proper Prometheus types: counter -> counter, histogram -> histogram
//! // - Scope labels: otel_scope_name="example"
//! // - Resource as target_info metric
//! # Ok(())
//! # }
//! ```
//!
//! # Memory Optimizations
//!
//! The implementation uses `Cow<str>` extensively to avoid unnecessary allocations:
//!
//! ```rust
//! # use std::borrow::Cow;
//! # use opentelemetry_prometheus_exporter::PrometheusExporter;
//! // Valid metric names are not modified (no allocation)
//! // sanitize_name("valid_metric_name") -> Cow::Borrowed("valid_metric_name")
//!
//! // Invalid names are sanitized (allocation only when needed)
//! // sanitize_name("invalid.metric.name") -> Cow::Owned("invalid_metric_name")
//!
//! // Units are converted only when necessary
//! // convert_unit("s") -> Cow::Borrowed("seconds")
//! // convert_unit("custom_unit") -> Cow::Borrowed("custom_unit")
//! ```

#[deny(clippy::all, clippy::pedantic)]
pub(crate) mod exporter;
pub(crate) mod serialize;

pub use self::exporter::PrometheusExporter;
