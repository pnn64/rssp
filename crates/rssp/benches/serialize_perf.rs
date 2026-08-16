use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

#[allow(dead_code)]
#[path = "support/report_timing.rs"]
mod report_timing_bench;
#[path = "support/serialize.rs"]
mod serialize_bench;

fn bench_serialize(c: &mut Criterion) {
    let fixture = serialize_bench::SerializeFixture::new();
    serialize_bench::assert_behavior(&fixture);
    let mut group = c.benchmark_group("serialize_ssc_3584_timing_segments");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(fixture.output_len as u64));
    group.bench_function("temporary_strings", |b| {
        let mut output = Vec::with_capacity(fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write(
                black_box(&fixture.summary),
                black_box(&mut output),
                true,
            ));
            black_box(&output);
        });
    });
    group.bench_function("direct_writer", |b| {
        let mut output = Vec::with_capacity(fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write(
                black_box(&fixture.summary),
                black_box(&mut output),
                false,
            ));
            black_box(&output);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_serialize);
criterion_main!(benches);
