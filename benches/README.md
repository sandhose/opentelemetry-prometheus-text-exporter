# OpenTelemetry Prometheus Exporter Benchmarks

This directory contains comprehensive benchmarks for testing the performance characteristics of the OpenTelemetry Prometheus exporter, focusing on how serialization performance scales with different workload patterns.

## Overview

The benchmarks use a **fake writer** (like `/dev/null`) to ensure we're only measuring serialization performance, not I/O performance. All data written to the fake writer is passed through `std::hint::black_box` to prevent compiler optimizations from eliminating the actual serialization work.

## Running Benchmarks

### Quick Run (Fast Feedback)
```bash
cargo bench --bench serialize -- --quick
```

### Full Benchmark Suite
```bash
cargo bench --bench serialize
```

### Run Specific Benchmark Groups
```bash
# Test scaling with number of metrics
cargo bench --bench serialize -- metrics_count

# Test scaling with cardinality
cargo bench --bench serialize -- cardinality

# Test scaling with total time series (metrics × cardinality)
cargo bench --bench serialize -- total_time_series

# Test different configurations
cargo bench --bench serialize -- configurations

# Test realistic scenarios
cargo bench --bench serialize -- realistic_scenarios

# Test memory optimization patterns
cargo bench --bench serialize -- memory_patterns
```

### Generate Reports
```bash
# Generate HTML report (requires gnuplot or uses plotters backend)
cargo bench --bench serialize
# Results saved to target/criterion/
```

## Benchmark Groups

### 1. `metrics_count` - Scaling with Number of Metrics
**Purpose**: Tests how performance scales as the number of different metrics increases.

- **Fixed**: 10 label combinations per metric (cardinality = 10)
- **Variable**: Number of metrics (1, 5, 10, 25, 50, 100)
- **Metric Type**: Counter metrics with units
- **Throughput**: Measured in time series per second

**Expected Behavior**: Should scale roughly linearly with the number of metrics.

### 2. `cardinality` - Scaling with Cardinality
**Purpose**: Tests how performance scales as the number of unique label combinations per metric increases.

- **Fixed**: 5 metrics
- **Variable**: Cardinality per metric (1, 5, 10, 25, 50, 100, 250)
- **Metric Type**: Counter metrics with complex label combinations
- **Throughput**: Measured in time series per second

**Expected Behavior**: Should scale roughly linearly with cardinality, but may have some overhead per unique label combination.

### 3. `total_time_series` - Scaling with Total Time Series
**Purpose**: Tests how performance scales with the total number of time series (metrics × cardinality).

- **Variable**: Different combinations of metrics and cardinality resulting in 100-10,000 time series
- **Metric Type**: Counter metrics
- **Throughput**: Measured in time series per second

**Expected Behavior**: Should show consistent throughput regardless of how the time series are distributed across metrics vs cardinality.

### 4. `configurations` - Configuration Impact
**Purpose**: Tests the performance impact of different exporter configuration options.

**Configurations Tested**:
- `default`: All features enabled (units, counter suffixes, target_info, scope_info)
- `without_units`: Disables unit suffixes in metric names
- `without_counter_suffixes`: Disables `_total` suffixes on counters
- `without_target_info`: Disables resource `target_info` metric
- `without_scope_info`: Disables OpenTelemetry scope labels
- `minimal`: All optimizations enabled (fastest configuration)

**Expected Behavior**: `minimal` should be fastest, others should show marginal differences.

### 5. `realistic_scenarios` - Real-World Workloads
**Purpose**: Tests performance with realistic service monitoring scenarios.

**Scenarios**:
- **Small Service**: 10 metrics, 5 label combinations each (~50 time series)
- **Medium Service**: 50 metrics, 20 label combinations each (~1,000 time series)
- **Large Service**: 100 metrics, 50 label combinations each (~5,000 time series)
- **High Cardinality**: 20 metrics, 200 label combinations each (~4,000 time series)
- **Throughput**: Measured in time series per second

**Expected Behavior**: Should demonstrate real-world performance characteristics.

### 6. `memory_patterns` - Memory Optimization
**Purpose**: Tests the effectiveness of `Cow<str>` memory optimizations.

**Test Cases**:
- **Clean Names**: Metric names that don't require sanitization (should use `Cow::Borrowed`)
- **Dirty Names**: Metric names with invalid characters requiring sanitization (should use `Cow::Owned`)

**Expected Behavior**: Clean names should be slightly faster due to avoiding allocations.

## Interpreting Results

### Key Metrics
- **Time**: Wall-clock time per iteration
- **Throughput**: Time series processed per second (shown as `thrpt`)
- **Scaling**: How performance changes with input parameters
- **Configuration Impact**: Performance differences between configurations

### Example Output
```
metrics_count/100       time:   [634.66 µs 644.55 µs 647.03 µs]
                        thrpt:  [1.5455 Melem/s 1.5515 Melem/s 1.5756 Melem/s]
```
This shows that serializing 100 counter metrics (with 10 cardinality each = 1000 time series) takes approximately 645 µs and achieves a throughput of ~1.55 million time series per second.

### Performance Expectations
Based on the benchmark design:
- **Linear Scaling**: Both metrics count and cardinality should scale roughly linearly
- **Consistent Throughput**: Should maintain ~1.5-1.8 million time series per second across different scales
- **Configuration Overhead**: Minimal differences between configurations (< 5%)
- **Memory Optimizations**: Clean names should be marginally faster than dirty names
- **Realistic Workloads**:
  - Small services: < 100 µs (50 time series)
  - Medium services: < 1 ms (1,000 time series)
  - Large services: < 10 ms (5,000+ time series)

## Technical Details

### Metric Types
Currently focuses on **counter metrics** as they are:
- Most common in real applications
- Have consistent serialization patterns
- Include all complexity (name sanitization, unit conversion, suffixes)

### Label Patterns
The benchmarks use realistic label patterns:
- **HTTP-style labels**: `method`, `status`, `endpoint`, `region`
- **Realistic values**: Various HTTP methods, status codes, API endpoints
- **Cardinality distribution**: Varied to simulate real-world scenarios

### Test Data Generation
- Metrics are pre-created with realistic names and units
- Label combinations are generated to create specified cardinality
- Data is recorded once during setup, not during measurement
- Provider lifetime is properly managed to ensure metrics remain available
- Throughput is measured in time series per second for easy comparison

### Benchmark Visualization
Criterion automatically generates performance plots showing:
- **Line charts**: Performance vs input parameter (number of metrics, cardinality, etc.)
- **Throughput charts**: Time series per second vs input parameter
- **Regression analysis**: Statistical confidence in scaling behavior
- **Comparison charts**: Before/after performance when re-running benchmarks

## Extending Benchmarks

### Adding New Benchmark Groups
1. Create a new function following the pattern: `fn bench_new_group(c: &mut Criterion)`
2. Add it to `criterion_group!` macro at the bottom
3. Use `setup_metrics_with_counters()` for consistent test data

### Adding Different Metric Types
Currently focused on counters. To add histograms or gauges:
1. Create new setup functions (e.g., `setup_metrics_with_histograms`)
2. Follow the same pattern but create different metric types
3. Ensure proper lifetime management with `Arc<SdkMeterProvider>`

### Custom Scenarios
To test specific scenarios:
1. Use the existing patterns as templates
2. Adjust metric counts, cardinality, and label patterns
3. Document expected behavior and performance characteristics

## Performance Goals

The benchmarks help ensure:
- **Scalability**: Performance scales predictably with workload
- **Efficiency**: Minimal overhead from configuration options
- **Memory Usage**: Effective use of `Cow<str>` optimizations
- **Real-World Performance**: Acceptable performance for production workloads

Results from these benchmarks inform optimization decisions and help identify performance regressions during development.
