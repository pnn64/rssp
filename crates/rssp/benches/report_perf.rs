use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/camellia_mix.ssc");

#[path = "support/report_timing.rs"]
mod report_timing_bench;

fn fast_options() -> rssp::AnalysisOptions {
    rssp::AnalysisOptions {
        mono_threshold: 6,
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..rssp::AnalysisOptions::default()
    }
}

fn large_course_summary(entry_count: usize) -> rssp::CourseSummary {
    let mut simfile = rssp::analyze(FIXTURE.as_bytes(), "ssc", &rssp::AnalysisOptions::default())
        .expect("fixture should analyze");
    let chart = simfile
        .charts
        .pop()
        .expect("fixture should contain a chart");

    let entries = (0..entry_count)
        .map(|index| rssp::CourseEntrySummary {
            song: format!("Song {index} \"Special\""),
            song_dir: format!("Group/Song {index}"),
            step_type: "dance-single".to_string(),
            difficulty: "Challenge".to_string(),
            rating: (10 + index % 20).to_string(),
            sha1: format!("{index:016x}"),
            bpm_neutral_sha1: format!("{:016x}", index.wrapping_mul(31)),
        })
        .collect();
    let sha1_hashes = (0..entry_count)
        .map(|index| format!("{index:016x}"))
        .collect();
    let bpm_neutral_sha1_hashes = (0..entry_count)
        .map(|index| format!("{:016x}", index.wrapping_mul(31)))
        .collect();

    rssp::CourseSummary {
        course: "Performance \"Course\"".to_string(),
        course_difficulty: "Challenge".to_string(),
        step_type: "dance-single".to_string(),
        total_length: 7_200,
        entries,
        chart,
        sha1_hashes,
        bpm_neutral_sha1_hashes,
        pattern_counts_enabled: true,
        tech_counts_enabled: true,
        total_elapsed: Duration::ZERO,
    }
}

fn bench_timing_snapshot(c: &mut Criterion) {
    let fixture = report_timing_bench::fixture();
    let summary = rssp::analyze(fixture.as_bytes(), "ssc", &report_timing_bench::options())
        .expect("fixture should analyze");
    let chart = summary
        .charts
        .first()
        .expect("fixture should contain a chart");

    let mut group = c.benchmark_group("report_timing");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(
        report_timing_bench::SEGMENT_COUNT as u64,
    ));
    group.bench_function("build_snapshot_many_segments", |b| {
        b.iter(|| {
            black_box(rssp::report::build_timing_snapshot(
                black_box(chart),
                black_box(&summary),
            ));
        });
    });
    group.finish();
}

fn bench_timing_json(c: &mut Criterion) {
    let fixture = report_timing_bench::fixture();
    let summary = rssp::analyze(fixture.as_bytes(), "ssc", &report_timing_bench::options())
        .expect("fixture should analyze");

    let mut group = c.benchmark_group("report_json_timing");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(
        report_timing_bench::SEGMENT_COUNT as u64,
    ));
    group.bench_function("write_512_segments", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("timing JSON report should write");
            black_box(output);
        });
    });
    group.finish();
}

fn bench_csv(c: &mut Criterion) {
    let summary =
        rssp::analyze(FIXTURE.as_bytes(), "ssc", &fast_options()).expect("fixture should analyze");

    let mut group = c.benchmark_group("report_csv");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("write_fixture", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&summary),
                rssp::report::OutputMode::CSV,
                black_box(&mut output),
            )
            .expect("CSV report should write");
            black_box(output);
        });
    });
    group.finish();
}

fn bench_json(c: &mut Criterion) {
    let fast_summary =
        rssp::analyze(FIXTURE.as_bytes(), "ssc", &fast_options()).expect("fixture should analyze");
    let full_summary = rssp::analyze(FIXTURE.as_bytes(), "ssc", &rssp::AnalysisOptions::default())
        .expect("fixture should analyze");

    let mut group = c.benchmark_group("report_json");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("write_fixture", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&fast_summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("JSON report should write");
            black_box(output);
        });
    });
    group.bench_function("write_fixture_full", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&full_summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("JSON report should write");
            black_box(output);
        });
    });
    group.finish();
}

fn bench_course_json(c: &mut Criterion) {
    const ENTRIES: usize = 1_024;
    let summary = large_course_summary(ENTRIES);

    let mut group = c.benchmark_group("report_course_json");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(ENTRIES as u64));
    group.bench_function("write_large_course", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_course_reports(
                black_box(&summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("course JSON report should write");
            black_box(output);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_timing_snapshot,
    bench_timing_json,
    bench_csv,
    bench_json,
    bench_course_json
);
criterion_main!(benches);
