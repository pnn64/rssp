use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

const FIXTURES: [(&str, &str); 3] = [
    (include_str!("fixtures/camellia_mix.ssc"), "ssc"),
    (include_str!("fixtures/watch_yo_step.ssc"), "ssc"),
    (include_str!("fixtures/200000_step_challenge.sm"), "sm"),
];

#[derive(Clone)]
struct MatrixInput {
    densities: Vec<usize>,
    bpm_map: Vec<(f64, f64)>,
}

fn build_inputs() -> Vec<MatrixInput> {
    let mut inputs = Vec::new();
    let options = rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..rssp::AnalysisOptions::default()
    };

    for (raw, ext) in FIXTURES {
        let summary = rssp::analyze(raw.as_bytes(), ext, &options).expect("fixture should analyze");
        for chart in summary.charts {
            let bpm_map = chart
                .timing_segments
                .bpms
                .iter()
                .map(|(beat, bpm)| (f64::from(*beat), f64::from(*bpm)))
                .collect();
            inputs.push(MatrixInput {
                densities: chart.measure_densities,
                bpm_map,
            });
        }
    }

    assert!(!inputs.is_empty(), "fixtures should contain charts");
    inputs
}

fn bench_matrix_rating(c: &mut Criterion) {
    let inputs = build_inputs();
    let mut group = c.benchmark_group("matrix");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_matrix_rating", |b| {
        b.iter(|| {
            let mut total = 0.0;
            for input in &inputs {
                total += rssp::matrix::compute_matrix_rating(
                    black_box(&input.densities),
                    black_box(&input.bpm_map),
                );
            }
            black_box(total);
        });
    });
    group.finish();
}

fn bench_many_unique_bpms(c: &mut Criterion) {
    let densities: Vec<_> = (0..2_048).map(|idx| [16, 20, 24, 32][idx & 3]).collect();
    let bpm_map: Vec<_> = (0..1_024)
        .map(|idx| (idx as f64 * 8.0, 60.0 + idx as f64 * 0.125))
        .collect();

    let mut group = c.benchmark_group("matrix_many_bpms");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("unique_segments", |b| {
        b.iter(|| {
            black_box(rssp::matrix::compute_matrix_rating(
                black_box(&densities),
                black_box(&bpm_map),
            ));
        });
    });
    group.finish();
}

fn bench_few_variable_bpms(c: &mut Criterion) {
    let densities: Vec<_> = (0..1_024).map(|idx| [16, 20, 24, 32][idx & 3]).collect();
    let bpm_map: Vec<_> = (0..16)
        .map(|idx| (idx as f64 * 32.0, 120.0 + (idx % 7) as f64 * 15.0))
        .collect();

    let mut group = c.benchmark_group("matrix_few_bpms");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("variable_segments", |b| {
        b.iter(|| {
            black_box(rssp::matrix::compute_matrix_rating(
                black_box(&densities),
                black_box(&bpm_map),
            ));
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_matrix_rating,
    bench_many_unique_bpms,
    bench_few_variable_bpms
);
criterion_main!(benches);
