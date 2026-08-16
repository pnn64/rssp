use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

#[path = "support/sm_timing.rs"]
mod sm_timing_bench;

fn bench_sm_timing(c: &mut Criterion) {
    sm_timing_bench::assert_behavior();
    let fixture = sm_timing_bench::SmTimingFixture::new();
    let mut group = c.benchmark_group("sm_timing_4096_bpms_2048_stops");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(sm_timing_bench::INPUT_COUNT));
    group.bench_function("legacy_f32_then_f64", |b| {
        b.iter(|| {
            black_box(rssp::timing::process_sm_timing_for_bench(
                black_box(&fixture.bpms),
                black_box(&fixture.stops),
                true,
            ));
        });
    });
    group.bench_function("direct_f64", |b| {
        b.iter(|| {
            black_box(rssp::timing::process_sm_timing_for_bench(
                black_box(&fixture.bpms),
                black_box(&fixture.stops),
                false,
            ));
        });
    });
    group.finish();
}

criterion_group!(benches, bench_sm_timing);
criterion_main!(benches);
