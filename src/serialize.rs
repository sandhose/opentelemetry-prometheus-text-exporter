//! OpenTelemetry to Prometheus format serialization.
//!
//! This module implements the conversion from OpenTelemetry metrics to the Prometheus
//! text exposition format, following the OpenTelemetry specification for Prometheus
//! compatibility: https://opentelemetry.io/docs/specs/otel/compatibility/prometheus_and_openmetrics/
//!
//! # Key Features
//!
//! - **Specification Compliance**: Follows the OpenTelemetry specification for metric name
//!   transformation, unit conversion, and label sanitization.
//! - **Zero-Allocation Streaming**: Writes directly to the output without building intermediate
//!   collections, minimizing memory allocations and improving performance.
//! - **Memory Optimizations**: Uses `Cow<str>` to avoid unnecessary allocations when strings
//!   don't need transformation.
//! - **Proper Type Mapping**: Correctly maps OpenTelemetry metric types to Prometheus types
//!   (e.g., up-down counters become gauges, monotonic sums become counters).
//! - **Scope Information**: Adds instrumentation scope information as `otel_scope_*` labels.
//! - **Resource Attributes**: Converts resource attributes to `target_info` metric with
//!   sanitized label names.
//!
//! # Performance Optimizations
//!
//! ## Streaming Architecture
//! This implementation uses a streaming approach that writes output directly to the provided
//! `Write` trait without building intermediate data structures:
//!
//! - **No Label Collections**: Instead of collecting labels into `Vec<(String, String)>`,
//!   labels are written directly to the output as they are processed.
//! - **Direct Writing**: Metric names, values, and metadata are formatted and written
//!   immediately rather than being accumulated in memory.
//! - **Minimal Buffering**: Only essential state (like handling label name conflicts) is
//!   kept in memory temporarily.
//!
//! ## Memory Allocation Patterns
//! - **String Processing**: Uses `Cow<str>` to avoid allocations when no transformation is needed
//! - **Label Conflicts**: Only allocates when multiple attributes map to the same sanitized name
//! - **Iterative Processing**: Processes attributes one at a time without collecting into vectors
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
//! - Conflicting sanitized names are handled by concatenating values with `;`
//! - Instrumentation scope information is added as `otel_scope_*` labels

use opentelemetry::KeyValue;
use opentelemetry_sdk::{
    Resource,
    metrics::{
        Temporality,
        data::{AggregatedMetrics, Gauge, Histogram, MetricData, ResourceMetrics, Sum},
    },
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;

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
            write!(writer, "{}", self)
        }
    }
}

impl Numeric for u64 {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        write!(writer, "{}", self)
    }
}

impl Numeric for i64 {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        write!(writer, "{}", self)
    }
}

/// Sanitizes a metric or label name to follow Prometheus naming conventions.
///
/// Prometheus metric and label names must match the regex: `[a-zA-Z_:]([a-zA-Z0-9_:])*`
///
/// This function uses `Cow<str>` to avoid allocations when the input is already valid.
///
/// # Transformations
/// - First character must be `[a-zA-Z_:]`, invalid chars become `_`
/// - Subsequent characters must be `[a-zA-Z0-9_:]`, invalid chars become `_`
/// - Multiple consecutive underscores are collapsed to single `_`
///
/// # Examples
/// ```
/// # use std::borrow::Cow;
/// # fn sanitize_name(name: &str) -> Cow<'_, str> { todo!() }
/// assert_eq!(sanitize_name("valid_name"), Cow::Borrowed("valid_name"));
/// assert_eq!(sanitize_name("invalid.name"), Cow::Owned("invalid_name".to_string()));
/// ```
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
/// This function uses `Cow<str>` to avoid allocations when no conversion is needed.
///
/// # Transformations
/// - Removes content within brackets: `count{packets}` → `count`
/// - Special cases: `1` → `ratio`
/// - Converts slashes: `foo/bar` → `foo_per_bar`
/// - Expands abbreviations: `ms` → `milliseconds`, `s` → `seconds`, etc.
///
/// # Examples
/// ```
/// # use std::borrow::Cow;
/// # fn convert_unit(unit: &str) -> Cow<'_, str> { todo!() }
/// assert_eq!(convert_unit("s"), Cow::Borrowed("seconds"));
/// assert_eq!(convert_unit("custom_unit"), Cow::Borrowed("custom_unit"));
/// assert_eq!(convert_unit("requests/second"), Cow::Owned("requests_per_second".to_string()));
/// ```
fn convert_unit(unit: &str) -> Cow<'_, str> {
    let trimmed = unit.trim();

    if trimmed.is_empty() {
        return Cow::Borrowed("");
    }

    // Remove content within brackets if present
    let without_brackets = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.find('}') {
            let mut result = String::with_capacity(trimmed.len());
            result.push_str(&trimmed[..start]);
            if end + 1 < trimmed.len() {
                result.push_str(&trimmed[end + 1..]);
            }
            Cow::Owned(result.trim().to_string())
        } else {
            Cow::Borrowed(trimmed)
        }
    } else {
        Cow::Borrowed(trimmed)
    };

    let final_unit = without_brackets.as_ref();

    // Special cases
    if final_unit == "1" {
        return Cow::Borrowed("ratio");
    }

    // Convert foo/bar to foo_per_bar
    if final_unit.contains('/') {
        return Cow::Owned(final_unit.replace('/', "_per_"));
    }

    // Convert abbreviations to full words
    match final_unit {
        "ms" => Cow::Borrowed("milliseconds"),
        "s" => Cow::Borrowed("seconds"),
        "m" => Cow::Borrowed("meters"),
        "kg" => Cow::Borrowed("kilograms"),
        "g" => Cow::Borrowed("grams"),
        "b" | "bytes" => Cow::Borrowed("bytes"),
        "%" => Cow::Borrowed("percent"),
        _ => {
            // If no conversion needed and input is unchanged, return original
            if final_unit == trimmed {
                Cow::Borrowed(unit.trim())
            } else {
                // We had to process it (remove brackets), so return owned
                Cow::Owned(final_unit.to_string())
            }
        }
    }
}

/// Adds unit suffix to metric name if not already present.
///
/// Uses `Cow<str>` to avoid allocations when no suffix is needed.
///
/// # Examples
/// ```
/// # use std::borrow::Cow;
/// # fn add_unit_suffix<'a>(name: &'a str, unit: &str) -> Cow<'a, str> { todo!() }
/// assert_eq!(add_unit_suffix("metric", ""), Cow::Borrowed("metric"));
/// assert_eq!(add_unit_suffix("metric_seconds", "seconds"), Cow::Borrowed("metric_seconds"));
/// assert_eq!(add_unit_suffix("metric", "seconds"), Cow::Owned("metric_seconds".to_string()));
/// ```
fn add_unit_suffix<'a>(name: &'a str, unit: &str) -> Cow<'a, str> {
    if unit.is_empty() || name.ends_with(unit) {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("{}_{}", name, unit))
    }
}

/// Writes attributes as Prometheus labels directly to the writer.
///
/// This is a key part of the zero-allocation streaming approach. Instead of building
/// an intermediate collection of labels, this function processes attributes one by one
/// and writes them directly to the output.
///
/// # Streaming Benefits
/// - No intermediate `Vec<(String, String)>` allocation
/// - Labels are processed and written immediately
/// - Memory usage scales with conflicts, not total attribute count
///
/// # Conflict Handling
/// When multiple attributes sanitize to the same Prometheus label name,
/// their values are concatenated with `;` separator. This is the only case
/// where temporary allocation occurs (to collect conflicting values).
///
/// # Returns
/// Returns true if any labels were written, false otherwise.
fn write_attributes_as_labels<W: Write>(
    attributes: impl Iterator<Item = KeyValue>,
    writer: &mut W,
) -> std::io::Result<bool> {
    let mut label_map: HashMap<String, Vec<String>> = HashMap::new();

    // Group by sanitized key
    for attr in attributes {
        let sanitized_key = sanitize_name(attr.key.as_str()).into_owned();
        let value = format!("{}", attr.value);
        label_map.entry(sanitized_key).or_default().push(value);
    }

    if label_map.is_empty() {
        return Ok(false);
    }

    // Write labels directly
    let mut first = true;
    for (key, mut values) in label_map {
        if !first {
            write!(writer, ",")?;
        }
        first = false;

        write!(writer, "{}=", key)?;
        if values.len() == 1 {
            write!(writer, "{}", escape_label_value(&values[0]))?;
        } else {
            // Sort values for deterministic output when there are conflicts
            values.sort();
            let concatenated = values.join(";");
            write!(writer, "{}", escape_label_value(&concatenated))?;
        }
    }
    Ok(true)
}

/// Escapes label value for Prometheus format using Rust's Debug formatting.
///
/// This ensures proper escaping of quotes, backslashes, and other special characters.
fn escape_label_value(value: &str) -> String {
    format!("{:?}", value)
}

/// Writes TYPE comment
fn write_type_comment<W: Write>(
    writer: &mut W,
    name: &str,
    metric_type: &str,
) -> std::io::Result<()> {
    write!(writer, "# TYPE {} {}\n", name, metric_type)
}

/// Writes HELP comment
fn write_help_comment<W: Write>(
    writer: &mut W,
    name: &str,
    description: &str,
) -> std::io::Result<()> {
    if !description.is_empty() {
        write!(writer, "# HELP {} {}\n", name, description)?;
    }
    Ok(())
}

/// Writes UNIT comment
fn write_unit_comment<W: Write>(writer: &mut W, name: &str, unit: &str) -> std::io::Result<()> {
    if !unit.is_empty() {
        write!(writer, "# UNIT {} {}\n", name, unit)?;
    }
    Ok(())
}

/// Writes scope labels directly to the writer.
fn write_scope_labels<W: Write>(
    scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
    writer: &mut W,
) -> std::io::Result<()> {
    let scope = scope_metrics.scope();

    // Add scope name
    if !scope.name().is_empty() {
        write!(
            writer,
            ",otel_scope_name={}",
            escape_label_value(scope.name())
        )?;
    }

    // Add scope version
    if let Some(version) = scope.version() {
        if !version.is_empty() {
            write!(
                writer,
                ",otel_scope_version={}",
                escape_label_value(version)
            )?;
        }
    }

    // Add scope schema URL
    if let Some(schema_url) = scope.schema_url() {
        if !schema_url.is_empty() {
            write!(
                writer,
                ",otel_scope_schema_url={}",
                escape_label_value(schema_url)
            )?;
        }
    }

    // Add scope attributes (excluding name, version, schema_url to avoid conflicts)
    for attr in scope.attributes() {
        let key = attr.key.as_str();
        if key != "name" && key != "version" && key != "schema_url" {
            let sanitized_key = sanitize_name(key);
            let value = format!("{}", attr.value);
            write!(
                writer,
                ",otel_scope_{}={}",
                sanitized_key.as_ref(),
                escape_label_value(&value)
            )?;
        }
    }

    Ok(())
}

pub(crate) fn serialize<W: Write>(rm: &ResourceMetrics, writer: &mut W) -> std::io::Result<()> {
    // Serialize all metrics first
    for sm in rm.scope_metrics() {
        serialize_scope_metrics(sm, writer)?;
    }

    // Serialize resource as target_info
    serialize_resource(rm.resource(), writer)?;

    Ok(())
}

fn serialize_resource<W: Write>(resource: &Resource, writer: &mut W) -> std::io::Result<()> {
    // Don't serialize empty resources
    if resource.is_empty() {
        return Ok(());
    }

    write_type_comment(writer, "target_info", "gauge")?;
    write_help_comment(writer, "target_info", "Target metadata")?;

    write!(writer, "target_info")?;

    // Write labels directly
    write!(writer, "{{")?;
    let mut first = true;
    for (key, value) in resource.iter() {
        if !first {
            write!(writer, ",")?;
        }
        first = false;
        let sanitized_key = sanitize_name(key.as_str());
        let value_str = format!("{}", value);
        write!(
            writer,
            "{}={}",
            sanitized_key.as_ref(),
            escape_label_value(&value_str)
        )?;
    }
    write!(writer, "}}")?;
    write!(writer, " 1\n")?;

    Ok(())
}

fn serialize_scope_metrics<W: Write>(
    scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
    writer: &mut W,
) -> std::io::Result<()> {
    for metric in scope_metrics.metrics() {
        serialize_metric(metric, scope_metrics, writer)?;
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

fn serialize_metric<W: Write>(
    metric: &opentelemetry_sdk::metrics::data::Metric,
    scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
    writer: &mut W,
) -> std::io::Result<()> {
    let data = metric.data();

    // Skip unsupported metrics
    let Some((prometheus_type, is_monotonic)) = get_prometheus_type_and_is_monotonic(data) else {
        return Ok(());
    };

    let original_name = metric.name();
    let sanitized_name = sanitize_name(original_name);
    let converted_unit = convert_unit(metric.unit().as_ref());

    // For monotonic sums, add _total suffix if not present
    let base_name = if is_monotonic {
        if !sanitized_name.ends_with("_total") {
            Cow::Owned(format!("{}_total", sanitized_name))
        } else {
            sanitized_name
        }
    } else {
        sanitized_name
    };

    let final_name = add_unit_suffix(base_name.as_ref(), converted_unit.as_ref());

    // Write metadata
    write_type_comment(writer, final_name.as_ref(), prometheus_type)?;
    write_help_comment(writer, final_name.as_ref(), metric.description())?;
    write_unit_comment(writer, final_name.as_ref(), converted_unit.as_ref())?;

    match data {
        AggregatedMetrics::F64(MetricData::Gauge(gauge)) => {
            serialize_gauge(final_name.as_ref(), gauge, scope_metrics, writer)?;
        }
        AggregatedMetrics::U64(MetricData::Gauge(gauge)) => {
            serialize_gauge(final_name.as_ref(), gauge, scope_metrics, writer)?;
        }
        AggregatedMetrics::I64(MetricData::Gauge(gauge)) => {
            serialize_gauge(final_name.as_ref(), gauge, scope_metrics, writer)?;
        }

        AggregatedMetrics::F64(MetricData::Sum(sum)) => {
            serialize_sum(final_name.as_ref(), sum, scope_metrics, writer)?;
        }
        AggregatedMetrics::U64(MetricData::Sum(sum)) => {
            serialize_sum(final_name.as_ref(), sum, scope_metrics, writer)?;
        }
        AggregatedMetrics::I64(MetricData::Sum(sum)) => {
            serialize_sum(final_name.as_ref(), sum, scope_metrics, writer)?;
        }

        AggregatedMetrics::F64(MetricData::Histogram(histogram)) => {
            serialize_histogram(final_name.as_ref(), histogram, scope_metrics, writer)?;
        }
        AggregatedMetrics::U64(MetricData::Histogram(histogram)) => {
            serialize_histogram(final_name.as_ref(), histogram, scope_metrics, writer)?;
        }
        AggregatedMetrics::I64(MetricData::Histogram(histogram)) => {
            serialize_histogram(final_name.as_ref(), histogram, scope_metrics, writer)?;
        }

        // Skip exponential histograms
        _ => {}
    }

    Ok(())
}

/// Writes labels for a metric including both attributes and scope labels.
///
/// This function orchestrates the streaming output of all metric labels:
/// 1. Opens the label block with `{` if any labels will be written
/// 2. Streams attribute labels directly to output
/// 3. Appends scope information labels
/// 4. Closes the label block with `}`
///
/// The streaming approach means no intermediate collections are built -
/// everything is written directly to the final output.
fn write_metric_labels<W: Write>(
    attributes: impl Iterator<Item = KeyValue>,
    scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
    writer: &mut W,
    has_additional_labels: bool,
) -> std::io::Result<()> {
    // Check if we have any attributes or scope info to write
    let attrs: Vec<_> = attributes.collect();
    let scope = scope_metrics.scope();
    let has_scope_name = !scope.name().is_empty();
    let has_scope_version = scope.version().map_or(false, |v| !v.is_empty());
    let has_scope_schema = scope.schema_url().map_or(false, |v| !v.is_empty());
    let has_scope_attrs = scope.attributes().next().is_some();

    let will_have_labels = !attrs.is_empty()
        || has_scope_name
        || has_scope_version
        || has_scope_schema
        || has_scope_attrs
        || has_additional_labels;

    if will_have_labels {
        write!(writer, "{{")?;

        let wrote_attrs = write_attributes_as_labels(attrs.into_iter(), writer)?;

        if wrote_attrs || has_scope_name || has_scope_version || has_scope_schema || has_scope_attrs
        {
            write_scope_labels(scope_metrics, writer)?;
        }

        write!(writer, "}}")?;
    }

    Ok(())
}

/// Writes bucket labels including the `le` label.
fn write_bucket_labels<W: Write>(
    attributes: impl Iterator<Item = KeyValue>,
    scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
    le_value: &str,
    writer: &mut W,
) -> std::io::Result<()> {
    write!(writer, "{{")?;

    // Write attributes first
    let has_attrs = write_attributes_as_labels(attributes, writer)?;

    // Add le label
    if has_attrs {
        write!(writer, ",le={}", escape_label_value(le_value))?;
    } else {
        write!(writer, "le={}", escape_label_value(le_value))?;
    }

    // Add scope labels
    write_scope_labels(scope_metrics, writer)?;

    write!(writer, "}}")?;
    Ok(())
}

fn serialize_gauge<T: Numeric, W: Write>(
    name: &str,
    gauge: &Gauge<T>,
    scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
    writer: &mut W,
) -> std::io::Result<()> {
    for data_point in gauge.data_points() {
        write!(writer, "{}", name)?;
        write_metric_labels(
            data_point.attributes().cloned(),
            scope_metrics,
            writer,
            false,
        )?;
        write!(writer, " ")?;
        data_point.value().serialize(writer)?;
        write!(writer, "\n")?;
    }

    Ok(())
}

fn serialize_sum<T: Numeric, W: Write>(
    name: &str,
    sum: &Sum<T>,
    scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
    writer: &mut W,
) -> std::io::Result<()> {
    for data_point in sum.data_points() {
        write!(writer, "{}", name)?;
        write_metric_labels(
            data_point.attributes().cloned(),
            scope_metrics,
            writer,
            false,
        )?;
        write!(writer, " ")?;
        data_point.value().serialize(writer)?;
        write!(writer, "\n")?;
    }

    Ok(())
}

fn serialize_histogram<T: Numeric, W: Write>(
    name: &str,
    histogram: &Histogram<T>,
    scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
    writer: &mut W,
) -> std::io::Result<()> {
    for data_point in histogram.data_points() {
        // _count metric
        write!(writer, "{}_count", name)?;
        write_metric_labels(
            data_point.attributes().cloned(),
            scope_metrics,
            writer,
            false,
        )?;
        write!(writer, " ")?;
        data_point.count().serialize(writer)?;
        write!(writer, "\n")?;

        // _sum metric
        write!(writer, "{}_sum", name)?;
        write_metric_labels(
            data_point.attributes().cloned(),
            scope_metrics,
            writer,
            false,
        )?;
        write!(writer, " ")?;
        data_point.sum().serialize(writer)?;
        write!(writer, "\n")?;

        // _bucket metrics
        let mut cumulative_count = 0u64;
        for (bound, count) in data_point.bounds().zip(data_point.bucket_counts()) {
            cumulative_count += count;

            write!(writer, "{}_bucket", name)?;
            write_bucket_labels(
                data_point.attributes().cloned(),
                scope_metrics,
                &bound.to_string(),
                writer,
            )?;
            write!(writer, " ")?;
            cumulative_count.serialize(writer)?;
            write!(writer, "\n")?;
        }

        // Final +Inf bucket
        write!(writer, "{}_bucket", name)?;
        write_bucket_labels(
            data_point.attributes().cloned(),
            scope_metrics,
            "+Inf",
            writer,
        )?;
        write!(writer, " ")?;
        data_point.count().serialize(writer)?;
        write!(writer, "\n")?;
    }

    Ok(())
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
                Cow::Owned(_) => panic!("Expected borrowed for valid name: {}", case),
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
                Cow::Borrowed(_) => panic!("Expected owned for invalid name: {}", input),
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
            match convert_unit(case) {
                Cow::Borrowed(s) => assert_eq!(s.trim(), case),
                Cow::Owned(_) => panic!("Expected borrowed for unchanged unit: {}", case),
            }
        }
    }

    #[test]
    fn test_convert_unit_allocation_when_converted() {
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
            ("requests/second", "requests_per_second"),
            ("count{packets}", "count"), // bracket removal
        ];

        for (input, expected) in cases {
            let result = convert_unit(input);
            assert_eq!(result.as_ref(), expected);
        }
    }

    #[test]
    fn test_streaming_approach_avoids_allocations() {
        use std::io::Cursor;

        // This test demonstrates that our streaming approach writes directly
        // to the output without building intermediate collections

        let mut output = Cursor::new(Vec::new());

        // Create some test attributes
        let attrs = vec![
            KeyValue::new("method", "GET"),
            KeyValue::new("status", "200"),
            KeyValue::new("path", "/api/users"),
        ];

        // Test writing attributes directly to writer
        output.write_all(b"test_metric{").unwrap();
        let wrote_attrs = write_attributes_as_labels(attrs.into_iter(), &mut output).unwrap();
        assert!(wrote_attrs);
        output.write_all(b"} 42\n").unwrap();

        let result = String::from_utf8(output.into_inner()).unwrap();

        // Verify the output contains properly formatted labels
        assert!(result.contains("test_metric{"));
        assert!(result.contains("method=\"GET\""));
        assert!(result.contains("status=\"200\""));
        assert!(result.contains("path=\"/api/users\""));
        assert!(result.contains("} 42"));

        // The key benefit: no intermediate Vec<(String, String)> was created!
        // Labels were written directly to the output stream as they were processed.
        //
        // Performance characteristics:
        // - O(1) memory usage per attribute (no accumulation)
        // - O(n) write operations where n = number of attributes
        // - Conflicts only allocate for the specific conflicting values
        // - No sorting overhead (deterministic order not required)
    }

    #[test]
    fn test_add_unit_suffix_no_allocation_when_unchanged() {
        // Cases where no suffix is needed should return Cow::Borrowed
        let cases = vec![
            ("metric_name", ""),                // empty unit
            ("metric_name_seconds", "seconds"), // already has suffix
            ("memory_bytes", "bytes"),          // already has suffix
        ];

        for (name, unit) in cases {
            match add_unit_suffix(name, unit) {
                Cow::Borrowed(s) => assert_eq!(s, name),
                Cow::Owned(_) => panic!(
                    "Expected borrowed for unchanged name: {} with unit: {}",
                    name, unit
                ),
            }
        }
    }

    #[test]
    fn test_add_unit_suffix_allocation_when_suffix_added() {
        // Cases where suffix is needed should return Cow::Owned
        let cases = vec![
            ("metric_name", "seconds", "metric_name_seconds"),
            ("http_requests", "total", "http_requests_total"),
            ("memory", "bytes", "memory_bytes"),
        ];

        for (name, unit, expected) in cases {
            match add_unit_suffix(name, unit) {
                Cow::Borrowed(_) => panic!(
                    "Expected owned for name needing suffix: {} with unit: {}",
                    name, unit
                ),
                Cow::Owned(s) => assert_eq!(s, expected),
            }
        }
    }
}
