use std::sync::{Arc, Weak};

use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::metrics::data::ResourceMetrics;
use opentelemetry_sdk::metrics::reader::MetricReader;
use opentelemetry_sdk::metrics::{ManualReader, ManualReaderBuilder, Pipeline};

use crate::serialize::PrometheusSerializer;

/// Configuration for the Prometheus exporter
#[derive(Debug, Clone, Copy, Default)]
pub struct ExporterConfig {
    pub disable_target_info: bool,
    pub without_units: bool,
    pub without_counter_suffixes: bool,
    pub disable_scope_info: bool,
}

#[derive(Clone, Debug)]
pub struct PrometheusExporter {
    inner: Arc<ManualReader>,
    serializer: PrometheusSerializer,
}

impl MetricReader for PrometheusExporter {
    fn register_pipeline(&self, pipeline: Weak<Pipeline>) {
        self.inner.register_pipeline(pipeline);
    }

    fn collect(&self, rm: &mut ResourceMetrics) -> OTelSdkResult {
        self.inner.collect(rm)
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: std::time::Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn temporality(
        &self,
        kind: opentelemetry_sdk::metrics::InstrumentKind,
    ) -> opentelemetry_sdk::metrics::Temporality {
        self.inner.temporality(kind)
    }
}

impl PrometheusExporter {
    /// Create a new exporter with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Create a new exporter builder
    #[must_use]
    pub fn builder() -> ExporterBuilder {
        ExporterBuilder::default()
    }

    /// Export the collected metrics to the given writer.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer fails to write the metrics.
    pub fn export<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let mut rm = ResourceMetrics::default();
        self.inner.collect(&mut rm).map_err(std::io::Error::other)?;
        self.serializer.serialize(&rm, writer)?;
        Ok(())
    }
}

impl Default for PrometheusExporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for configuring [`PrometheusExporter`] with various options.
///
/// This builder implements the same configuration API as the
/// `opentelemetry-prometheus` crate to provide a compatible replacement. The
/// following configuration options are supported:
///
/// # Configuration Options
///
/// ## Unit Handling
/// - [`without_units()`]: Disables automatic unit suffix addition to metric
///   names
///   - Example: `request.duration` with unit `ms` becomes `request_duration`
///     instead of `request_duration_milliseconds`
///
/// ## Counter Suffixes
/// - [`without_counter_suffixes()`]: Disables automatic `_total` suffix for
///   monotonic counters
///   - Example: `http.requests` becomes `http_requests` instead of
///     `http_requests_total`
///
/// ## Resource Information
/// - [`without_target_info()`]: Disables the `target_info` metric that contains
///   resource attributes
///   - When disabled, resource attributes are not exported as a separate metric
///
/// ## Scope Information
/// - [`without_scope_info()`]: Disables OpenTelemetry scope labels and metrics
///   - When disabled, `otel_scope_name`, `otel_scope_version`, etc. labels are
///     not added
///   - Also disables the `otel_scope_info` metric
///
/// # Example Usage
///
/// ```rust
/// use opentelemetry_prometheus_text_exporter::PrometheusExporter;
///
/// # fn main() {
/// // Create exporter with default configuration (all features enabled)
/// let exporter = PrometheusExporter::builder().build();
///
/// // Create exporter with selective features disabled
/// let exporter = PrometheusExporter::builder()
///     .without_units()
///     .without_counter_suffixes()
///     .build();
///
/// // Create exporter with all optional features disabled
/// let exporter = PrometheusExporter::builder()
///     .without_units()
///     .without_counter_suffixes()
///     .without_target_info()
///     .without_scope_info()
///     .build();
/// # }
/// ```
///
/// [`without_units()`]: ExporterBuilder::without_units
/// [`without_counter_suffixes()`]: ExporterBuilder::without_counter_suffixes
/// [`without_target_info()`]: ExporterBuilder::without_target_info
/// [`without_scope_info()`]: ExporterBuilder::without_scope_info
#[derive(Default)]
pub struct ExporterBuilder {
    disable_target_info: bool,
    without_units: bool,
    without_counter_suffixes: bool,
    disable_scope_info: bool,
    reader: ManualReaderBuilder,
}

impl std::fmt::Debug for ExporterBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExporterBuilder")
            .field("disable_target_info", &self.disable_target_info)
            .field("without_units", &self.without_units)
            .field("without_counter_suffixes", &self.without_counter_suffixes)
            .field("disable_scope_info", &self.disable_scope_info)
            .finish_non_exhaustive()
    }
}

impl ExporterBuilder {
    /// Disables exporter's addition of unit suffixes to metric names.
    ///
    /// By default, metric names include a unit suffix to follow Prometheus
    /// naming conventions. For example, the counter metric
    /// `request.duration`, with unit `ms` would become
    /// `request_duration_milliseconds_total`.
    ///
    /// With this option set, the name would instead be
    /// `request_duration_total`.
    #[must_use]
    pub fn without_units(mut self) -> Self {
        self.without_units = true;
        self
    }

    /// Disables exporter's addition `_total` suffixes on counters.
    ///
    /// By default, metric names include a `_total` suffix to follow Prometheus
    /// naming conventions. For example, the counter metric `happy.people` would
    /// become `happy_people_total`. With this option set, the name would
    /// instead be `happy_people`.
    #[must_use]
    pub fn without_counter_suffixes(mut self) -> Self {
        self.without_counter_suffixes = true;
        self
    }

    /// Configures the exporter to not export the resource `target_info` metric.
    ///
    /// If not specified, the exporter will create a `target_info` metric
    /// containing the metrics' [Resource] attributes.
    ///
    /// [Resource]: opentelemetry_sdk::Resource
    #[must_use]
    pub fn without_target_info(mut self) -> Self {
        self.disable_target_info = true;
        self
    }

    /// Configures the exporter to not export the `otel_scope_info` metric.
    ///
    /// If not specified, the exporter will create a `otel_scope_info` metric
    /// containing the metrics' Instrumentation Scope, and also add labels about
    /// Instrumentation Scope to all metric points.
    #[must_use]
    pub fn without_scope_info(mut self) -> Self {
        self.disable_scope_info = true;
        self
    }

    /// Creates a new [`PrometheusExporter`] from this configuration.
    #[must_use]
    pub fn build(self) -> PrometheusExporter {
        let inner = Arc::new(self.reader.build());

        let config = ExporterConfig {
            disable_target_info: self.disable_target_info,
            without_units: self.without_units,
            without_counter_suffixes: self.without_counter_suffixes,
            disable_scope_info: self.disable_scope_info,
        };

        let serializer = PrometheusSerializer::with_config(config);

        PrometheusExporter { inner, serializer }
    }
}
