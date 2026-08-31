use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

#[path = "support/pattern_scratch.rs"]
mod pattern_scratch_bench;

const FIXTURE: &str = include_str!("fixtures/camellia_mix.ssc");
const BATCH_FIXTURE: &[u8] = b"#VERSION:0.83;#TITLE:Batch;#BPMS:0=120;\
#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Challenge;#METER:10;\
#NOTES:\n1000\n0100\n0010\n0001\n;";
const MONO_THRESHOLD: usize = 6;

fn step_type_lanes(step_type: &str) -> usize {
    let normalized = step_type.trim().to_ascii_lowercase().replace('_', "-");
    if normalized == "dance-double" { 8 } else { 4 }
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

fn generate_rows(minimized_chart: &[u8]) -> Vec<[u8; 4]> {
    minimized_chart
        .split(|&b| b == b'\n')
        .filter_map(|line| {
            if line.len() < 4 || line[0] == b',' {
                return None;
            }
            Some([line[0], line[1], line[2], line[3]])
        })
        .collect()
}

fn build_pattern_input() -> (Vec<[u8; 4]>, Vec<u8>) {
    let parsed =
        rssp::parse::extract_sections(FIXTURE.as_bytes(), "ssc").expect("fixture should parse");

    let mut best_chart: Option<(usize, Vec<u8>)> = None;
    for entry in parsed.notes_list {
        if entry.field_count < 5 {
            continue;
        }

        let step_type = std::str::from_utf8(entry.fields[0]).unwrap_or("").trim();
        if step_type == "lights-cabinet" {
            continue;
        }

        let lanes = step_type_lanes(step_type);
        if lanes != 4 {
            continue;
        }

        let (mut minimized_chart, stats, _measure_densities) =
            rssp::stats::minimize_chart_and_count_with_lanes(entry.note_data, lanes);
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
    (
        generate_rows(&minimized_chart),
        generate_bitmasks(&minimized_chart),
    )
}

fn custom_pattern_input(unique_count: usize) -> Vec<String> {
    const DIRECTIONS: [u8; 4] = *b"LDUR";
    let mut patterns = Vec::with_capacity(unique_count * 3);
    for mut value in 0..unique_count {
        let mut bytes = [b'L'; 8];
        for byte in &mut bytes {
            *byte = DIRECTIONS[value & 3];
            value >>= 2;
        }
        let pattern = String::from_utf8(bytes.to_vec()).expect("directions are valid UTF-8");
        patterns.push(pattern.clone());
        patterns.push(pattern.to_ascii_lowercase());
        patterns.push(pattern);
    }
    patterns
}

fn bench_mono_counts(c: &mut Criterion) {
    let (rows, bitmasks) = build_pattern_input();
    let mut group = c.benchmark_group("mono");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("count_facing_steps", |b| {
        b.iter(|| {
            let counts =
                rssp::patterns::count_facing_steps(black_box(&bitmasks), black_box(MONO_THRESHOLD));
            black_box(counts);
        });
    });
    group.finish();

    let compiled = rssp::patterns::compiled_custom_empty();
    let mut group = c.benchmark_group("pattern_pipeline");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("separate_with_bitmask_buffer", |b| {
        b.iter(|| {
            let masks: Vec<_> = black_box(&rows)
                .iter()
                .map(|row| {
                    u8::from(matches!(row[0], b'1' | b'2' | b'4'))
                        | (u8::from(matches!(row[1], b'1' | b'2' | b'4')) << 1)
                        | (u8::from(matches!(row[2], b'1' | b'2' | b'4')) << 2)
                        | (u8::from(matches!(row[3], b'1' | b'2' | b'4')) << 3)
                })
                .collect();
            black_box((
                rssp::patterns::detect_default_patterns(&masks),
                rssp::patterns::count_anchors(&masks),
                rssp::patterns::count_facing_steps(&masks, MONO_THRESHOLD),
            ));
        });
    });
    group.bench_function("fused_rows", |b| {
        b.iter(|| {
            black_box(rssp::patterns::analyze_patterns_from_rows(
                black_box(&rows),
                black_box(MONO_THRESHOLD),
                black_box(&compiled),
            ));
        });
    });
    group.finish();
}

fn bench_custom_patterns(c: &mut Criterion) {
    const UNIQUE_PATTERNS: usize = 256;
    let patterns = custom_pattern_input(UNIQUE_PATTERNS);

    let mut compile_group = c.benchmark_group("custom_patterns_compile");
    compile_group.sample_size(100);
    compile_group.measurement_time(Duration::from_secs(2));
    compile_group.throughput(Throughput::Elements(patterns.len() as u64));
    compile_group.bench_function("legacy_hashmap", |b| {
        b.iter(|| {
            black_box(rssp::patterns::compile_custom_patterns_legacy_for_bench(
                black_box(&patterns),
            ));
        });
    });
    compile_group.bench_function("open_addressed_growing_dfa", |b| {
        b.iter(|| {
            black_box(
                rssp::patterns::compile_custom_patterns_growing_dfa_for_bench(black_box(&patterns)),
            );
        });
    });
    compile_group.bench_function("open_addressed_presized_dfa", |b| {
        b.iter(|| {
            black_box(rssp::patterns::compile_custom_patterns(black_box(
                &patterns,
            )));
        });
    });
    compile_group.finish();

    let compiled = rssp::patterns::compile_custom_patterns(&patterns);
    let rows = pattern_scratch_bench::rows();
    pattern_scratch_bench::assert_behavior(&rows, MONO_THRESHOLD, &compiled);
    let mut counts = Vec::new();
    let mut analysis_group = c.benchmark_group("custom_pattern_count_scratch");
    analysis_group.sample_size(100);
    analysis_group.measurement_time(Duration::from_secs(2));
    analysis_group.throughput(Throughput::Elements(rows.len() as u64));
    analysis_group.bench_function("allocating", |b| {
        b.iter(|| {
            black_box(rssp::patterns::analyze_patterns_from_rows(
                black_box(&rows),
                black_box(MONO_THRESHOLD),
                black_box(&compiled),
            ));
        });
    });
    analysis_group.bench_function("reused", |b| {
        b.iter(|| {
            black_box(rssp::patterns::analyze_patterns_from_rows_with_scratch(
                black_box(&rows),
                black_box(MONO_THRESHOLD),
                black_box(&compiled),
                black_box(&mut counts),
            ));
        });
    });
    analysis_group.finish();

    let options = rssp::AnalysisOptions {
        custom_patterns: patterns,
        compute_tech_counts: false,
        ..rssp::AnalysisOptions::default()
    };
    let prepared = rssp::PreparedAnalysis::new(options.clone());
    let mut prepared_scratch = rssp::AnalysisScratch::default();
    let expected = rssp::analyze(BATCH_FIXTURE, "ssc", &options.clone())
        .expect("fresh batch analysis should succeed");
    let actual = rssp::analyze_prepared_in(BATCH_FIXTURE, "ssc", &prepared, &mut prepared_scratch)
        .expect("prepared batch analysis should succeed");
    let (mut expected_json, mut actual_json) = (Vec::new(), Vec::new());
    rssp::report::write_reports(
        &expected,
        rssp::report::OutputMode::JSON,
        &mut expected_json,
    )
    .expect("fresh batch summary should serialize");
    rssp::report::write_reports(&actual, rssp::report::OutputMode::JSON, &mut actual_json)
        .expect("prepared batch summary should serialize");
    assert_eq!(actual_json, expected_json);
    let mut batch_group = c.benchmark_group("custom_patterns_batch");
    batch_group.sample_size(100);
    batch_group.measurement_time(Duration::from_secs(2));
    batch_group.bench_function("fresh_each_file", |b| {
        b.iter(|| {
            black_box(rssp::analyze(
                black_box(BATCH_FIXTURE),
                "ssc",
                black_box(&options.clone()),
            ))
        });
    });
    batch_group.bench_function("prepared_reused", |b| {
        b.iter(|| {
            black_box(rssp::analyze_prepared_in(
                black_box(BATCH_FIXTURE),
                "ssc",
                black_box(&prepared),
                black_box(&mut prepared_scratch),
            ))
        });
    });
    batch_group.finish();
}

criterion_group!(benches, bench_mono_counts, bench_custom_patterns);
criterion_main!(benches);
