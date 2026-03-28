//! Startup time benchmarks

use criterion::{Criterion, criterion_group, criterion_main};
use std::process::Command;

fn startup_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("startup");

    // Benchmark --help startup time
    group.bench_function("help_startup", |b| {
        b.iter(|| {
            let output = Command::new("cargo")
                .args(["run", "--release", "--", "--help"])
                .output()
                .expect("Failed to execute command");
            assert!(output.status.success());
        });
    });

    // Benchmark --version startup time
    group.bench_function("version_startup", |b| {
        b.iter(|| {
            let output = Command::new("cargo")
                .args(["run", "--release", "--", "--version"])
                .output()
                .expect("Failed to execute command");
            assert!(output.status.success());
        });
    });

    group.finish();
}

criterion_group!(benches, startup_benchmark);
criterion_main!(benches);
