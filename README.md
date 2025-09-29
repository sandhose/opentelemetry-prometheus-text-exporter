# OpenTelemetry Prometheus Text Exporter

A high-performance OpenTelemetry metrics exporter that converts metrics to Prometheus text exposition format.

[![Crates.io](https://img.shields.io/crates/v/opentelemetry-prometheus-text-exporter.svg)](https://crates.io/crates/opentelemetry-prometheus-text-exporter)
[![Documentation](https://img.shields.io/docsrs/opentelemetry-prometheus-text-exporter.svg)](https://docs.rs/opentelemetry-prometheus-text-exporter)
[![Apache 2.0 License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

## Features

- **High Performance**: ~2.5-3x faster than existing implementations with ~2.0-2.5 million time series per second throughput
- **Memory Optimized**: Minimal allocations and memory footprint
- **Prometheus Compliant**: Generates valid Prometheus text exposition format
- **Configurable**: Flexible options for metric naming, units, and metadata
- **Minimal Dependencies**: Small dependency footprint with only essential crates (smartstring for optimization)

## Supported Metric Types

- ✅ **Gauges**: All numeric types → Prometheus Gauge
- ✅ **Sums**: Cumulative+Monotonic → Counter, Cumulative+Non-monotonic → Gauge
- ✅ **Histograms**: All numeric types → Prometheus Histogram family
- ❌ **Exponential Histograms**: Currently unsupported (not supported by the Prometheus text exposition format)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
opentelemetry-prometheus-text-exporter = "0.3.0"
opentelemetry = "0.31"
opentelemetry_sdk = "0.31"
```

## Quick Start

```rust
use opentelemetry::{KeyValue, metrics::MeterProvider};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_prometheus_text_exporter::PrometheusExporter;

// Create exporter with default configuration
let exporter = PrometheusExporter::new();

// Set up metrics provider
let provider = SdkMeterProvider::builder()
    .with_resource(
        opentelemetry_sdk::Resource::builder_empty()
            .with_attribute(KeyValue::new("service.name", "my-service"))
            .with_attribute(KeyValue::new("service.version", "1.0.0"))
            .build()
    )
    .with_reader(exporter.clone())
    .build();

// Create and use meters
let meter = provider.meter("my-meter");
let counter = meter.u64_counter("requests_total").build();
counter.add(1, &[KeyValue::new("method", "GET")]);

// Export metrics to Prometheus format
let mut output = Vec::new();
exporter.export(&mut output).unwrap();
println!("{}", String::from_utf8(output).unwrap());
```

## Configuration Options

The exporter supports extensive configuration through the builder pattern:

```rust
use opentelemetry_prometheus_text_exporter::PrometheusExporter;

let exporter = PrometheusExporter::builder()
    .without_units()              // Disable unit suffixes in metric names
    .without_counter_suffixes()   // Disable _total suffixes on counters
    .without_target_info()        // Disable target_info metric from resources
    .without_scope_info()         // Disable otel_scope_info metrics
    .build();
```

### Configuration Details

| Option | Description | Default |
|--------|-------------|---------|
| `without_units()` | Disables automatic unit suffixes (e.g., `_seconds`, `_bytes`) | Units enabled |
| `without_counter_suffixes()` | Disables `_total` suffix on counter metrics | Suffixes enabled |
| `without_target_info()` | Disables `target_info` metric generation from resource attributes | target_info enabled |
| `without_scope_info()` | Disables `otel_scope_info` metric with instrumentation scope labels | scope_info enabled |

## Output Format

The exporter generates standard Prometheus text exposition format:

```text
# HELP target_info Target metadata
# TYPE target_info gauge
target_info{service_name="my-service",service_version="1.0.0"} 1
# HELP requests_total Total number of requests
# TYPE requests_total counter
requests_total{method="GET"} 1
```

## Performance

This implementation is optimized for high-throughput scenarios:

- **Throughput**: ~2.0-2.5 million time series per second
- **Memory**: Minimal allocations and memory footprint
- **Scaling**: Linear performance with metric count and cardinality
- **Overhead**: Minimal configuration impact (< 5% difference between options)

### Benchmarks

Run benchmarks to measure performance on your system:

```bash
# Quick benchmark
cargo bench --bench serialize -- --quick

# Full benchmark suite
cargo bench --bench serialize

# Compare against opentelemetry-prometheus
cargo bench --bench comparison
```

## Why This Crate?

This crate was created to provide:

1. **Better Performance**: Significantly faster than existing OpenTelemetry Prometheus exporters
2. **Text Format Focus**: Specifically designed for Prometheus text exposition format
3. **Memory Efficiency**: Optimized memory usage patterns
4. **API Compatibility**: Drop-in replacement for existing exporters

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Links

- [Repository](https://github.com/sandhose/opentelemetry-prometheus-text-exporter)
- [Documentation](https://docs.rs/opentelemetry-prometheus-text-exporter)
- [Crates.io](https://crates.io/crates/opentelemetry-prometheus-text-exporter)
- [OpenTelemetry](https://opentelemetry.io/)
- [Prometheus](https://prometheus.io/)
