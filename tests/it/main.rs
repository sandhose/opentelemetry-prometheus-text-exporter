use opentelemetry::KeyValue;
use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;

#[test]
fn test_serialize() {
    let exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::new();
    let provider = SdkMeterProvider::builder()
        .with_resource(
            Resource::builder_empty()
                .with_attribute(KeyValue::new("foo", "bar"))
                .build(),
        )
        .with_reader(exporter.clone())
        .build();
    let meter = provider.meter("test");
    let counter = meter
        .i64_up_down_counter("http.server.active_requests")
        .with_description("Number of active HTTP server requests.")
        .with_unit("{request}")
        .build();
    counter.add(1, &[KeyValue::new("method", "GET")]);
    counter.add(1, &[KeyValue::new("method", "POST")]);

    let gauge = meter
        .f64_gauge("system.uptime")
        .with_description("The time the system has been running")
        .with_unit("s")
        .build();
    gauge.record(23.4, &[]);

    let histogram = meter
        .f64_histogram("http.server.request.duration")
        .with_description("Duration of HTTP server requests.")
        .with_unit("ms")
        .build();
    histogram.record(23.5, &[KeyValue::new("method", "GET")]);
    histogram.record(1.3, &[KeyValue::new("method", "POST")]);

    let mut buffer = Vec::new();
    exporter.export(&mut buffer).unwrap();

    let buffer = String::from_utf8(buffer).unwrap();

    // Sort the lines in each block
    // There are a few things that don't have a stable ordering:
    //  - the order in which each set of label is rendered
    //  - the order of labels in each metric (including the target_info)
    //
    // To compensate for this, this test:
    //  - has only one resource set
    //  - has one label set per metric
    //  - sort the lines in each metric block
    let buffer = buffer
        .split("\n\n")
        .map(|block| {
            let mut lines: Vec<&str> = block.lines().collect();
            lines.sort();
            lines.join("\n")
        })
        .collect::<Vec<String>>()
        .join("\n\n");

    insta::assert_snapshot!(buffer, @r##"
    # HELP http_server_active_requests Number of active HTTP server requests.
    # TYPE http_server_active_requests gauge
    http_server_active_requests{method="GET",otel_scope_name="test"} 1
    http_server_active_requests{method="POST",otel_scope_name="test"} 1

    # HELP system_uptime_seconds The time the system has been running
    # TYPE system_uptime_seconds gauge
    # UNIT system_uptime_seconds seconds
    system_uptime_seconds{otel_scope_name="test"} 23.4

    # HELP http_server_request_duration_milliseconds Duration of HTTP server requests.
    # TYPE http_server_request_duration_milliseconds histogram
    # UNIT http_server_request_duration_milliseconds milliseconds
    http_server_request_duration_milliseconds_bucket{method="GET",le="+Inf",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="0",otel_scope_name="test"} 0
    http_server_request_duration_milliseconds_bucket{method="GET",le="10",otel_scope_name="test"} 0
    http_server_request_duration_milliseconds_bucket{method="GET",le="100",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="1000",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="10000",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="25",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="250",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="2500",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="5",otel_scope_name="test"} 0
    http_server_request_duration_milliseconds_bucket{method="GET",le="50",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="500",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="5000",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="75",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="750",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="GET",le="7500",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="+Inf",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="0",otel_scope_name="test"} 0
    http_server_request_duration_milliseconds_bucket{method="POST",le="10",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="100",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="1000",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="10000",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="25",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="250",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="2500",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="5",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="50",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="500",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="5000",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="75",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="750",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_bucket{method="POST",le="7500",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_count{method="GET",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_count{method="POST",otel_scope_name="test"} 1
    http_server_request_duration_milliseconds_sum{method="GET",otel_scope_name="test"} 23.5
    http_server_request_duration_milliseconds_sum{method="POST",otel_scope_name="test"} 1.3

    # HELP target_info Target metadata
    # TYPE target_info gauge
    target_info{foo="bar"} 1
    "##);
}

#[test]
fn test_without_units() {
    let exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::builder()
        .without_units()
        .build();

    let provider = SdkMeterProvider::builder()
        .with_resource(Resource::builder_empty().build())
        .with_reader(exporter.clone())
        .build();

    let meter = provider.meter("test");
    let histogram = meter
        .f64_histogram("http.server.request.duration")
        .with_description("Duration of HTTP server requests.")
        .with_unit("ms")
        .build();
    histogram.record(23.5, &[]);

    let mut buffer = Vec::new();
    exporter.export(&mut buffer).unwrap();
    let output = String::from_utf8(buffer).unwrap();

    // Should not contain "milliseconds" in metric name
    assert!(!output.contains("http_server_request_duration_milliseconds"));
    // Should contain the base name without unit suffix
    assert!(output.contains("http_server_request_duration_bucket"));
}

#[test]
fn test_without_counter_suffixes() {
    let exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::builder()
        .without_counter_suffixes()
        .build();

    let provider = SdkMeterProvider::builder()
        .with_resource(Resource::builder_empty().build())
        .with_reader(exporter.clone())
        .build();

    let meter = provider.meter("test");
    let counter = meter
        .u64_counter("http.server.requests")
        .with_description("Number of HTTP server requests")
        .build();
    counter.add(1, &[]);

    let mut buffer = Vec::new();
    exporter.export(&mut buffer).unwrap();
    let output = String::from_utf8(buffer).unwrap();

    // Should not contain "_total" suffix
    assert!(!output.contains("http_server_requests_total"));
    // Should contain the base name without _total suffix
    assert!(output.contains("http_server_requests{"));
}

#[test]
fn test_without_target_info() {
    let exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::builder()
        .without_target_info()
        .build();

    let provider = SdkMeterProvider::builder()
        .with_resource(
            Resource::builder_empty()
                .with_attribute(KeyValue::new("service.name", "test-service"))
                .build(),
        )
        .with_reader(exporter.clone())
        .build();

    let meter = provider.meter("test");
    let counter = meter.u64_counter("test.counter").build();
    counter.add(1, &[]);

    let mut buffer = Vec::new();
    exporter.export(&mut buffer).unwrap();
    let output = String::from_utf8(buffer).unwrap();

    // Should not contain target_info metric
    assert!(!output.contains("target_info"));
    assert!(!output.contains("service.name"));
}

#[test]
fn test_without_scope_info() {
    let exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::builder()
        .without_scope_info()
        .build();

    let provider = SdkMeterProvider::builder()
        .with_resource(Resource::builder_empty().build())
        .with_reader(exporter.clone())
        .build();

    let meter = provider.meter("test-meter");
    let counter = meter.u64_counter("test.counter").build();
    counter.add(1, &[]);

    let mut buffer = Vec::new();
    exporter.export(&mut buffer).unwrap();
    let output = String::from_utf8(buffer).unwrap();

    // Should not contain otel_scope_name labels
    assert!(!output.contains("otel_scope_name"));
    assert!(!output.contains("test-meter"));
}

#[test]
fn test_combined_configuration() {
    let exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::builder()
        .without_units()
        .without_counter_suffixes()
        .without_target_info()
        .without_scope_info()
        .build();

    let provider = SdkMeterProvider::builder()
        .with_resource(
            Resource::builder_empty()
                .with_attribute(KeyValue::new("service.name", "test-service"))
                .build(),
        )
        .with_reader(exporter.clone())
        .build();

    let meter = provider.meter("test-meter");

    let counter = meter
        .u64_counter("http.server.requests")
        .with_description("Number of HTTP server requests")
        .with_unit("{request}")
        .build();
    counter.add(1, &[]);

    let histogram = meter
        .f64_histogram("http.server.duration")
        .with_description("Duration of HTTP server requests")
        .with_unit("ms")
        .build();
    histogram.record(23.5, &[]);

    let mut buffer = Vec::new();
    exporter.export(&mut buffer).unwrap();
    let output = String::from_utf8(buffer).unwrap();

    // Should not contain any of the disabled features
    assert!(!output.contains("_total"));
    assert!(!output.contains("milliseconds"));
    assert!(!output.contains("target_info"));
    assert!(!output.contains("otel_scope_name"));
    assert!(!output.contains("service.name"));
    assert!(!output.contains("test-meter"));

    // Should contain the basic metric names
    assert!(output.contains("http_server_requests"));
    assert!(output.contains("http_server_duration_bucket"));
}

#[test]
fn test_builder_pattern_comprehensive_example() {
    // Test 1: Default configuration (all features enabled)
    let default_exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::new();

    // Test 2: Using builder pattern with all options disabled
    let custom_exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::builder()
        .without_units()
        .without_counter_suffixes()
        .without_target_info()
        .without_scope_info()
        .build();

    // Test 3: Selective configuration
    let selective_exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::builder()
        .without_units()
        .without_target_info()
        .build();

    // Create test data for each exporter
    let test_cases = vec![
        ("default", default_exporter),
        ("all_disabled", custom_exporter),
        ("selective", selective_exporter),
    ];

    for (name, exporter) in test_cases {
        println!("\n=== Testing {name} configuration ===");

        let provider = SdkMeterProvider::builder()
            .with_resource(
                Resource::builder_empty()
                    .with_attribute(KeyValue::new("service.name", "demo-service"))
                    .with_attribute(KeyValue::new("service.version", "1.0.0"))
                    .build(),
            )
            .with_reader(exporter.clone())
            .build();

        let meter = provider.meter("demo-meter");

        // Create various metric types
        let counter = meter
            .u64_counter("http.requests")
            .with_description("Total HTTP requests")
            .with_unit("{request}")
            .build();
        counter.add(42, &[KeyValue::new("method", "GET")]);

        let histogram = meter
            .f64_histogram("request.duration")
            .with_description("Request duration")
            .with_unit("ms")
            .build();
        histogram.record(123.45, &[KeyValue::new("method", "POST")]);

        let gauge = meter
            .f64_gauge("cpu.utilization")
            .with_description("CPU utilization")
            .with_unit("1")
            .build();
        gauge.record(0.75, &[]);

        let mut buffer = Vec::new();
        exporter.export(&mut buffer).unwrap();
        let output = String::from_utf8(buffer).unwrap();

        // Verify expected behavior based on configuration
        match name {
            "default" => {
                // Default should have all features
                assert!(output.contains("http_requests_total"));
                assert!(output.contains("request_duration_milliseconds"));
                assert!(output.contains("cpu_utilization_ratio"));
                assert!(output.contains("target_info"));
                assert!(output.contains("otel_scope_name=\"demo-meter\""));
                // Check for target_info with service attributes (format might vary)
                assert!(output.contains("target_info{"));
                assert!(output.contains("service"));
            }
            "all_disabled" => {
                // All features disabled
                assert!(output.contains("http_requests"));
                assert!(!output.contains("http_requests_total"));
                assert!(output.contains("request_duration"));
                assert!(!output.contains("request_duration_milliseconds"));
                assert!(output.contains("cpu_utilization"));
                assert!(!output.contains("cpu_utilization_ratio"));
                assert!(!output.contains("target_info"));
                assert!(!output.contains("otel_scope_name"));
                assert!(!output.contains("service"));
            }
            "selective" => {
                // Only units and target_info disabled
                assert!(output.contains("http_requests_total"));
                assert!(output.contains("request_duration"));
                assert!(!output.contains("request_duration_milliseconds"));
                assert!(output.contains("cpu_utilization"));
                assert!(!output.contains("cpu_utilization_ratio"));
                assert!(!output.contains("target_info"));
                assert!(output.contains("otel_scope_name=\"demo-meter\""));
                assert!(!output.contains("service"));
            }
            _ => unreachable!(),
        }
    }
}
