use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rssp::step_parity::{analyze_timing_rows, timing_rows_scratch};
use rssp::timing::{TimingFormat, timing_data_from_chart_data};
use std::hint::black_box;
use std::time::Duration;

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("scan/parse");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for (name, ext, data) in [
        (
            "small_ssc",
            "ssc",
            &include_bytes!("fixtures/hash_fixture.ssc")[..],
        ),
        (
            "camellia",
            "ssc",
            &include_bytes!("fixtures/camellia_mix.ssc")[..],
        ),
        (
            "200k",
            "sm",
            &include_bytes!("fixtures/200000_step_challenge.sm")[..],
        ),
        (
            "24h",
            "sm",
            &include_bytes!("fixtures/24h_of_100bpm_stream.sm")[..],
        ),
    ] {
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &data, |b, data| {
            b.iter(|| {
                black_box(
                    rssp::parse::extract_sections(black_box(data), ext).expect("valid fixture"),
                )
            });
        });
    }
    group.finish();
}

fn bench_rows<const LANES: usize>(c: &mut Criterion) {
    let timing = timing_data_from_chart_data(
        0.0,
        0.0,
        None,
        "0.000=120.000",
        None,
        "",
        None,
        "",
        None,
        "",
        None,
        "",
        None,
        "",
        None,
        "",
        TimingFormat::Ssc,
        true,
    );
    let beats: Vec<_> = (0..4096u16).map(|row| f32::from(row) * 0.25).collect();
    let mut group = c.benchmark_group(format!("scan/parity{LANES}"));
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(beats.len() as u64));
    for name in ["taps", "empty", "early_hold", "late_hold"] {
        let mut rows = vec![[b'0'; LANES]; beats.len()];
        match name {
            "taps" => {
                for (i, row) in rows.iter_mut().enumerate() {
                    row[i % LANES] = b'1';
                }
            }
            "early_hold" => {
                rows[0][0] = b'2';
                rows[1][0] = b'3';
            }
            "late_hold" => {
                rows[4094][LANES - 1] = b'4';
                rows[4095][LANES - 1] = b'3';
            }
            _ => {}
        }
        let mut scratch = timing_rows_scratch::<LANES>().expect("supported layout");
        black_box(analyze_timing_rows(&rows, &beats, &timing, &mut scratch));
        group.bench_function(name, |b| {
            b.iter(|| {
                black_box(analyze_timing_rows(
                    black_box(&rows),
                    black_box(&beats),
                    black_box(&timing),
                    black_box(&mut scratch),
                ))
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse, bench_rows::<4>, bench_rows::<8>);
criterion_main!(benches);
