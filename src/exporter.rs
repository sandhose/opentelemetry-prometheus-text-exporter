use std::sync::{Arc, Weak};

use opentelemetry_sdk::{
    error::OTelSdkResult,
    metrics::{ManualReader, Pipeline, data::ResourceMetrics, reader::MetricReader},
};

#[derive(Clone, Default, Debug)]
pub struct PrometheusExporter {
    inner: Arc<ManualReader>,
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Export the collected metrics to the given writer.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer fails to write the metrics.
    pub fn export<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let mut rm = ResourceMetrics::default();
        self.inner.collect(&mut rm).map_err(std::io::Error::other)?;
        crate::serialize::serialize(&rm, writer)?;
        Ok(())
    }
}
