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

    let buffer = serialize_bench::BufferFixture::new();
    serialize_bench::assert_buffer_behavior(&buffer);
    let mut group = c.benchmark_group("serialize_stack_buffer");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(buffer.output_len as u64));
    group.bench_function("unbuffered", |b| {
        let mut output = Vec::with_capacity(buffer.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_buffered(
                black_box(&buffer.summary),
                black_box(&mut output),
                true,
            ));
            black_box(&output);
        });
    });
    group.bench_function("stack_buffered", |b| {
        let mut output = Vec::with_capacity(buffer.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_buffered(
                black_box(&buffer.summary),
                black_box(&mut output),
                false,
            ));
            black_box(&output);
        });
    });
    group.finish();

    let escape = serialize_bench::EscapeFixture::new();
    serialize_bench::assert_escape_behavior(&escape);
    let mut group = c.benchmark_group("serialize_escape_metadata");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(escape.output_len as u64));
    group.bench_function("byte_at_a_time", |b| {
        let mut output = Vec::with_capacity(escape.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_escape(
                black_box(&escape.summary),
                black_box(&mut output),
                true,
            ));
            black_box(&output);
        });
    });
    group.bench_function("batched_spans", |b| {
        let mut output = Vec::with_capacity(escape.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_escape(
                black_box(&escape.summary),
                black_box(&mut output),
                false,
            ));
            black_box(&output);
        });
    });
    group.finish();

    let field = escape.summary.title_str.as_bytes();
    let mut group = c.benchmark_group("sm_escape_metadata");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(field.len() as u64));
    group.bench_function("byte_at_a_time", |b| {
        let mut output = Vec::with_capacity(escape.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_escape_field(
                black_box(field),
                black_box(&mut output),
                true,
            ));
            black_box(&output);
        });
    });
    group.bench_function("batched_spans", |b| {
        let mut output = Vec::with_capacity(escape.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_escape_field(
                black_box(field),
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
