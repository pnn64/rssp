use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;

const MONO_THRESHOLD: usize = 6;
const REPEATS: usize = 25_000;
const BASE_PATTERN: &str = "1000\n0100\n0010\n0001\n";

fn generate_bitmasks(minimized_chart: &[u8]) -> Vec<u8> {
    minimized_chart
        .split(|&b| b == b'\n')
        .filter_map(|line| {
            if line.len() < 4 || line.iter().all(|&b| b == b' ' || b == b',') {
                return None;
            }

            let mut mask = 0u8;
            for i in 0..4 {
                if matches!(line[i], b'1' | b'2' | b'4') {
                    mask |= 1 << i;
                }
            }
            Some(mask)
        })
        .collect()
}

fn build_bitmasks() -> Vec<u8> {
    let mut chart = Vec::with_capacity(BASE_PATTERN.len() * REPEATS);
    for _ in 0..REPEATS {
        chart.extend_from_slice(BASE_PATTERN.as_bytes());
    }
    generate_bitmasks(&chart)
}

fn bench_mono_counts(c: &mut Criterion) {
    let bitmasks = build_bitmasks();
    let mut group = c.benchmark_group("mono");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("count_facing_steps", |b| {
        b.iter(|| {
            let counts = rssp::patterns::count_facing_steps(
                black_box(&bitmasks),
                black_box(MONO_THRESHOLD),
            );
            black_box(counts);
        })
    });
    group.finish();
}

criterion_group!(benches, bench_mono_counts);
criterion_main!(benches);
