use opentelemetry::{Key, KeyValue, Value};
use opentelemetry_sdk::{
    Resource,
    metrics::data::{AggregatedMetrics, Gauge, Histogram, MetricData, ResourceMetrics, Sum},
};

trait Numeric: Copy {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()>;
}

impl Numeric for f64 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        write!(writer, "{self}")?;
        Ok(())
    }
}

impl Numeric for u64 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        write!(writer, "{self}")?;
        Ok(())
    }
}

impl Numeric for i64 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        write!(writer, "{self}")?;
        Ok(())
    }
}

pub(crate) fn serialize<W: std::io::Write>(
    rm: &ResourceMetrics,
    writer: &mut W,
) -> std::io::Result<()> {
    for sm in rm.scope_metrics() {
        serialize_scope_metrics(sm, writer)?;
    }

    serialize_resource(rm.resource(), writer)?;
    Ok(())
}

fn serialize_resource<W: std::io::Write>(
    resource: &Resource,
    writer: &mut W,
) -> std::io::Result<()> {
    writer.write_all(b"# TYPE target info\n")?;
    writer.write_all(b"# HELP target Target metadata\n")?;

    writer.write_all(b"target_info")?;

    let mut has_written = false;
    for (key, value) in resource.iter() {
        if !has_written {
            has_written = true;
            writer.write_all(b"{")?;
        } else {
            writer.write_all(b",")?;
        }

        serialize_key_value(key, value, writer)?;
    }

    if has_written {
        writer.write_all(b"}")?;
    }

    writer.write_all(b" 1\n")?;

    Ok(())
}

fn serialize_scope_metrics<W: std::io::Write>(
    scope_metrics: &opentelemetry_sdk::metrics::data::ScopeMetrics,
    writer: &mut W,
) -> std::io::Result<()> {
    for m in scope_metrics.metrics() {
        serialize_metric(m, writer)?;
    }
    Ok(())
}

fn kind(data: &AggregatedMetrics) -> Option<&'static str> {
    match data {
        AggregatedMetrics::F64(MetricData::Gauge(_))
        | AggregatedMetrics::U64(MetricData::Gauge(_))
        | AggregatedMetrics::I64(MetricData::Gauge(_)) => Some("gauge"),

        AggregatedMetrics::F64(MetricData::Sum(_))
        | AggregatedMetrics::U64(MetricData::Sum(_))
        | AggregatedMetrics::I64(MetricData::Sum(_)) => Some("counter"),

        AggregatedMetrics::F64(MetricData::Histogram(_))
        | AggregatedMetrics::U64(MetricData::Histogram(_))
        | AggregatedMetrics::I64(MetricData::Histogram(_)) => Some("histogram"),

        AggregatedMetrics::F64(MetricData::ExponentialHistogram(_))
        | AggregatedMetrics::U64(MetricData::ExponentialHistogram(_))
        | AggregatedMetrics::I64(MetricData::ExponentialHistogram(_)) => None, // Unsupported
    }
}

fn serialize_metric<W: std::io::Write>(
    metric: &opentelemetry_sdk::metrics::data::Metric,
    writer: &mut W,
) -> std::io::Result<()> {
    // TODO: Transform the name according to
    // https://opentelemetry.io/docs/specs/otel/compatibility/prometheus_and_openmetrics/#metric-metadata-1
    let name = metric.name();

    let data = metric.data();
    let Some(kind) = kind(data) else {
        // Skipping unsupported metric
        return Ok(());
    };

    writer.write_all(b"# TYPE ")?;
    writer.write_all(name.as_bytes())?;
    writer.write_all(b" ")?;
    writer.write_all(kind.as_bytes())?;
    writer.write_all(b"\n")?;

    let description = metric.description();
    if !description.is_empty() {
        writer.write_all(b"# HELP ")?;
        writer.write_all(name.as_bytes())?;
        writer.write_all(b" ")?;
        writer.write_all(description.as_bytes())?;
        writer.write_all(b"\n")?;
    }

    // TODO: Transform the unit according to
    // https://opentelemetry.io/docs/specs/otel/compatibility/prometheus_and_openmetrics/#metric-metadata-1
    let unit = metric.unit();
    if !unit.is_empty() {
        writer.write_all(b"# UNIT ")?;
        writer.write_all(name.as_bytes())?;
        writer.write_all(b" ")?;
        writer.write_all(unit.as_bytes())?;
        writer.write_all(b"\n")?;
    }

    match data {
        AggregatedMetrics::F64(MetricData::Gauge(gauge)) => serialize_gauge(name, gauge, writer)?,
        AggregatedMetrics::U64(MetricData::Gauge(gauge)) => serialize_gauge(name, gauge, writer)?,
        AggregatedMetrics::I64(MetricData::Gauge(gauge)) => serialize_gauge(name, gauge, writer)?,

        AggregatedMetrics::F64(MetricData::Sum(sum)) => serialize_sum(name, sum, writer)?,
        AggregatedMetrics::U64(MetricData::Sum(sum)) => serialize_sum(name, sum, writer)?,
        AggregatedMetrics::I64(MetricData::Sum(sum)) => serialize_sum(name, sum, writer)?,

        AggregatedMetrics::F64(MetricData::Histogram(h)) => serialize_histogram(name, h, writer)?,
        AggregatedMetrics::U64(MetricData::Histogram(h)) => serialize_histogram(name, h, writer)?,
        AggregatedMetrics::I64(MetricData::Histogram(h)) => serialize_histogram(name, h, writer)?,

        AggregatedMetrics::F64(MetricData::ExponentialHistogram(_))
        | AggregatedMetrics::U64(MetricData::ExponentialHistogram(_))
        | AggregatedMetrics::I64(MetricData::ExponentialHistogram(_)) => {
            // Unsupported, should not happen
            unreachable!()
        }
    }

    writer.write_all(b"\n")?;

    Ok(())
}

fn serialize_key<W: std::io::Write>(key: &Key, writer: &mut W) -> std::io::Result<()> {
    // TODO: Transform the key according to
    // https://opentelemetry.io/docs/specs/otel/compatibility/prometheus_and_openmetrics/#metric-metadata-1
    write!(writer, "{key}")?;
    Ok(())
}

fn serialize_value(value: &Value, writer: &mut impl std::io::Write) -> std::io::Result<()> {
    // This adds quotes around the value and escapes quotes inside the value
    let str_value = format!("{value}");
    write!(writer, "{str_value:?}")?;
    Ok(())
}

fn serialize_key_value<W: std::io::Write>(
    key: &Key,
    value: &Value,
    writer: &mut W,
) -> std::io::Result<()> {
    serialize_key(key, writer)?;
    writer.write_all(b"=")?;
    serialize_value(value, writer)?;
    Ok(())
}

fn serialize_attributes<'a, W: std::io::Write>(
    attributes: impl Iterator<Item = &'a KeyValue>,
    writer: &mut W,
) -> std::io::Result<()> {
    let mut has_written = false;
    for attr in attributes {
        if !has_written {
            has_written = true;
            writer.write_all(b"{")?;
        } else {
            writer.write_all(b",")?;
        }

        serialize_key_value(&attr.key, &attr.value, writer)?;
    }

    if has_written {
        writer.write_all(b"}")?;
    }

    Ok(())
}

fn serialize_gauge<T: Numeric, W: std::io::Write>(
    name: &str,
    gauge: &Gauge<T>,
    writer: &mut W,
) -> std::io::Result<()> {
    for data_point in gauge.data_points() {
        writer.write_all(name.as_bytes())?;
        serialize_attributes(data_point.attributes(), writer)?;
        writer.write_all(b" ")?;
        data_point.value().serialize(writer)?;
        writer.write_all(b"\n")?;
    }

    Ok(())
}

fn serialize_sum<T: Numeric, W: std::io::Write>(
    name: &str,
    sum: &Sum<T>,
    writer: &mut W,
) -> std::io::Result<()> {
    for data_point in sum.data_points() {
        writer.write_all(name.as_bytes())?;
        serialize_attributes(data_point.attributes(), writer)?;
        writer.write_all(b" ")?;
        data_point.value().serialize(writer)?;
        writer.write_all(b"\n")?;
    }

    Ok(())
}

fn serialize_histogram<T: Numeric, W: std::io::Write>(
    name: &str,
    histogram: &Histogram<T>,
    writer: &mut W,
) -> std::io::Result<()> {
    for data_point in histogram.data_points() {
        writer.write_all(name.as_bytes())?;
        writer.write_all(b"_total")?;
        serialize_attributes(data_point.attributes(), writer)?;
        writer.write_all(b" ")?;
        data_point.sum().serialize(writer)?;
        writer.write_all(b"\n")?;

        writer.write_all(name.as_bytes())?;
        writer.write_all(b"_count")?;
        serialize_attributes(data_point.attributes(), writer)?;
        writer.write_all(b" ")?;
        data_point.count().serialize(writer)?;
        writer.write_all(b"\n")?;

        let mut cumulative = 0;
        for (bound, value) in data_point.bounds().zip(data_point.bucket_counts()) {
            cumulative += value;
            writer.write_all(name.as_bytes())?;
            writer.write_all(b"_bucket")?;
            serialize_attributes(
                data_point
                    .attributes()
                    .chain(std::iter::once(&KeyValue::new("le", bound))),
                writer,
            )?;
            writer.write_all(b" ")?;
            cumulative.serialize(writer)?;
            writer.write_all(b"\n")?;
        }
    }

    Ok(())
}
