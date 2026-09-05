use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

#[path = "support/nps_cases.rs"]
mod nps_cases;

fn bench_nps(c: &mut Criterion) {
    let cases = nps_cases::cases();
    let mut group = c.benchmark_group("nps");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for (name, values) in &cases {
        group.throughput(Throughput::Elements(values.len() as u64));
        group.bench_with_input(BenchmarkId::new("cold", name), values, |b, values| {
            b.iter(|| black_box(rssp::nps::get_nps_stats(black_box(values))));
        });
        let mut scratch = Vec::new();
        black_box(rssp::nps::get_nps_stats_with_scratch(values, &mut scratch));
        group.bench_with_input(BenchmarkId::new("reused", name), values, |b, values| {
            b.iter(|| {
                black_box(rssp::nps::get_nps_stats_with_scratch(
                    black_box(values),
                    black_box(&mut scratch),
                ))
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_nps);
criterion_main!(benches);
