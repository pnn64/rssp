use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

#[path = "support/course.rs"]
mod course_bench;

fn analyze_course(
    fixture: &course_bench::CourseFixture,
    options: rssp::AnalysisOptions,
) -> rssp::CourseSummary {
    rssp::course::analyze_crs_path(
        black_box(fixture.course_path()),
        Some(black_box(fixture.songs_dir())),
        black_box("dance-single"),
        black_box("Medium"),
        black_box(options),
    )
    .expect("benchmark course should analyze")
}

fn analyze_course_cache_all(
    fixture: &course_bench::CourseFixture,
    options: rssp::AnalysisOptions,
) -> rssp::CourseSummary {
    rssp::course::analyze_crs_path_cache_all_for_bench(
        black_box(fixture.course_path()),
        Some(black_box(fixture.songs_dir())),
        black_box("dance-single"),
        black_box("Medium"),
        black_box(options),
    )
    .expect("benchmark course should analyze")
}

fn bench_course_analysis(c: &mut Criterion) {
    let fixture = course_bench::CourseFixture::new();
    let fast_options = course_bench::fast_options();
    let clone_heavy_options = course_bench::clone_heavy_options();

    let mut group = c.benchmark_group("course_analysis");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    group.bench_function("analyze_64_cache_all", |b| {
        b.iter(|| {
            black_box(analyze_course_cache_all(
                &fixture,
                black_box(fast_options.clone()),
            ));
        });
    });
    group.bench_function("analyze_64_fast", |b| {
        b.iter(|| {
            black_box(analyze_course(&fixture, black_box(fast_options.clone())));
        });
    });
    group.bench_function("analyze_64_clone_heavy", |b| {
        b.iter(|| {
            black_box(analyze_course(
                &fixture,
                black_box(clone_heavy_options.clone()),
            ));
        });
    });
    group.finish();
}

fn bench_stepstype_match(c: &mut Criterion) {
    const CASES: [(&str, &str); 8] = [
        ("dance-single", "dance-single"),
        (" DANCE_SINGLE ", "dance-single"),
        ("dance-double", "dance-single"),
        ("DANCE-SOLO", "dance-single"),
        ("pump_single", "pump-single"),
        ("lights-cabinet", "lights-cabinet"),
        ("kb7-single", "dance-single"),
        ("非ASCII-single", "dance-single"),
    ];

    let mut group = c.benchmark_group("course_stepstype_match");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(CASES.len() as u64));
    group.bench_function("allocating", |b| {
        b.iter(|| {
            for (raw, normalized) in CASES {
                black_box(rssp::course::profile_stepstype_eq_legacy(
                    black_box(raw),
                    black_box(normalized),
                ));
            }
        });
    });
    group.bench_function("bytes", |b| {
        b.iter(|| {
            for (raw, normalized) in CASES {
                black_box(rssp::course::profile_stepstype_eq(
                    black_box(raw),
                    black_box(normalized),
                ));
            }
        });
    });
    group.finish();
}

criterion_group!(benches, bench_course_analysis, bench_stepstype_match);
criterion_main!(benches);
