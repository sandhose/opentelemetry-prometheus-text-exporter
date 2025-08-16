use std::io::Write;
use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use opentelemetry::KeyValue;
use opentelemetry::metrics::MeterProvider;
use opentelemetry_prometheus_text_exporter::PrometheusExporter;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;

// # OpenTelemetry Prometheus Exporter Benchmarks
//
// This benchmark suite tests the performance characteristics of the
// OpenTelemetry Prometheus exporter, with a focus on how serialization scales
// with:
// - Number of metrics
// - Label cardinality (number of unique label combinations)
// - Different configuration options
// - Memory optimization patterns
//
// ## Focus on Counter Metrics
//
// These benchmarks currently focus on **counter metrics** because they:
// - Are the most commonly used metric type in production systems
// - Exercise all the complex serialization paths (name sanitization, unit
//   conversion, suffixes)
// - Have predictable serialization patterns that make performance analysis
//   easier
// - Represent a good baseline for understanding overall exporter performance
//
// Future enhancements could add benchmarks for histograms and gauges, which
// have different serialization characteristics (histograms generate multiple
// time series per metric, gauges have simpler serialization).
//
// ## Methodology
//
// All benchmarks use a fake writer (DevNull) that discards data but prevents
// compiler optimizations via `black_box`. This ensures we're measuring only
// serialization performance, not I/O performance. The metrics are pre-created
// during setup and the provider is kept alive during measurement to ensure
// realistic collection behavior.
//
// ## Performance Expectations
//
// Based on the benchmark design and OpenTelemetry SDK architecture:
// - Performance should scale roughly linearly with both metric count and
//   cardinality
// - Configuration options should have minimal impact (< 5% difference)
// - Memory optimizations (Cow<str>) should provide measurable benefits for
//   clean names
// - Realistic workloads should complete in microseconds to low milliseconds

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

/// Create a provider with metrics and return both provider and exporter
/// to ensure proper lifetime management
fn setup_metrics_with_counters(
    num_metrics: usize,
    cardinality_per_metric: usize,
    exporter: PrometheusExporter,
) -> (Arc<SdkMeterProvider>, PrometheusExporter) {
    let provider = SdkMeterProvider::builder()
        .with_resource(
            Resource::builder_empty()
                .with_attribute(KeyValue::new("service.name", "benchmark-service"))
                .with_attribute(KeyValue::new("service.version", "1.0.0"))
                .build(),
        )
        .with_reader(exporter.clone())
        .build();

    let meter = provider.meter("benchmark");

    // Create multiple metrics with different cardinalities
    for metric_idx in 0..num_metrics {
        let counter = meter
            .u64_counter(format!("requests_metric_{metric_idx}"))
            .with_description(format!("Counter metric number {metric_idx}"))
            .with_unit("{request}")
            .build();

        // Create different label combinations for cardinality
        for label_idx in 0..cardinality_per_metric {
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

    (Arc::new(provider), exporter)
}

/// Benchmark serialization performance with varying number of metrics
fn bench_metrics_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics_count");

    // Test with fixed cardinality (10) but varying number of metrics
    let cardinality = 10;
    let metric_counts = vec![1, 5, 10, 25, 50, 100];

    for num_metrics in metric_counts {
        // Calculate total time series (metrics * cardinality)
        let total_time_series = num_metrics * cardinality;
        group.throughput(Throughput::Elements(total_time_series as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(num_metrics),
            &num_metrics,
            |b, &num_metrics| {
                let exporter = PrometheusExporter::new();
                let (_provider, setup_exporter) =
                    setup_metrics_with_counters(num_metrics, cardinality, exporter);

                b.iter(|| {
                    let mut writer = DevNull;
                    setup_exporter.export(&mut writer).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark serialization performance with varying cardinality per metric
fn bench_cardinality(c: &mut Criterion) {
    let mut group = c.benchmark_group("cardinality");

    // Test with fixed number of metrics (5) but varying cardinality
    let num_metrics = 5;
    let cardinalities = vec![1, 5, 10, 25, 50, 100, 250];

    for cardinality in cardinalities {
        // Calculate total time series (metrics * cardinality)
        let total_time_series = num_metrics * cardinality;
        group.throughput(Throughput::Elements(total_time_series as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(cardinality),
            &cardinality,
            |b, &cardinality| {
                let exporter = PrometheusExporter::new();
                let (_provider, setup_exporter) =
                    setup_metrics_with_counters(num_metrics, cardinality, exporter);

                b.iter(|| {
                    let mut writer = DevNull;
                    setup_exporter.export(&mut writer).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark different exporter configurations
fn bench_configurations(c: &mut Criterion) {
    let mut group = c.benchmark_group("configurations");

    let num_metrics = 10;
    let cardinality = 20;

    // Default configuration
    group.bench_function("default", |b| {
        let exporter = PrometheusExporter::new();
        let (_provider, setup_exporter) =
            setup_metrics_with_counters(num_metrics, cardinality, exporter);

        b.iter(|| {
            let mut writer = DevNull;
            setup_exporter.export(&mut writer).unwrap();
        });
    });

    // Without units
    group.bench_function("without_units", |b| {
        let exporter = PrometheusExporter::builder().without_units().build();
        let (_provider, setup_exporter) =
            setup_metrics_with_counters(num_metrics, cardinality, exporter);

        b.iter(|| {
            let mut writer = DevNull;
            setup_exporter.export(&mut writer).unwrap();
        });
    });

    // Without counter suffixes
    group.bench_function("without_counter_suffixes", |b| {
        let exporter = PrometheusExporter::builder()
            .without_counter_suffixes()
            .build();
        let (_provider, setup_exporter) =
            setup_metrics_with_counters(num_metrics, cardinality, exporter);

        b.iter(|| {
            let mut writer = DevNull;
            setup_exporter.export(&mut writer).unwrap();
        });
    });

    // Without target info
    group.bench_function("without_target_info", |b| {
        let exporter = PrometheusExporter::builder().without_target_info().build();
        let (_provider, setup_exporter) =
            setup_metrics_with_counters(num_metrics, cardinality, exporter);

        b.iter(|| {
            let mut writer = DevNull;
            setup_exporter.export(&mut writer).unwrap();
        });
    });

    // Without scope info
    group.bench_function("without_scope_info", |b| {
        let exporter = PrometheusExporter::builder().without_scope_info().build();
        let (_provider, setup_exporter) =
            setup_metrics_with_counters(num_metrics, cardinality, exporter);

        b.iter(|| {
            let mut writer = DevNull;
            setup_exporter.export(&mut writer).unwrap();
        });
    });

    // All optimizations enabled
    group.bench_function("minimal", |b| {
        let exporter = PrometheusExporter::builder()
            .without_units()
            .without_counter_suffixes()
            .without_target_info()
            .without_scope_info()
            .build();
        let (_provider, setup_exporter) =
            setup_metrics_with_counters(num_metrics, cardinality, exporter);

        b.iter(|| {
            let mut writer = DevNull;
            setup_exporter.export(&mut writer).unwrap();
        });
    });

    group.finish();
}

/// Benchmark scalability with realistic workload scenarios
fn bench_realistic_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("realistic_scenarios");

    let scenarios = vec![
        ("small_service", 10, 5),      // 50 time series
        ("medium_service", 50, 20),    // 1,000 time series
        ("large_service", 100, 50),    // 5,000 time series
        ("high_cardinality", 20, 200), // 4,000 time series
    ];

    for (name, num_metrics, cardinality) in scenarios {
        let total_time_series = num_metrics * cardinality;
        group.throughput(Throughput::Elements(total_time_series as u64));

        group.bench_function(name, |b| {
            let exporter = PrometheusExporter::new();
            let (_provider, setup_exporter) =
                setup_metrics_with_counters(num_metrics, cardinality, exporter);

            b.iter(|| {
                let mut writer = DevNull;
                setup_exporter.export(&mut writer).unwrap();
            });
        });
    }

    group.finish();
}

/// Benchmark scaling with total time series count (metrics × cardinality)
fn bench_total_time_series(c: &mut Criterion) {
    let mut group = c.benchmark_group("total_time_series");

    #[derive(Clone, Copy)]
    struct Scenario(usize, usize);

    impl std::fmt::Display for Scenario {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "metrics: {}, cardinality: {}", self.0, self.1)
        }
    }

    // Test different combinations that result in similar total time series counts
    let scenarios = vec![
        Scenario(10, 10),  // 100 time series
        Scenario(20, 10),  // 200 time series
        Scenario(25, 20),  // 500 time series
        Scenario(50, 20),  // 1,000 time series
        Scenario(100, 20), // 2,000 time series
        Scenario(100, 50), // 5,000 time series
        Scenario(200, 50), // 10,000 time series
    ];

    for scenario in scenarios {
        group.throughput(Throughput::Elements((scenario.0 * scenario.1) as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(scenario),
            &scenario,
            |b, &Scenario(num_metrics, cardinality)| {
                let exporter = PrometheusExporter::new();
                let (_provider, setup_exporter) =
                    setup_metrics_with_counters(num_metrics, cardinality, exporter);

                b.iter(|| {
                    let mut writer = DevNull;
                    setup_exporter.export(&mut writer).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory optimization patterns (Cow<str> usage)
fn bench_memory_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_patterns");

    // Test with metrics that have "good" names (no sanitization needed)
    group.bench_function("clean_metric_names", |b| {
        let exporter = PrometheusExporter::new();
        let provider = Arc::new(
            SdkMeterProvider::builder()
                .with_resource(Resource::builder_empty().build())
                .with_reader(exporter.clone())
                .build(),
        );

        let meter = provider.meter("benchmark");

        // Create metrics with names that don't need sanitization
        for i in 0..20 {
            let counter = meter
                .u64_counter(format!("clean_metric_name_{i}"))
                .with_unit("1")
                .build();

            for j in 0..10 {
                counter.add(1, &[KeyValue::new("label", format!("value_{j}"))]);
            }
        }

        b.iter(|| {
            let _provider = provider.clone();
            let mut writer = DevNull;
            exporter.export(&mut writer).unwrap();
        });
    });

    // Test with metrics that need sanitization (should trigger Cow::Owned)
    group.bench_function("dirty_metric_names", |b| {
        let exporter = PrometheusExporter::new();
        let provider = Arc::new(
            SdkMeterProvider::builder()
                .with_resource(Resource::builder_empty().build())
                .with_reader(exporter.clone())
                .build(),
        );

        let meter = provider.meter("benchmark");

        // Create metrics with names that need sanitization
        for i in 0..20 {
            let counter = meter
                .u64_counter(format!("dirty.metric-name@{i}#invalid"))
                .with_unit("ms")
                .build();

            for j in 0..10 {
                counter.add(1, &[KeyValue::new("method", format!("POST_{j}"))]);
            }
        }

        b.iter(|| {
            let _provider = provider.clone();
            let mut writer = DevNull;
            exporter.export(&mut writer).unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_metrics_count,
    bench_cardinality,
    bench_total_time_series,
    bench_configurations,
    bench_realistic_scenarios,
    bench_memory_patterns
);
criterion_main!(benches);
