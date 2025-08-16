use std::io::Write;
use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use opentelemetry::KeyValue;
use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use prometheus::{Encoder, Registry, TextEncoder};

// Comparison between implementations:
// - opentelemetry_prometheus_text_exporter (our implementation)
// - opentelemetry_prometheus (existing implementation)

/// A fake writer that discards all data (like /dev/null)
/// This ensures we're only benchmarking serialization, not I/O performance
struct DevNull;

impl Write for DevNull {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Use black_box to prevent the compiler from optimizing away the write
        black_box(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Setup metrics using our new implementation
fn setup_new_implementation(
    num_metrics: usize,
    cardinality: usize,
) -> (
    Arc<SdkMeterProvider>,
    opentelemetry_prometheus_text_exporter::PrometheusExporter,
) {
    let exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::new();

    let provider = Arc::new(
        SdkMeterProvider::builder()
            .with_resource(
                Resource::builder_empty()
                    .with_attribute(KeyValue::new("service.name", "comparison-service"))
                    .with_attribute(KeyValue::new("service.version", "1.0.0"))
                    .build(),
            )
            .with_reader(exporter.clone())
            .build(),
    );

    let meter = provider.meter("comparison");

    // Create metrics with realistic patterns
    for metric_idx in 0..num_metrics {
        let counter = meter
            .u64_counter(format!("requests_metric_{metric_idx}"))
            .with_description(format!("Counter metric number {metric_idx}"))
            .with_unit("{request}")
            .build();

        // Create different label combinations for cardinality
        for label_idx in 0..cardinality {
            let labels = vec![
                KeyValue::new(
                    "method",
                    if label_idx % 4 == 0 {
                        "GET"
                    } else if label_idx % 4 == 1 {
                        "POST"
                    } else if label_idx % 4 == 2 {
                        "PUT"
                    } else {
                        "DELETE"
                    },
                ),
                KeyValue::new("status", format!("{}", 200 + (label_idx % 5) * 100)),
                KeyValue::new(
                    "endpoint",
                    format!("/api/v{}/resource_{}", (label_idx % 3) + 1, label_idx % 10),
                ),
                KeyValue::new(
                    "region",
                    if label_idx % 2 == 0 {
                        "us-east-1"
                    } else {
                        "us-west-2"
                    },
                ),
            ];

            // Record some data
            counter.add(1 + (label_idx as u64), &labels);
        }
    }

    (provider, exporter)
}

/// Setup metrics using the existing opentelemetry-prometheus implementation
fn setup_existing_implementation(
    num_metrics: usize,
    cardinality: usize,
) -> (Arc<SdkMeterProvider>, Registry) {
    let registry = Registry::new();
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .build()
        .unwrap();

    let provider = Arc::new(
        SdkMeterProvider::builder()
            .with_resource(
                Resource::builder_empty()
                    .with_attribute(KeyValue::new("service.name", "comparison-service"))
                    .with_attribute(KeyValue::new("service.version", "1.0.0"))
                    .build(),
            )
            .with_reader(exporter)
            .build(),
    );

    let meter = provider.meter("comparison");

    // Create the same metrics as in the new implementation
    for metric_idx in 0..num_metrics {
        let counter = meter
            .u64_counter(format!("requests_metric_{metric_idx}"))
            .with_description(format!("Counter metric number {metric_idx}"))
            .with_unit("{request}")
            .build();

        // Create the same label combinations
        for label_idx in 0..cardinality {
            let labels = vec![
                KeyValue::new(
                    "method",
                    if label_idx % 4 == 0 {
                        "GET"
                    } else if label_idx % 4 == 1 {
                        "POST"
                    } else if label_idx % 4 == 2 {
                        "PUT"
                    } else {
                        "DELETE"
                    },
                ),
                KeyValue::new("status", format!("{}", 200 + (label_idx % 5) * 100)),
                KeyValue::new(
                    "endpoint",
                    format!("/api/v{}/resource_{}", (label_idx % 3) + 1, label_idx % 10),
                ),
                KeyValue::new(
                    "region",
                    if label_idx % 2 == 0 {
                        "us-east-1"
                    } else {
                        "us-west-2"
                    },
                ),
            ];

            // Record the same data
            counter.add(1 + (label_idx as u64), &labels);
        }
    }

    (provider, registry)
}

/// Compare the two implementations with a realistic workload
fn bench_implementation_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("implementation_comparison");

    // Use a realistic workload: 25 metrics with 20 cardinality each (500 time
    // series)
    let num_metrics = 25;
    let cardinality = 20;
    let total_time_series = num_metrics * cardinality;

    // Benchmark new implementation
    group.throughput(Throughput::Elements(total_time_series as u64));
    group.bench_function("new_implementation", |b| {
        let (_provider, exporter) = setup_new_implementation(num_metrics, cardinality);

        b.iter(|| {
            let mut writer = DevNull;
            exporter.export(&mut writer).unwrap();
        });
    });

    // Benchmark existing implementation
    group.throughput(Throughput::Elements(total_time_series as u64));
    group.bench_function("existing_implementation", |b| {
        let (_provider, registry) = setup_existing_implementation(num_metrics, cardinality);

        b.iter(|| {
            let mut writer = DevNull;
            let encoder = TextEncoder::new();
            let metric_families = registry.gather();
            encoder.encode(&metric_families, &mut writer).unwrap();
        });
    });

    group.finish();
}

/// Compare implementations across different workload sizes
fn bench_scaling_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling_comparison");

    // Test different scales to see how implementations compare
    let scenarios = vec![
        ("small", 5, 5),    // 25 time series
        ("medium", 10, 10), // 100 time series
        ("large", 20, 25),  // 500 time series
    ];

    for (name, num_metrics, cardinality) in scenarios {
        let total_time_series = num_metrics * cardinality;
        group.throughput(Throughput::Elements(total_time_series as u64));

        // Benchmark new implementation
        group.bench_with_input(
            BenchmarkId::new("new", name),
            &(num_metrics, cardinality),
            |b, &(num_metrics, cardinality)| {
                let (_provider, exporter) = setup_new_implementation(num_metrics, cardinality);

                b.iter(|| {
                    let mut writer = DevNull;
                    exporter.export(&mut writer).unwrap();
                });
            },
        );

        // Benchmark existing implementation
        group.bench_with_input(
            BenchmarkId::new("existing", name),
            &(num_metrics, cardinality),
            |b, &(num_metrics, cardinality)| {
                let (_provider, registry) = setup_existing_implementation(num_metrics, cardinality);

                b.iter(|| {
                    let mut writer = DevNull;
                    let encoder = TextEncoder::new();
                    let metric_families = registry.gather();
                    encoder.encode(&metric_families, &mut writer).unwrap();
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_implementation_comparison,
    bench_scaling_comparison
);
criterion_main!(benches);
