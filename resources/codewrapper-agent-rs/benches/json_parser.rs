//! JSON parser benchmarks

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use tokio::io::{AsyncBufReadExt, BufReader};

// Note: This benchmark file is a placeholder.
// The actual JsonStreamParser is in src/parser.rs

fn json_parsing_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Create sample JSON lines
    let sample_lines: String = (0..1000)
        .map(|i| {
            format!(
                r#"{{"type": "event", "id": {}, "content": "Sample content {}"}}"#,
                i, i
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut group = c.benchmark_group("json_parser");
    group.throughput(Throughput::Elements(1000));

    group.bench_function("parse_1000_events", |b| {
        b.iter(|| {
            rt.block_on(async {
                let reader = BufReader::new(sample_lines.as_bytes());
                let mut count = 0;
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _: serde_json::Value = serde_json::from_str(&line).unwrap();
                    count += 1;
                }
                black_box(count)
            })
        });
    });

    group.finish();
}

fn throughput_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Create larger sample for throughput testing
    let sample_lines: String = (0..10000)
        .map(|i| format!(r#"{{"type": "message", "index": {}, "data": "Lorem ipsum dolor sit amet, consectetur adipiscing elit."}}"#, i))
        .collect::<Vec<_>>()
        .join("\n");

    let bytes = sample_lines.len();

    let mut group = c.benchmark_group("throughput");
    group.throughput(Throughput::Bytes(bytes as u64));

    group.bench_function("parse_10k_events", |b| {
        b.iter(|| {
            rt.block_on(async {
                let reader = BufReader::new(sample_lines.as_bytes());
                let mut count = 0;
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(_v) = serde_json::from_str::<serde_json::Value>(&line) {
                        count += 1;
                    }
                }
                black_box(count)
            })
        });
    });

    group.finish();
}

criterion_group!(benches, json_parsing_benchmark, throughput_benchmark);
criterion_main!(benches);
