use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/variation.ssc");
const MONO_THRESHOLD: usize = 6;

fn step_type_lanes(step_type: &str) -> usize {
    let normalized = step_type.trim().to_ascii_lowercase().replace('_', "-");
    if normalized == "dance-double" {
        8
    } else {
        4
    }
}

fn generate_bitmasks(minimized_chart: &[u8]) -> Vec<u8> {
    minimized_chart
        .split(|&b| b == b'\n')
        .filter_map(|line| {
            if line.len() < 4 || line.iter().all(|&b| b == b' ' || b == b',') {
                return None;
            }

            let mut mask = 0u8;
            for i in 0..4 {
                if matches!(line[i], b'1' | b'2' | b'4') {
                    mask |= 1 << i;
                }
            }
            Some(mask)
        })
        .collect()
}

fn build_bitmasks() -> Vec<u8> {
    let parsed = rssp::parse::extract_sections(FIXTURE.as_bytes(), "ssc")
        .expect("fixture should parse");

    let mut best_chart: Option<(usize, Vec<u8>)> = None;
    for entry in parsed.notes_list {
        let (fields, chart_data) = rssp::parse::split_notes_fields(&entry.notes);
        if fields.len() < 5 {
            continue;
        }

        let step_type = std::str::from_utf8(fields[0]).unwrap_or("").trim();
        if step_type == "lights-cabinet" {
            continue;
        }

        let lanes = step_type_lanes(step_type);
        if lanes != 4 {
            continue;
        }

        let (mut minimized_chart, stats, _measure_densities) =
            rssp::stats::minimize_chart_and_count_with_lanes(chart_data, lanes);
        if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
            minimized_chart.truncate(pos + 1);
        }

        let total_steps = stats.total_steps as usize;
        match best_chart {
            Some((best_steps, _)) if best_steps >= total_steps => {}
            _ => {
                best_chart = Some((total_steps, minimized_chart));
            }
        }
    }

    let (_, minimized_chart) = best_chart.expect("fixture should contain a 4-lane chart");
    generate_bitmasks(&minimized_chart)
}

fn bench_mono_counts(c: &mut Criterion) {
    let bitmasks = build_bitmasks();
    let mut group = c.benchmark_group("mono");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("count_facing_steps", |b| {
        b.iter(|| {
            let counts = rssp::patterns::count_facing_steps(
                black_box(&bitmasks),
                black_box(MONO_THRESHOLD),
            );
            black_box(counts);
        })
    });
    group.finish();
}

criterion_group!(benches, bench_mono_counts);
criterion_main!(benches);
