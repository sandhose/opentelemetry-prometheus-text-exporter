//! OpenTelemetry to Prometheus format serialization.
//!
//! This module implements the conversion from OpenTelemetry metrics to the Prometheus
//! text exposition format, following the OpenTelemetry specification for Prometheus
//! compatibility: https://opentelemetry.io/docs/specs/otel/compatibility/prometheus_and_openmetrics/
//!
//! # Transformations Applied
//!
//! ## Metric Names
//! - Dots (`.`) are converted to underscores (`_`) for Prometheus compatibility
//! - Invalid characters are sanitized to follow Prometheus naming conventions
//! - Monotonic sums get `_total` suffix if not already present
//! - Units are converted and added as suffixes when appropriate
//!
//! ## Units
//! - OTLP units are converted to Prometheus conventions (e.g., `s` → `seconds`)
//! - Content within brackets is removed (e.g., `{packet}` → empty)
//! - Special conversions: `1` → `ratio`, `foo/bar` → `foo_per_bar`
//!
//! ## Labels
//! - Attribute names are sanitized to follow Prometheus label naming rules
//! - Instrumentation scope information is added as `otel_scope_*` labels

use opentelemetry::KeyValue;
use opentelemetry_sdk::{
    Resource,
    metrics::{
        Temporality,
        data::{AggregatedMetrics, Gauge, Histogram, Metric, MetricData, ResourceMetrics, Sum},
    },
};
use std::borrow::Cow;

use std::io::Write;

/// Prometheus format serializer with configurable options
#[derive(Debug, Clone)]
pub struct PrometheusSerializer {
    /// Whether to include OpenTelemetry scope labels (otel_scope_name, etc.)
    pub include_scope_labels: bool,
}

impl PrometheusSerializer {
    /// Create a new serializer with default configuration
    pub fn new() -> Self {
        Self {
            include_scope_labels: true,
        }
    }

    /// Create a new serializer with scope labels disabled
    pub fn without_scope_labels() -> Self {
        Self {
            include_scope_labels: false,
        }
    }

    /// Serialize ResourceMetrics to Prometheus format
    pub fn serialize<W: Write>(&self, rm: &ResourceMetrics, writer: &mut W) -> std::io::Result<()> {
        self.serialize_resource_metrics(rm, writer)
    }

    fn serialize_resource_metrics<W: Write>(
        &self,
        rm: &ResourceMetrics,
        writer: &mut W,
    ) -> std::io::Result<()> {
        // Serialize all scope metrics first
        for sm in rm.scope_metrics() {
            self.serialize_scope_metrics(sm, writer)?;
        }

        // Serialize resource as target_info
        self.serialize_resource(rm.resource(), writer)?;

        Ok(())
    }

    fn serialize_resource<W: Write>(
        &self,
        resource: &Resource,
        writer: &mut W,
    ) -> std::io::Result<()> {
        // Don't serialize empty resources
        if resource.is_empty() {
            return Ok(());
        }

        write_type_comment(writer, "target_info", "gauge")?;
        write_help_comment(writer, "target_info", "Target metadata")?;

        write!(writer, "target_info")?;

        let mut label_writer = LabelWriter::new(writer);
        for (key, value) in resource.iter() {
            let sanitized_key = sanitize_name(key.as_str());
            let value_str = format!("{value}");
            label_writer.emit(&sanitized_key, &value_str)?;
        }
        label_writer.finish()?;

        writeln!(writer, " 1")?;

        Ok(())
    }

    fn serialize_scope_metrics<W: Write>(
        &self,
        scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
        writer: &mut W,
    ) -> std::io::Result<()> {
        for metric in scope_metrics.metrics() {
            self.serialize_metric(metric, scope_metrics, writer)?;
        }
        Ok(())
    }

    fn serialize_metric<W: Write>(
        &self,
        metric: &Metric,
        scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
        writer: &mut W,
    ) -> std::io::Result<()> {
        let data = metric.data();

        let Some((prometheus_type, is_monotonic)) = get_prometheus_type_and_is_monotonic(data)
        else {
            return Ok(()); // Skip unsupported metrics
        };

        // Apply name transformations
        let sanitized_name = sanitize_name(metric.name());
        let converted_unit = convert_unit(metric.unit());

        // Add unit suffix if needed and not already present
        let final_name = if converted_unit.is_empty() {
            sanitized_name
        } else {
            add_unit_suffix(sanitized_name.as_ref(), converted_unit.as_ref())
        };

        // Add _total suffix for monotonic sums if needed
        let final_name = if is_monotonic && !final_name.ends_with("_total") {
            Cow::Owned(format!("{final_name}_total"))
        } else {
            final_name
        };

        // Write metadata
        write_type_comment(writer, final_name.as_ref(), prometheus_type)?;
        write_help_comment(writer, final_name.as_ref(), metric.description())?;
        write_unit_comment(writer, final_name.as_ref(), converted_unit.as_ref())?;

        match data {
            AggregatedMetrics::F64(MetricData::Gauge(gauge)) => {
                self.serialize_gauge(final_name.as_ref(), gauge, scope_metrics, writer)?;
            }
            AggregatedMetrics::U64(MetricData::Gauge(gauge)) => {
                self.serialize_gauge(final_name.as_ref(), gauge, scope_metrics, writer)?;
            }
            AggregatedMetrics::I64(MetricData::Gauge(gauge)) => {
                self.serialize_gauge(final_name.as_ref(), gauge, scope_metrics, writer)?;
            }

            AggregatedMetrics::F64(MetricData::Sum(sum)) => {
                self.serialize_sum(final_name.as_ref(), sum, scope_metrics, writer)?;
            }
            AggregatedMetrics::U64(MetricData::Sum(sum)) => {
                self.serialize_sum(final_name.as_ref(), sum, scope_metrics, writer)?;
            }
            AggregatedMetrics::I64(MetricData::Sum(sum)) => {
                self.serialize_sum(final_name.as_ref(), sum, scope_metrics, writer)?;
            }

            AggregatedMetrics::F64(MetricData::Histogram(histogram)) => {
                self.serialize_histogram(final_name.as_ref(), histogram, scope_metrics, writer)?;
            }
            AggregatedMetrics::U64(MetricData::Histogram(histogram)) => {
                self.serialize_histogram(final_name.as_ref(), histogram, scope_metrics, writer)?;
            }
            AggregatedMetrics::I64(MetricData::Histogram(histogram)) => {
                self.serialize_histogram(final_name.as_ref(), histogram, scope_metrics, writer)?;
            }

            // Skip exponential histograms
            AggregatedMetrics::F64(MetricData::ExponentialHistogram(_))
            | AggregatedMetrics::U64(MetricData::ExponentialHistogram(_))
            | AggregatedMetrics::I64(MetricData::ExponentialHistogram(_)) => {}
        }

        writeln!(writer)?;

        Ok(())
    }

    fn write_scope_labels<W: Write>(
        &self,
        scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
        label_writer: &mut LabelWriter<W>,
    ) -> std::io::Result<()> {
        if !self.include_scope_labels {
            return Ok(());
        }
        let scope = scope_metrics.scope();

        // Add scope name
        if !scope.name().is_empty() {
            label_writer.emit("otel_scope_name", scope.name())?;
        }

        // Add scope version
        if let Some(version) = scope.version() {
            if !version.is_empty() {
                label_writer.emit("otel_scope_version", version)?;
            }
        }

        // Add scope schema URL
        if let Some(schema_url) = scope.schema_url() {
            if !schema_url.is_empty() {
                label_writer.emit("otel_scope_schema_url", schema_url)?;
            }
        }

        // Add scope attributes (excluding name, version, schema_url to avoid conflicts)
        for attr in scope.attributes() {
            let key = attr.key.as_str();
            if key != "name" && key != "version" && key != "schema_url" {
                let sanitized_key = sanitize_name(key);
                let value = format!("{}", attr.value);
                let prefixed_key = format!("otel_scope_{}", sanitized_key.as_ref());
                label_writer.emit(&prefixed_key, &value)?;
            }
        }

        Ok(())
    }

    fn write_metric_labels<W: Write>(
        &self,
        attributes: impl Iterator<Item = KeyValue>,
        scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
        writer: &mut W,
    ) -> std::io::Result<()> {
        let mut label_writer = LabelWriter::new(writer);

        write_attributes_as_labels(attributes, &mut label_writer)?;
        self.write_scope_labels(scope_metrics, &mut label_writer)?;

        label_writer.finish()
    }

    fn write_bucket_labels<W: Write>(
        &self,
        attributes: impl Iterator<Item = KeyValue>,
        scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
        le_value: &str,
        writer: &mut W,
    ) -> std::io::Result<()> {
        let mut label_writer = LabelWriter::new(writer);

        write_attributes_as_labels(attributes, &mut label_writer)?;
        label_writer.emit("le", le_value)?;
        self.write_scope_labels(scope_metrics, &mut label_writer)?;

        label_writer.finish()
    }

    fn serialize_gauge<T: Numeric, W: Write>(
        &self,
        name: &str,
        gauge: &Gauge<T>,
        scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
        writer: &mut W,
    ) -> std::io::Result<()> {
        for data_point in gauge.data_points() {
            write!(writer, "{name}")?;
            self.write_metric_labels(data_point.attributes().cloned(), scope_metrics, writer)?;
            write!(writer, " ")?;
            data_point.value().serialize(writer)?;
            writeln!(writer)?;
        }

        Ok(())
    }

    fn serialize_sum<T: Numeric, W: Write>(
        &self,
        name: &str,
        sum: &Sum<T>,
        scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
        writer: &mut W,
    ) -> std::io::Result<()> {
        for data_point in sum.data_points() {
            write!(writer, "{name}")?;
            self.write_metric_labels(data_point.attributes().cloned(), scope_metrics, writer)?;
            write!(writer, " ")?;
            data_point.value().serialize(writer)?;
            writeln!(writer)?;
        }

        Ok(())
    }

    fn serialize_histogram<T: Numeric, W: Write>(
        &self,
        name: &str,
        histogram: &Histogram<T>,
        scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
        writer: &mut W,
    ) -> std::io::Result<()> {
        for data_point in histogram.data_points() {
            // _count metric
            write!(writer, "{name}_count")?;
            self.write_metric_labels(data_point.attributes().cloned(), scope_metrics, writer)?;
            write!(writer, " ")?;
            data_point.count().serialize(writer)?;
            writeln!(writer)?;

            // _sum metric
            write!(writer, "{name}_sum")?;
            self.write_metric_labels(data_point.attributes().cloned(), scope_metrics, writer)?;
            write!(writer, " ")?;
            data_point.sum().serialize(writer)?;
            writeln!(writer)?;

            // _bucket metrics
            let mut cumulative_count = 0u64;
            for (bound, count) in data_point.bounds().zip(data_point.bucket_counts()) {
                cumulative_count += count;

                write!(writer, "{name}_bucket")?;
                self.write_bucket_labels(
                    data_point.attributes().cloned(),
                    scope_metrics,
                    &bound.to_string(),
                    writer,
                )?;
                write!(writer, " ")?;
                cumulative_count.serialize(writer)?;
                writeln!(writer)?;
            }

            // +Inf bucket
            write!(writer, "{name}_bucket")?;
            self.write_bucket_labels(
                data_point.attributes().cloned(),
                scope_metrics,
                "+Inf",
                writer,
            )?;
            write!(writer, " ")?;
            data_point.count().serialize(writer)?;
            writeln!(writer)?;
        }

        Ok(())
    }
}

impl Default for PrometheusSerializer {
    fn default() -> Self {
        Self::new()
    }
}

trait Numeric: Copy {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()>;
}

impl Numeric for f64 {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        if self.is_nan() {
            write!(writer, "NaN")
        } else if self.is_infinite() {
            if self.is_sign_positive() {
                write!(writer, "+Inf")
            } else {
                write!(writer, "-Inf")
            }
        } else {
            write!(writer, "{self}")
        }
    }
}

impl Numeric for u64 {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        write!(writer, "{self}")
    }
}

impl Numeric for i64 {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        write!(writer, "{self}")
    }
}

/// Sanitizes a metric or label name to follow Prometheus naming conventions.
///
/// Prometheus metric and label names must match the regex: `[a-zA-Z_:]([a-zA-Z0-9_:])*`
///
/// # Transformations
///
/// - First character must be `[a-zA-Z_:]`, invalid chars become `_`
/// - Subsequent characters must be `[a-zA-Z0-9_:]`, invalid chars become `_`
/// - Multiple consecutive underscores are collapsed to single `_`
fn sanitize_name(name: &str) -> Cow<'_, str> {
    // Check if name is already valid
    let mut chars = name.chars();
    let needs_sanitization = if let Some(first) = chars.next() {
        // First character must be [a-zA-Z_:]
        if !(first.is_ascii_alphabetic() || first == '_' || first == ':') {
            true
        } else {
            // Check remaining characters and for consecutive underscores
            let mut prev_underscore = false;
            chars.any(|ch| {
                if ch == '_' {
                    if prev_underscore {
                        return true; // Found consecutive underscores
                    }
                    prev_underscore = true;
                    false
                } else {
                    prev_underscore = false;
                    !(ch.is_ascii_alphanumeric() || ch == ':')
                }
            })
        }
    } else {
        false // Empty string is valid
    };

    if !needs_sanitization {
        return Cow::Borrowed(name);
    }

    // Need to sanitize
    let mut result = String::new();
    let mut chars = name.chars();

    // First character must be [a-zA-Z_:]
    if let Some(first) = chars.next() {
        if first.is_ascii_alphabetic() || first == '_' || first == ':' {
            result.push(first);
        } else {
            result.push('_');
        }
    }

    // Subsequent characters must be [a-zA-Z0-9_:]
    for ch in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' {
            result.push(ch);
        } else {
            result.push('_');
        }
    }

    // Replace multiple consecutive underscores with single underscore
    while result.contains("__") {
        result = result.replace("__", "_");
    }

    Cow::Owned(result)
}

/// Converts OTLP unit to Prometheus unit following the OpenTelemetry specification.
///
/// # Transformations
///
/// - Removes content within brackets: `count{packets}` → `count`
/// - Special cases: `1` → `ratio`
/// - Converts slashes: `foo/bar` → `foo_per_bar`
/// - Expands abbreviations: `ms` → `milliseconds`, `s` → `seconds`, etc.
fn convert_unit(unit: &str) -> Cow<'_, str> {
    let trimmed = unit.trim();

    if trimmed.is_empty() {
        return Cow::Borrowed("");
    }

    // Remove content within brackets if present
    let without_brackets = if let Some((before, middle)) = trimmed.split_once('{')
        && let Some((_middle, after)) = middle.split_once('}')
    {
        // We can return a borrowed string if one of the side is empty
        if before.is_empty() {
            Cow::Borrowed(after)
        } else if after.is_empty() {
            Cow::Borrowed(before)
        } else {
            Cow::Owned(format!("{before}{after}"))
        }
    } else {
        Cow::Borrowed(trimmed)
    };

    // Special cases
    if &without_brackets == "1" {
        return Cow::Borrowed("ratio");
    }

    // Convert foo/bar to foo_per_bar
    if without_brackets.contains('/') {
        return Cow::Owned(without_brackets.replace('/', "_per_"));
    }

    // Convert abbreviations to full words
    match &*without_brackets {
        "ms" => Cow::Borrowed("milliseconds"),
        "s" => Cow::Borrowed("seconds"),
        "m" => Cow::Borrowed("meters"),
        "kg" => Cow::Borrowed("kilograms"),
        "g" => Cow::Borrowed("grams"),
        "b" | "bytes" | "By" => Cow::Borrowed("bytes"),
        "%" => Cow::Borrowed("percent"),
        _ => without_brackets,
    }
}

/// Adds unit suffix to metric name if not already present.
fn add_unit_suffix<'a>(name: &'a str, unit: &str) -> Cow<'a, str> {
    if unit.is_empty() || name.ends_with(unit) {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("{name}_{unit}"))
    }
}

/// Writes attributes as Prometheus labels directly to the writer.
///
/// Handles writing the brackets and separating labels with commas.
struct LabelWriter<'a, W: Write> {
    writer: &'a mut W,
    has_written: bool,
}

impl<'a, W: Write> LabelWriter<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            has_written: false,
        }
    }

    fn emit(&mut self, key: &str, value: &str) -> std::io::Result<()> {
        if !self.has_written {
            self.has_written = true;
            write!(self.writer, "{{")?;
        } else {
            write!(self.writer, ",")?;
        }

        write!(self.writer, "{key}={value:?}")?;
        Ok(())
    }

    fn finish(self) -> std::io::Result<()> {
        if self.has_written {
            write!(self.writer, "}}")?;
        }
        Ok(())
    }
}

fn write_attributes_as_labels<W: Write>(
    attributes: impl Iterator<Item = KeyValue>,
    label_writer: &mut LabelWriter<W>,
) -> std::io::Result<()> {
    for attr in attributes {
        let sanitized_key = sanitize_name(attr.key.as_str());
        let value = format!("{}", attr.value);
        label_writer.emit(sanitized_key.as_ref(), &value)?;
    }
    Ok(())
}

/// Writes TYPE comment
fn write_type_comment<W: Write>(
    writer: &mut W,
    name: &str,
    metric_type: &str,
) -> std::io::Result<()> {
    writeln!(writer, "# TYPE {name} {metric_type}")
}

/// Writes HELP comment
fn write_help_comment<W: Write>(
    writer: &mut W,
    name: &str,
    description: &str,
) -> std::io::Result<()> {
    if !description.is_empty() {
        let escaped_description = escape_help_text(description);
        writeln!(writer, "# HELP {name} {escaped_description}")?;
    }
    Ok(())
}

/// Escapes special characters in HELP comment text according to Prometheus format.
fn escape_help_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

/// Writes UNIT comment
fn write_unit_comment<W: Write>(writer: &mut W, name: &str, unit: &str) -> std::io::Result<()> {
    if !unit.is_empty() {
        writeln!(writer, "# UNIT {name} {unit}")?;
    }
    Ok(())
}

fn get_prometheus_type_and_is_monotonic(data: &AggregatedMetrics) -> Option<(&'static str, bool)> {
    match data {
        AggregatedMetrics::F64(MetricData::Gauge(_))
        | AggregatedMetrics::U64(MetricData::Gauge(_))
        | AggregatedMetrics::I64(MetricData::Gauge(_)) => Some(("gauge", false)),

        AggregatedMetrics::F64(MetricData::Sum(sum)) => {
            if sum.temporality() == Temporality::Cumulative && sum.is_monotonic() {
                Some(("counter", true))
            } else {
                Some(("gauge", false))
            }
        }
        AggregatedMetrics::U64(MetricData::Sum(sum)) => {
            if sum.temporality() == Temporality::Cumulative && sum.is_monotonic() {
                Some(("counter", true))
            } else {
                Some(("gauge", false))
            }
        }
        AggregatedMetrics::I64(MetricData::Sum(sum)) => {
            if sum.temporality() == Temporality::Cumulative && sum.is_monotonic() {
                Some(("counter", true))
            } else {
                Some(("gauge", false))
            }
        }

        AggregatedMetrics::F64(MetricData::Histogram(_))
        | AggregatedMetrics::U64(MetricData::Histogram(_))
        | AggregatedMetrics::I64(MetricData::Histogram(_)) => Some(("histogram", false)),

        // Exponential histograms are not supported in text format
        AggregatedMetrics::F64(MetricData::ExponentialHistogram(_))
        | AggregatedMetrics::U64(MetricData::ExponentialHistogram(_))
        | AggregatedMetrics::I64(MetricData::ExponentialHistogram(_)) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_name_no_allocation_when_valid() {
        // Valid names should return Cow::Borrowed (no allocation)
        let cases = vec![
            "valid_name",
            "ValidName",
            "valid:name",
            "a",
            "_valid",
            ":valid",
            "valid123",
            "valid_name_123",
        ];

        for case in cases {
            match sanitize_name(case) {
                Cow::Borrowed(s) => assert_eq!(s, case),
                Cow::Owned(_) => panic!("Expected borrowed for valid name: {case}"),
            }
        }
    }

    #[test]
    fn test_sanitize_name_allocation_when_invalid() {
        // Invalid names should return Cow::Owned (allocation needed)
        let cases = vec![
            ("123invalid", "_23invalid"),
            ("invalid-name", "invalid_name"),
            ("invalid.name", "invalid_name"),
            ("invalid__name", "invalid_name"), // consecutive underscores
            ("invalid___name", "invalid_name"), // multiple consecutive underscores
        ];

        for (input, expected) in cases {
            match sanitize_name(input) {
                Cow::Borrowed(_) => panic!("Expected owned for invalid name: {input}"),
                Cow::Owned(s) => assert_eq!(s, expected),
            }
        }

        // Test empty string separately (it's valid and should return Borrowed)
        match sanitize_name("") {
            Cow::Borrowed(s) => assert_eq!(s, ""),
            Cow::Owned(_) => panic!("Expected borrowed for empty string"),
        }
    }

    #[test]
    fn test_convert_unit_no_allocation_when_unchanged() {
        // Units that don't need conversion should return Cow::Borrowed
        let cases = vec!["", "custom_unit", "other_unit"];

        for case in cases {
            let result = convert_unit(case);
            assert_eq!(result, Cow::Borrowed(case));
        }
    }

    #[test]
    fn test_convert_unit_no_allocation_when_converted() {
        // Units that need conversion should return appropriate result
        let cases = vec![
            ("1", "ratio"),
            ("ms", "milliseconds"),
            ("s", "seconds"),
            ("m", "meters"),
            ("kg", "kilograms"),
            ("g", "grams"),
            ("b", "bytes"),
            ("bytes", "bytes"),
            ("%", "percent"),
            ("count{packets}", "count"),
        ];

        for (input, expected) in cases {
            let result = convert_unit(input);
            match result {
                Cow::Borrowed(s) => assert_eq!(s, expected),
                Cow::Owned(_) => panic!("Expected borrowed for unchanged unit: {expected}"),
            }
        }
    }

    #[test]
    fn test_convert_unit_allocation_when_converted() {
        // Units that need conversion should return appropriate result
        let cases = vec![
            ("something{packets}else", "somethingelse"),
            ("requests/second", "requests_per_second"),
        ];

        for (input, expected) in cases {
            let result = convert_unit(input);
            match result {
                Cow::Owned(s) => assert_eq!(s, expected),
                Cow::Borrowed(_) => panic!("Expected owned for converted unit: {input}"),
            }
        }
    }

    #[test]
    fn test_escape_help_text() {
        let cases = vec![
            ("Simple description", "Simple description"),
            ("Description with\nnewline", "Description with\\nnewline"),
            ("Description with\ttab", "Description with\\ttab"),
            (
                "Description with\rcarriage return",
                "Description with\\rcarriage return",
            ),
            (
                "Description with\\backslash",
                "Description with\\\\backslash",
            ),
            (
                "Complex\nwith\ttabs\rand\\backslashes",
                "Complex\\nwith\\ttabs\\rand\\\\backslashes",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(escape_help_text(input), expected);
        }
    }

    #[test]
    fn test_write_help_comment_escapes_description() {
        let mut output = Vec::new();
        let description = "This is a description\nwith a newline";

        write_help_comment(&mut output, "test_metric", description).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(
            result,
            "# HELP test_metric This is a description\\nwith a newline\n"
        );
    }

    #[test]
    fn test_scope_labels_configuration() {
        // Test with scope labels enabled (default)
        let serializer_with_scope = PrometheusSerializer::new();
        assert!(serializer_with_scope.include_scope_labels);

        // Test with scope labels disabled
        let serializer_without_scope = PrometheusSerializer::without_scope_labels();
        assert!(!serializer_without_scope.include_scope_labels);
    }
}
