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

fn bench_course_analysis(c: &mut Criterion) {
    let fixture = course_bench::CourseFixture::new();
    let fast_options = course_bench::fast_options();
    let clone_heavy_options = course_bench::clone_heavy_options();

    let mut group = c.benchmark_group("course_analysis");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
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

criterion_group!(benches, bench_course_analysis);
criterion_main!(benches);
