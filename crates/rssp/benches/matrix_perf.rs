use criterion::{Criterion, Throughput, criterion_group, criterion_main};
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

fn bench_matrix_profile(c: &mut Criterion) {
    let inputs = build_inputs();
    let mut group = c.benchmark_group("matrix_profile");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(inputs.len() as u64));
    group.bench_function("legacy_build", |b| {
        b.iter(|| {
            let mut entries = 0usize;
            for input in &inputs {
                let profile = rssp::matrix::compute_matrix_profile_legacy_for_bench(
                    black_box(&input.densities),
                    black_box(&input.bpm_map),
                );
                entries += profile.len();
                black_box(profile);
            }
            black_box(entries);
        });
    });
    group.bench_function("reserved_build", |b| {
        b.iter(|| {
            let mut entries = 0usize;
            for input in &inputs {
                let profile = rssp::matrix::compute_matrix_profile_reserved_for_bench(
                    black_box(&input.densities),
                    black_box(&input.bpm_map),
                );
                entries += profile.len();
                black_box(profile);
            }
            black_box(entries);
        });
    });
    group.bench_function("optimized_build", |b| {
        b.iter(|| {
            let mut entries = 0usize;
            for input in &inputs {
                let profile = rssp::matrix::compute_matrix_profile(
                    black_box(&input.densities),
                    black_box(&input.bpm_map),
                );
                entries += profile.len();
                black_box(profile);
            }
            black_box(entries);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_matrix_profile);
criterion_main!(benches);
