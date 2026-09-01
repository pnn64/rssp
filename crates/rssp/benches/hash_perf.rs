use criterion::{Criterion, criterion_group, criterion_main};
use std::borrow::Cow;
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/camellia_mix.ssc");

#[path = "support/metadata.rs"]
mod metadata_bench;

#[derive(Clone)]
struct ChartInput {
    field_count: u8,
    fields: [&'static [u8]; 5],
    note_data: &'static [u8],
    chart_bpms: Option<Vec<u8>>,
}

fn step_type_lanes(step_type: &str) -> usize {
    let normalized = step_type.trim().to_ascii_lowercase().replace('_', "-");
    if normalized == "dance-double" { 8 } else { 4 }
}

fn build_chart_inputs() -> (Vec<ChartInput>, String) {
    let parsed =
        rssp::parse::extract_sections(FIXTURE.as_bytes(), "ssc").expect("fixture should parse");
    let normalized_global_bpms = {
        let raw = std::str::from_utf8(parsed.bpms.unwrap_or(b"")).unwrap_or("");
        rssp::bpm::normalize_float_digits(raw)
    };
    let charts = parsed
        .notes_list
        .into_iter()
        .map(|entry| ChartInput {
            field_count: entry.field_count,
            fields: entry.fields,
            note_data: entry.note_data,
            chart_bpms: entry.chart_bpms.map(std::borrow::Cow::into_owned),
        })
        .collect();
    (charts, normalized_global_bpms)
}

fn bench_hash_pipeline(c: &mut Criterion) {
    let fixture = FIXTURE.as_bytes();
    let mut group = c.benchmark_group("hash_pipeline");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_all_hashes", |b| {
        b.iter(|| {
            let hashes = rssp::compute_all_hashes(black_box(fixture), black_box("ssc"))
                .expect("hashing should succeed");
            black_box(hashes);
        });
    });
    group.finish();
}

fn bench_hash_inner(c: &mut Criterion) {
    let (charts, normalized_global_bpms) = build_chart_inputs();
    let mut group = c.benchmark_group("hash_inner");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("minimize_bpm_hash", |b| {
        b.iter(|| {
            let mut hashes = Vec::with_capacity(charts.len());
            for entry in &charts {
                if entry.field_count < 5 {
                    continue;
                }

                let step_type = std::str::from_utf8(entry.fields[0]).unwrap_or("").trim();
                if step_type == "lights-cabinet" {
                    continue;
                }

                let lanes = step_type_lanes(step_type);
                let mut minimized_chart =
                    rssp::stats::minimize_chart_for_hash(entry.note_data, lanes);
                if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
                    minimized_chart.truncate(pos + 1);
                }

                let bpms_to_use = if let Some(chart_bpms) = entry.chart_bpms.as_ref() {
                    let normalized = rssp::bpm::normalize_float_digits(
                        std::str::from_utf8(chart_bpms).unwrap_or(""),
                    );
                    Cow::Owned(normalized)
                } else {
                    Cow::Borrowed(normalized_global_bpms.as_str())
                };

                let hash = rssp::hash::compute_chart_hash(&minimized_chart, bpms_to_use.as_ref());
                hashes.push(hash);
            }
            black_box(hashes);
        });
    });
    group.bench_function("streaming_minimize_bpm_hash", |b| {
        b.iter(|| {
            let mut hashes = Vec::with_capacity(charts.len());
            for entry in &charts {
                if entry.field_count < 5 {
                    continue;
                }

                let step_type = std::str::from_utf8(entry.fields[0]).unwrap_or("").trim();
                if step_type == "lights-cabinet" {
                    continue;
                }

                let lanes = step_type_lanes(step_type);
                let bpms_to_use = if let Some(chart_bpms) = entry.chart_bpms.as_ref() {
                    let normalized = rssp::bpm::normalize_float_digits(
                        std::str::from_utf8(chart_bpms).unwrap_or(""),
                    );
                    Cow::Owned(normalized)
                } else {
                    Cow::Borrowed(normalized_global_bpms.as_str())
                };

                hashes.push(rssp::hash::compute_note_data_hash(
                    entry.note_data,
                    lanes,
                    bpms_to_use.as_ref(),
                ));
            }
            black_box(hashes);
        });
    });
    group.finish();
}

fn assert_hashes_eq(current: &[rssp::ChartHashInfo], legacy: &[rssp::ChartHashInfo]) {
    assert_eq!(current.len(), legacy.len());
    for (current, legacy) in current.iter().zip(legacy) {
        assert_eq!(current.step_type, legacy.step_type);
        assert_eq!(current.difficulty, legacy.difficulty);
        assert_eq!(current.hash, legacy.hash);
    }
}

fn hash_batch(data: &[u8], legacy: bool) -> Vec<rssp::ChartHashInfo> {
    rssp::analysis::profile_compute_all_hashes(black_box(data), "ssc", legacy)
        .expect("hash scratch fixture should hash")
}

#[allow(clippy::cast_precision_loss)]
fn print_hash_scratch_pairs(data: &[u8]) {
    const SAMPLES: usize = 31;
    assert_hashes_eq(&hash_batch(data, false), &hash_batch(data, true));
    let mut legacy = [0u128; SAMPLES];
    let mut reused = [0u128; SAMPLES];
    let mut ratios = [0.0f64; SAMPLES];
    for sample in 0..SAMPLES {
        let elapsed = |legacy| {
            const ITERATIONS: usize = 32;
            let start = std::time::Instant::now();
            for _ in 0..ITERATIONS {
                black_box(hash_batch(data, legacy));
            }
            start.elapsed().as_nanos()
        };
        let (legacy_ns, reused_ns) = if sample.is_multiple_of(2) {
            (elapsed(true), elapsed(false))
        } else {
            let reused_ns = elapsed(false);
            (elapsed(true), reused_ns)
        };
        legacy[sample] = legacy_ns;
        reused[sample] = reused_ns;
        ratios[sample] = reused_ns as f64 / legacy_ns as f64;
    }
    legacy.sort_unstable();
    reused.sort_unstable();
    ratios.sort_by(f64::total_cmp);
    let mid = SAMPLES / 2;
    eprintln!(
        concat!(
            "hash_scratch paired_samples={} per_chart_median_ns={} reused_median_ns={} ",
            "median_change={:+.3}%"
        ),
        SAMPLES,
        legacy[mid],
        reused[mid],
        (ratios[mid] - 1.0) * 100.0,
    );
}

fn bench_hash_scratch(c: &mut Criterion) {
    let fixture = metadata_bench::fixture("0.83");
    print_hash_scratch_pairs(fixture.as_bytes());
    let mut group = c.benchmark_group("hash_scratch_256_charts");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(criterion::Throughput::Elements(
        metadata_bench::CHART_COUNT as u64,
    ));
    for (name, legacy) in [("per_chart_buffers", true), ("reused_lane_buffers", false)] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(hash_batch(fixture.as_bytes(), legacy)));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_hash_pipeline,
    bench_hash_inner,
    bench_hash_scratch
);
criterion_main!(benches);
