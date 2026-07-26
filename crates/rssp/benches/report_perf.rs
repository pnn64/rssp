use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/camellia_mix.ssc");

fn fast_options() -> rssp::AnalysisOptions {
    rssp::AnalysisOptions {
        mono_threshold: 6,
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..rssp::AnalysisOptions::default()
    }
}

fn many_timing_segments_fixture(segment_count: usize) -> String {
    fn push_pairs(out: &mut String, count: usize, mut value: impl FnMut(usize) -> f64) {
        for idx in 0..count {
            if idx != 0 {
                out.push(',');
            }
            write!(out, "{}={}", idx * 4, value(idx)).unwrap();
        }
        out.push_str(";\n");
    }

    let mut fixture = String::with_capacity(segment_count * 180);
    fixture.push_str("#VERSION:0.83;\n#OFFSET:-0.125;\n#BPMS:");
    push_pairs(&mut fixture, segment_count, |idx| 90.0 + (idx % 211) as f64);
    fixture.push_str("#STOPS:");
    push_pairs(&mut fixture, segment_count, |idx| {
        0.01 + (idx % 17) as f64 / 100.0
    });
    fixture.push_str("#DELAYS:");
    push_pairs(&mut fixture, segment_count, |idx| {
        0.02 + (idx % 13) as f64 / 100.0
    });
    fixture.push_str("#WARPS:");
    push_pairs(&mut fixture, segment_count, |idx| 0.5 + (idx % 7) as f64);

    fixture.push_str("#SPEEDS:");
    for idx in 0..segment_count {
        if idx != 0 {
            fixture.push(',');
        }
        write!(
            &mut fixture,
            "{}={}=0.25={}",
            idx * 4,
            1.25 + (idx % 9) as f64 / 10.0,
            idx & 1
        )
        .unwrap();
    }
    fixture.push_str(";\n#SCROLLS:");
    push_pairs(&mut fixture, segment_count, |idx| {
        0.75 + (idx % 11) as f64 / 10.0
    });
    fixture.push_str("#FAKES:");
    push_pairs(&mut fixture, segment_count, |idx| 0.25 + (idx % 5) as f64);

    fixture.push_str(concat!(
        "#TIMESIGNATURES:0=4=4,64=3=4,128=7=8;\n",
        "#LABELS:0=Song Start,64=Middle,128=Finale;\n",
        "#TICKCOUNTS:0=4,64=8,128=12;\n",
        "#COMBOS:0=1=1,64=2=3,128=4=5;\n",
        "#NOTEDATA:;\n",
        "#STEPSTYPE:dance-single;\n",
        "#DESCRIPTION:report benchmark;\n",
        "#DIFFICULTY:Challenge;\n",
        "#METER:10;\n",
        "#CREDIT:;\n",
        "#NOTES:\n",
        "1000\n0100\n0010\n0001\n",
        ";\n"
    ));
    fixture
}

fn bench_timing_snapshot(c: &mut Criterion) {
    const SEGMENTS: usize = 512;
    let fixture = many_timing_segments_fixture(SEGMENTS);
    let summary =
        rssp::analyze(fixture.as_bytes(), "ssc", &fast_options()).expect("fixture should analyze");
    let chart = summary
        .charts
        .first()
        .expect("fixture should contain a chart");

    let mut group = c.benchmark_group("report_timing");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(SEGMENTS as u64));
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

criterion_group!(benches, bench_timing_snapshot, bench_csv, bench_json);
criterion_main!(benches);
