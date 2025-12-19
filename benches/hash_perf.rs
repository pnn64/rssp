use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::borrow::Cow;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/hash_fixture.ssc");

#[derive(Clone)]
struct ChartInput {
    notes: Vec<u8>,
    chart_bpms: Option<Vec<u8>>,
}

fn step_type_lanes(step_type: &str) -> usize {
    let normalized = step_type.trim().to_ascii_lowercase().replace('_', "-");
    if normalized == "dance-double" {
        8
    } else {
        4
    }
}

fn build_chart_inputs() -> (Vec<ChartInput>, String) {
    let parsed = rssp::parse::extract_sections(FIXTURE.as_bytes(), "ssc")
        .expect("fixture should parse");
    let normalized_global_bpms = {
        let raw = std::str::from_utf8(parsed.bpms.unwrap_or(b"")).unwrap_or("");
        rssp::bpm::normalize_float_digits(raw)
    };
    let charts = parsed
        .notes_list
        .into_iter()
        .map(|entry| ChartInput {
            notes: entry.notes,
            chart_bpms: entry.chart_bpms,
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
        })
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
                let (fields, chart_data) = rssp::parse::split_notes_fields(&entry.notes);
                if fields.len() < 5 {
                    continue;
                }

                let step_type = std::str::from_utf8(fields[0]).unwrap_or("").trim();
                if step_type == "lights-cabinet" {
                    continue;
                }

                let lanes = step_type_lanes(step_type);
                let mut minimized_chart = rssp::stats::minimize_chart_for_hash(chart_data, lanes);
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

                let hash = rssp::hashing::compute_chart_hash(&minimized_chart, bpms_to_use.as_ref());
                hashes.push(hash);
            }
            black_box(hashes);
        })
    });
    group.finish();
}

criterion_group!(benches, bench_hash_pipeline, bench_hash_inner);
criterion_main!(benches);
