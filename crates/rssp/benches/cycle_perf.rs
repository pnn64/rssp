#[cfg(windows)]
use criterion::measurement::{Measurement, ValueFormatter};
#[cfg(windows)]
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
#[cfg(windows)]
use std::hint::black_box;
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
#[path = "support/step_parity.rs"]
mod step_parity_bench;

#[cfg(windows)]
#[derive(Clone, Copy)]
struct ThreadCycles;

#[cfg(windows)]
struct CycleFormatter;

#[cfg(windows)]
impl ValueFormatter for CycleFormatter {
    fn scale_values(&self, typical: f64, values: &mut [f64]) -> &'static str {
        let (scale, unit) = if typical < 1_000.0 {
            (1.0, "cycles")
        } else if typical < 1_000_000.0 {
            (1e-3, "Kcycles")
        } else if typical < 1_000_000_000.0 {
            (1e-6, "Mcycles")
        } else {
            (1e-9, "Gcycles")
        };
        for value in values {
            *value *= scale;
        }
        unit
    }

    fn scale_throughputs(
        &self,
        _typical: f64,
        throughput: &Throughput,
        values: &mut [f64],
    ) -> &'static str {
        let (items, unit) = match *throughput {
            Throughput::Elements(items) => (items as f64, "elem/Kcycle"),
            Throughput::Bytes(bytes) | Throughput::BytesDecimal(bytes) => {
                (bytes as f64, "bytes/Kcycle")
            }
            Throughput::Bits(bits) => (bits as f64, "bits/Kcycle"),
            Throughput::ElementsAndBytes { elements, .. } => (elements as f64, "elem/Kcycle"),
        };
        for value in values {
            *value = items * 1_000.0 / *value;
        }
        unit
    }

    fn scale_for_machines(&self, _values: &mut [f64]) -> &'static str {
        "cycles"
    }
}

#[cfg(windows)]
static CYCLE_FORMATTER: CycleFormatter = CycleFormatter;

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;

    type Handle = *mut c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> Handle;
        fn QueryThreadCycleTime(thread: Handle, cycles: *mut u64) -> i32;
        fn SetThreadAffinityMask(thread: Handle, mask: usize) -> usize;
        fn SetThreadPriority(thread: Handle, priority: i32) -> i32;
    }

    pub fn read_cycles() -> u64 {
        let mut cycles = 0;
        let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
        assert_ne!(ok, 0, "QueryThreadCycleTime failed");
        cycles
    }

    pub fn stabilize_thread() -> usize {
        let cpu_count = std::thread::available_parallelism().map_or(1, usize::from);
        let cpu = std::env::var("RSSP_BENCH_CPU")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|&cpu| cpu < cpu_count)
            .unwrap_or_else(|| if cpu_count > 2 { 2 } else { 0 });
        let thread = unsafe { GetCurrentThread() };
        let previous = unsafe { SetThreadAffinityMask(thread, 1usize << cpu) };
        assert_ne!(previous, 0, "SetThreadAffinityMask failed");
        const THREAD_PRIORITY_HIGHEST: i32 = 2;
        let ok = unsafe { SetThreadPriority(thread, THREAD_PRIORITY_HIGHEST) };
        assert_ne!(ok, 0, "SetThreadPriority failed");
        cpu
    }
}

#[cfg(windows)]
impl Measurement for ThreadCycles {
    type Intermediate = u64;
    type Value = u64;

    fn start(&self) -> Self::Intermediate {
        platform::read_cycles()
    }

    fn end(&self, start: Self::Intermediate) -> Self::Value {
        platform::read_cycles().saturating_sub(start)
    }

    fn add(&self, left: &Self::Value, right: &Self::Value) -> Self::Value {
        left.saturating_add(*right)
    }

    fn zero(&self) -> Self::Value {
        0
    }

    fn to_f64(&self, value: &Self::Value) -> f64 {
        *value as f64
    }

    fn formatter(&self) -> &dyn ValueFormatter {
        &CYCLE_FORMATTER
    }
}

#[cfg(windows)]
fn large_pair_map(entries: usize) -> String {
    use std::fmt::Write;

    let mut map = String::with_capacity(entries * 20);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        write!(&mut map, "{}={}", idx * 4, 60 + idx % 300).unwrap();
    }
    map
}

#[cfg(windows)]
fn large_speed_map(entries: usize) -> String {
    use std::fmt::Write;

    let mut map = String::with_capacity(entries * 28);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        write!(&mut map, "{}={}=0={}", idx * 4, 1 + idx % 7, idx & 1).unwrap();
    }
    map
}

#[cfg(windows)]
fn large_stop_map(entries: usize) -> String {
    use std::fmt::Write;

    let mut map = String::with_capacity(entries * 16);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        write!(&mut map, "{}=0.125", idx * 4).unwrap();
    }
    map
}

#[cfg(windows)]
fn cp1252_metadata(bytes: usize) -> Vec<u8> {
    (0..bytes)
        .map(|idx| if idx % 16 == 0 { 0x93 } else { b'a' })
        .collect()
}

#[cfg(windows)]
fn sorted_bpm_stats_reference(map: &[(f64, f64)]) -> (f64, f64) {
    let mut values: Vec<_> = map
        .iter()
        .map(|&(_, bpm)| bpm)
        .filter(|&bpm| bpm > 0.0 && bpm < 10_000.0)
        .collect();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if values.len() % 2 == 0 {
        f64::midpoint(values[values.len() / 2 - 1], values[values.len() / 2])
    } else {
        values[values.len() / 2]
    };
    (median, values.iter().sum::<f64>() / values.len() as f64)
}

#[cfg(windows)]
fn bench_cycles(c: &mut Criterion<ThreadCycles>) {
    const ENTRIES: usize = 4_096;
    let cpu = platform::stabilize_thread();
    eprintln!("cycle_perf measurement=QueryThreadCycleTime logical_cpu={cpu}");

    let pair_map = large_pair_map(ENTRIES);
    let speed_map = large_speed_map(ENTRIES);
    let stop_map = large_stop_map(ENTRIES);
    let legacy_metadata = cp1252_metadata(ENTRIES);
    let valid_tech = "BR+ FS- 24ths XO+ SKT- 32nds DS++ JA- WA+ BXF- ".repeat(64);
    let invalid_tech = "BR+garbage Hard unknown ".repeat(64);
    let row_segments = rssp::timing::TimingSegments {
        beat0_offset_adjust: 0.0,
        bpms: vec![(0.0, 120.0)],
        stops: (0..ENTRIES).map(|idx| (idx as f32 * 4.0, 0.125)).collect(),
        delays: Vec::new(),
        warps: Vec::new(),
        speeds: Vec::new(),
        scrolls: Vec::new(),
        fakes: Vec::new(),
    };
    let bpm_segments = rssp::timing::TimingSegments {
        beat0_offset_adjust: 0.0,
        bpms: (0..ENTRIES)
            .map(|idx| (idx as f32 * 4.0, 60.0 + (idx % 300) as f32))
            .collect(),
        stops: Vec::new(),
        delays: Vec::new(),
        warps: Vec::new(),
        speeds: Vec::new(),
        scrolls: Vec::new(),
        fakes: Vec::new(),
    };
    let bpm_stats_map: Vec<_> = (0..ENTRIES)
        .map(|idx| (idx as f64 * 4.0, 60.0 + (idx % 300) as f64))
        .collect();

    let mut parsing = c.benchmark_group("cycles/parsing");
    parsing.throughput(Throughput::Elements(ENTRIES as u64));
    parsing.sample_size(100);
    parsing.measurement_time(Duration::from_secs(2));
    parsing.bench_function("bpm_map", |b| {
        b.iter(|| {
            black_box(rssp::bpm::parse_bpm_map(black_box(&pair_map)));
        });
    });
    parsing.finish();

    let mut tech_notation = c.benchmark_group("cycles/tech_notation");
    tech_notation.sample_size(100);
    tech_notation.measurement_time(Duration::from_secs(2));
    tech_notation.throughput(Throughput::Bytes(valid_tech.len() as u64));
    tech_notation.bench_function("valid", |b| {
        b.iter(|| {
            black_box(rssp::tech::parse_tech_notation(
                black_box(&valid_tech),
                black_box(""),
            ));
        });
    });
    tech_notation.throughput(Throughput::Bytes(invalid_tech.len() as u64));
    tech_notation.bench_function("invalid", |b| {
        b.iter(|| {
            black_box(rssp::tech::parse_tech_notation(
                black_box(""),
                black_box(&invalid_tech),
            ));
        });
    });
    tech_notation.finish();

    let mut decoding = c.benchmark_group("cycles/decoding");
    decoding.sample_size(100);
    decoding.measurement_time(Duration::from_secs(2));
    decoding.throughput(Throughput::Bytes(legacy_metadata.len() as u64));
    decoding.bench_function("cp1252_metadata", |b| {
        b.iter(|| {
            black_box(rssp::parse::decode_bytes(black_box(&legacy_metadata)));
        });
    });
    decoding.finish();

    let mut normalization = c.benchmark_group("cycles/normalization");
    normalization.throughput(Throughput::Bytes(pair_map.len() as u64));
    normalization.sample_size(100);
    normalization.measurement_time(Duration::from_secs(2));
    normalization.bench_function("pair_map_separate", |b| {
        b.iter(|| {
            black_box((
                rssp::bpm::clean_timing_map(black_box(&pair_map)),
                rssp::bpm::normalize_float_digits(black_box(&pair_map)),
            ));
        });
    });
    normalization.bench_function("pair_map_fused", |b| {
        b.iter(|| {
            black_box(rssp::bpm::clean_and_normalize_float_digits(black_box(
                &pair_map,
            )));
        });
    });
    normalization.throughput(Throughput::Bytes(speed_map.len() as u64));
    normalization.bench_function("speed_map_separate", |b| {
        b.iter(|| {
            black_box((
                rssp::bpm::clean_timing_map(black_box(&speed_map)),
                rssp::bpm::normalize_speeds_float_digits(black_box(&speed_map)),
            ));
        });
    });
    normalization.bench_function("speed_map_fused", |b| {
        b.iter(|| {
            black_box(rssp::bpm::clean_and_normalize_speeds_float_digits(
                black_box(&speed_map),
            ));
        });
    });
    normalization.throughput(Throughput::Elements(bpm_stats_map.len() as u64));
    normalization.bench_function("bpm_stats_values", |b| {
        b.iter(|| {
            let values: Vec<_> = black_box(&bpm_stats_map)
                .iter()
                .map(|&(_, bpm)| bpm)
                .collect();
            black_box(rssp::bpm::compute_bpm_stats(&values));
        });
    });
    normalization.bench_function("bpm_stats_map", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_bpm_map_stats(black_box(&bpm_stats_map)));
        });
    });
    normalization.bench_function("bpm_stats_sorted_reference", |b| {
        b.iter(|| {
            black_box(sorted_bpm_stats_reference(black_box(&bpm_stats_map)));
        });
    });
    normalization.bench_function("bpm_summary_separate", |b| {
        b.iter(|| {
            black_box((
                rssp::bpm::compute_bpm_range(black_box(&bpm_stats_map)),
                rssp::bpm::compute_bpm_map_stats(black_box(&bpm_stats_map)),
            ));
        });
    });
    normalization.bench_function("bpm_summary_combined", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_bpm_range_and_stats(black_box(
                &bpm_stats_map,
            )));
        });
    });
    normalization.finish();

    let mut cleanup = c.benchmark_group("cycles/cleanup");
    cleanup.throughput(Throughput::Elements(ENTRIES as u64));
    cleanup.sample_size(100);
    cleanup.measurement_time(Duration::from_secs(2));
    cleanup.bench_function("ordered_speeds", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                0.0,
                0.0,
                None,
                "0=120",
                None,
                "",
                None,
                "",
                None,
                "",
                None,
                black_box(&speed_map),
                None,
                "",
                None,
                "",
                rssp::timing::TimingFormat::Ssc,
                true,
            ));
        });
    });
    cleanup.bench_function("ordered_scrolls", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                0.0,
                0.0,
                None,
                "0=120",
                None,
                "",
                None,
                "",
                None,
                "",
                None,
                "",
                None,
                black_box(&pair_map),
                None,
                "",
                rssp::timing::TimingFormat::Ssc,
                true,
            ));
        });
    });
    cleanup.bench_function("ordered_segment_rows", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_segments(
                0.0,
                0.0,
                black_box(&row_segments),
            ));
        });
    });
    cleanup.bench_function("ordered_sm_bpms", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                0.0,
                0.0,
                None,
                black_box(&pair_map),
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
                rssp::timing::TimingFormat::Sm,
                true,
            ));
        });
    });
    cleanup.bench_function("ordered_sm_stops", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                0.0,
                0.0,
                None,
                "0=120",
                None,
                black_box(&stop_map),
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
                rssp::timing::TimingFormat::Sm,
                true,
            ));
        });
    });
    cleanup.bench_function("many_bpms_from_segments", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_segments(
                0.0,
                0.0,
                black_box(&bpm_segments),
            ));
        });
    });
    cleanup.finish();

    let stream_densities: Vec<_> = (0..16_384)
        .map(|idx| match idx % 23 {
            0..=7 => 16,
            8..=11 => 20,
            12..=14 => 24,
            15..=16 => 32,
            _ => 0,
        })
        .collect();
    let matrix_densities: Vec<_> = (0..2_048).map(|idx| [16, 20, 24, 32][idx & 3]).collect();
    let matrix_bpms: Vec<_> = (0..1_024)
        .map(|idx| (idx as f64 * 8.0, 60.0 + idx as f64 * 0.125))
        .collect();
    let nps_values: Vec<_> = (0..1_025)
        .map(|idx| ((idx * 37) % 257) as f64 / 7.0)
        .collect();
    let mut nps_scratch = Vec::new();

    let mut optimizations = c.benchmark_group("cycles/optimizations");
    optimizations.sample_size(100);
    optimizations.measurement_time(Duration::from_secs(2));
    optimizations.throughput(Throughput::Elements(stream_densities.len() as u64));
    optimizations.bench_function("stream_outputs", |b| {
        b.iter(|| {
            black_box(rssp::stats::compute_stream_outputs(black_box(
                &stream_densities,
            )));
        });
    });
    optimizations.throughput(Throughput::Elements(matrix_densities.len() as u64));
    optimizations.bench_function("matrix_many_bpms", |b| {
        b.iter(|| {
            black_box(rssp::matrix::compute_matrix_rating(
                black_box(&matrix_densities),
                black_box(&matrix_bpms),
            ));
        });
    });
    const NPS_BATCH: usize = 256;
    optimizations.throughput(Throughput::Elements((nps_values.len() * NPS_BATCH) as u64));
    optimizations.bench_function("nps_stats_allocating", |b| {
        b.iter(|| {
            for _ in 0..NPS_BATCH {
                black_box(rssp::bpm::get_nps_stats(black_box(&nps_values)));
            }
        });
    });
    optimizations.bench_function("nps_stats_reused", |b| {
        b.iter(|| {
            for _ in 0..NPS_BATCH {
                black_box(rssp::bpm::get_nps_stats_with_scratch(
                    black_box(&nps_values),
                    black_box(&mut nps_scratch),
                ));
            }
        });
    });
    optimizations.finish();

    let parity_timing = step_parity_bench::timing();
    let single_rows = step_parity_bench::rows::<4>(
        step_parity_bench::SINGLE_ROW_COUNT,
        step_parity_bench::SINGLE_MASKS,
    );
    let single_beats = step_parity_bench::beats(step_parity_bench::SINGLE_ROW_COUNT);
    let single_hold_rows = step_parity_bench::hold_rows::<4>(
        step_parity_bench::SINGLE_ROW_COUNT,
        step_parity_bench::SINGLE_MASKS,
    );
    let double_rows = step_parity_bench::rows::<8>(
        step_parity_bench::DOUBLE_ROW_COUNT,
        step_parity_bench::DOUBLE_MASKS,
    );
    let double_beats = step_parity_bench::beats(step_parity_bench::DOUBLE_ROW_COUNT);
    let double_hold_rows = step_parity_bench::hold_rows::<8>(
        step_parity_bench::DOUBLE_ROW_COUNT,
        step_parity_bench::DOUBLE_MASKS,
    );
    let mut single_scratch =
        rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
    let mut double_scratch =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");

    let mut parity = c.benchmark_group("cycles/step_parity");
    parity.sample_size(50);
    parity.measurement_time(Duration::from_secs(3));
    parity.throughput(Throughput::Elements(
        step_parity_bench::SINGLE_ROW_COUNT as u64,
    ));
    parity.bench_function("dense_single", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_known_holds(
                black_box(&single_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                false,
                black_box(&mut single_scratch),
            ));
        });
    });
    parity.bench_function("dense_single_holds", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_known_holds(
                black_box(&single_hold_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                true,
                black_box(&mut single_scratch),
            ));
        });
    });
    parity.throughput(Throughput::Elements(
        step_parity_bench::DOUBLE_ROW_COUNT as u64,
    ));
    parity.bench_function("dense_double", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_known_holds(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                black_box(&mut double_scratch),
            ));
        });
    });
    parity.bench_function("dense_double_holds", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_known_holds(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                black_box(&mut double_scratch),
            ));
        });
    });
    parity.finish();
}

#[cfg(windows)]
criterion_group! {
    name = benches;
    config = Criterion::default().with_measurement(ThreadCycles);
    targets = bench_cycles
}
#[cfg(windows)]
criterion_main!(benches);

#[cfg(not(windows))]
fn main() {
    eprintln!("cycle_perf requires Windows per-thread cycle accounting");
}
