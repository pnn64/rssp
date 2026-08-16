#[cfg(windows)]
use criterion::measurement::{Measurement, ValueFormatter};
#[cfg(windows)]
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
#[cfg(windows)]
use std::fmt::Write as _;
#[cfg(windows)]
use std::hint::black_box;
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
#[allow(dead_code)]
#[path = "support/assets.rs"]
mod assets_bench;
#[cfg(windows)]
#[allow(dead_code)]
#[path = "support/course.rs"]
mod course_bench;
#[cfg(windows)]
#[path = "support/elapsed.rs"]
mod elapsed_bench;
#[cfg(windows)]
#[path = "support/nps_stats.rs"]
mod nps_stats_bench;
#[cfg(windows)]
#[path = "support/pack.rs"]
mod pack_bench;
#[cfg(windows)]
#[path = "support/report_nps.rs"]
mod report_nps_bench;
#[cfg(windows)]
#[path = "support/report_patterns.rs"]
mod report_patterns_bench;
#[cfg(windows)]
#[path = "support/report_timing.rs"]
mod report_timing_bench;
#[cfg(windows)]
#[path = "support/serialize.rs"]
mod serialize_bench;
#[cfg(windows)]
#[path = "support/sm_timing.rs"]
mod sm_timing_bench;
#[cfg(windows)]
#[path = "support/step_parity.rs"]
mod step_parity_bench;
#[cfg(windows)]
#[path = "support/timing_merge.rs"]
mod timing_merge_bench;
#[cfg(windows)]
#[path = "support/translate.rs"]
mod translate_bench;

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
fn typed_rows_owned(data: &[u8]) -> usize {
    let (chart, stats, densities, rows, beats, last) = rssp::stats::minimize_rows_typed::<4>(data);
    let checksum = chart
        .len()
        .wrapping_add(stats.total_arrows as usize)
        .wrapping_add(densities.len())
        .wrapping_add(rows.len())
        .wrapping_add(beats.len())
        .wrapping_add(last.to_bits() as usize);
    black_box((
        chart.as_slice(),
        &stats,
        densities.as_slice(),
        rows.as_slice(),
        beats.as_slice(),
        last,
    ));
    checksum
}

#[cfg(windows)]
fn typed_rows_reused(data: &[u8], scratch: &mut rssp::stats::TypedRowsScratch<4>) -> usize {
    let (chart, stats, densities, beats, last) =
        rssp::stats::minimize_rows_typed_in::<4>(data, scratch);
    let checksum = chart
        .len()
        .wrapping_add(stats.total_arrows as usize)
        .wrapping_add(densities.len())
        .wrapping_add(scratch.rows().len())
        .wrapping_add(beats.len())
        .wrapping_add(last.to_bits() as usize);
    black_box((
        chart.as_slice(),
        &stats,
        densities.as_slice(),
        scratch.rows(),
        beats.as_slice(),
        last,
    ));
    checksum
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
fn control_pair_map(entries: usize) -> String {
    use std::fmt::Write;

    let mut map = String::with_capacity(entries * 20);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        write!(&mut map, "\u{000b}{}={}\u{000b}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
    }
    map
}

#[cfg(windows)]
fn control_normalize_map(entries: usize) -> String {
    use std::fmt::Write;

    let mut map = String::with_capacity(entries * 20);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        write!(&mut map, "+\u{000b}{}=+\u{000b}{}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
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
fn transition_speed_map(entries: usize) -> String {
    use std::fmt::Write;

    let mut map = String::with_capacity(entries * 20);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        write!(
            &mut map,
            "{}={}={}={}",
            idx * 4,
            1 + idx % 7,
            1 + idx % 4,
            idx & 1
        )
        .unwrap();
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
#[allow(clippy::cast_precision_loss)]
fn growing_bpm_stats_reference(values: &[f64]) -> (f64, f64) {
    const SELECTION_MIN_VALUES: usize = 64;

    if values.is_empty() {
        return (0.0, 0.0);
    }
    let mut filtered: Vec<_> = values
        .iter()
        .copied()
        .filter(|&bpm| bpm > 0.0 && bpm < 10_000.0)
        .collect();
    let can_select = !filtered.is_empty();
    if !can_select {
        filtered.extend_from_slice(values);
    }
    if can_select && filtered.len() >= SELECTION_MIN_VALUES {
        let average = filtered.iter().sum::<f64>() / filtered.len() as f64;
        let mid = filtered.len() / 2;
        let even = mid * 2 == filtered.len();
        let (_, upper, _) = filtered.select_nth_unstable_by(mid, |a, b| {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        });
        let upper = *upper;
        let median = if even {
            let lower = filtered[..mid]
                .iter()
                .copied()
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(upper);
            f64::midpoint(lower, upper)
        } else {
            upper
        };
        return (median, average);
    }

    filtered.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if filtered.len().is_multiple_of(2) {
        f64::midpoint(
            filtered[filtered.len() / 2 - 1],
            filtered[filtered.len() / 2],
        )
    } else {
        filtered[filtered.len() / 2]
    };
    (median, filtered.iter().sum::<f64>() / filtered.len() as f64)
}

#[cfg(windows)]
fn bench_cycles(c: &mut Criterion<ThreadCycles>) {
    const ENTRIES: usize = 4_096;
    let cpu = platform::stabilize_thread();
    eprintln!("cycle_perf measurement=QueryThreadCycleTime logical_cpu={cpu}");

    let pair_map = large_pair_map(ENTRIES);
    let control_pair_map = control_pair_map(ENTRIES);
    let control_normalize_map = control_normalize_map(ENTRIES);
    let speed_map = large_speed_map(ENTRIES);
    let stop_map = large_stop_map(ENTRIES);
    let medium_pair_map = large_pair_map(512);
    let medium_stop_map = large_stop_map(256);
    let medium_speed_map = transition_speed_map(512);
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
    let bpm_stats_values: Vec<_> = bpm_stats_map.iter().map(|&(_, bpm)| bpm).collect();

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

    const DISPLAY_CASES: [(Option<&str>, f64, f64, f64); 4] = [
        (None, 120.0, 180.0, 1.0),
        (Some("150"), 120.0, 180.0, 1.0),
        (Some("120:180"), 120.0, 180.0, 1.25),
        (Some("*"), 90.0, 240.0, 1.1),
    ];
    let mut display_bpm = c.benchmark_group("cycles/display_bpm");
    display_bpm.throughput(Throughput::Elements(1_024));
    display_bpm.sample_size(100);
    display_bpm.measurement_time(Duration::from_secs(2));
    display_bpm.bench_function("mixed_1024", |b| {
        b.iter(|| {
            for _ in 0..256 {
                for (tag, min, max, rate) in DISPLAY_CASES {
                    black_box(rssp::bpm::resolve_display_bpm(
                        black_box(tag),
                        black_box(min),
                        black_box(max),
                        black_box(rate),
                    ));
                }
            }
        });
    });
    display_bpm.finish();

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
    normalization.throughput(Throughput::Bytes(control_pair_map.len() as u64));
    normalization.bench_function("control_pair_map", |b| {
        b.iter(|| {
            black_box(rssp::bpm::clean_timing_map(black_box(&control_pair_map)));
        });
    });
    normalization.throughput(Throughput::Bytes(control_normalize_map.len() as u64));
    normalization.bench_function("control_normalize_map", |b| {
        b.iter(|| {
            black_box(rssp::bpm::normalize_float_digits(black_box(
                &control_normalize_map,
            )));
        });
    });
    normalization.bench_function("control_fused_map", |b| {
        b.iter(|| {
            black_box(rssp::bpm::clean_and_normalize_float_digits(black_box(
                &control_normalize_map,
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
    normalization.bench_function("bpm_stats_slice", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_bpm_stats(black_box(&bpm_stats_values)));
        });
    });
    normalization.bench_function("bpm_stats_slice_growing_reference", |b| {
        b.iter(|| {
            black_box(growing_bpm_stats_reference(black_box(&bpm_stats_values)));
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
    cleanup.bench_function("ordered_ssc_bpms_stops", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                0.0,
                0.0,
                None,
                black_box(&pair_map),
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

    let mut timing_build = c.benchmark_group("cycles/timing_build_ssc");
    timing_build.throughput(Throughput::Elements(1_280));
    timing_build.sample_size(100);
    timing_build.measurement_time(Duration::from_secs(2));
    timing_build.bench_function("bpm_512_stops_256_speeds_512", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                0.0,
                0.0,
                None,
                black_box(&medium_pair_map),
                None,
                black_box(&medium_stop_map),
                None,
                "",
                None,
                "",
                None,
                black_box(&medium_speed_map),
                None,
                "",
                None,
                "",
                rssp::timing::TimingFormat::Ssc,
                true,
            ));
        });
    });
    timing_build.finish();

    sm_timing_bench::assert_behavior();
    let sm_timing_fixture = sm_timing_bench::SmTimingFixture::new();
    let mut sm_timing = c.benchmark_group("cycles/sm_timing_4096_bpms_2048_stops");
    sm_timing.throughput(Throughput::Elements(sm_timing_bench::INPUT_COUNT));
    sm_timing.sample_size(50);
    sm_timing.measurement_time(Duration::from_secs(3));
    sm_timing.bench_function("legacy_f32_then_f64", |b| {
        b.iter(|| {
            black_box(rssp::timing::process_sm_timing_for_bench(
                black_box(&sm_timing_fixture.bpms),
                black_box(&sm_timing_fixture.stops),
                true,
            ));
        });
    });
    sm_timing.bench_function("direct_f64", |b| {
        b.iter(|| {
            black_box(rssp::timing::process_sm_timing_for_bench(
                black_box(&sm_timing_fixture.bpms),
                black_box(&sm_timing_fixture.stops),
                false,
            ));
        });
    });
    sm_timing.finish();

    timing_merge_bench::assert_behavior();
    let timing_merge_fixture = timing_merge_bench::TimingMergeFixture::new();
    let mut timing_merge = c.benchmark_group("cycles/sm_stop_merge_2048_each");
    timing_merge.throughput(Throughput::Elements(timing_merge_bench::MERGE_INPUT_COUNT));
    timing_merge.sample_size(50);
    timing_merge.measurement_time(Duration::from_secs(3));
    timing_merge.bench_function("materialize_warps", |b| {
        b.iter(|| {
            black_box(timing_merge_bench::legacy_convert(
                black_box(&timing_merge_fixture.bpms),
                black_box(&timing_merge_fixture.stops),
                black_box(&timing_merge_fixture.delays),
                black_box(&timing_merge_fixture.warps),
            ));
        });
    });
    timing_merge.bench_function("fused_warps", |b| {
        b.iter(|| {
            black_box(rssp::timing::convert_warps_and_delays_to_sm_stops(
                black_box(&timing_merge_fixture.bpms),
                black_box(&timing_merge_fixture.stops),
                black_box(&timing_merge_fixture.delays),
                black_box(&timing_merge_fixture.warps),
            ));
        });
    });
    timing_merge.finish();

    let cursor_densities: Vec<_> = (0..512)
        .map(|idx| [0, 16, 20, 24, 32][(idx * 7) % 5])
        .collect();
    let cursor_timing = rssp::timing::timing_data_from_chart_data(
        0.0,
        0.0,
        None,
        &medium_pair_map,
        None,
        &medium_stop_map,
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
        rssp::timing::TimingFormat::Ssc,
        true,
    );
    let mut nps_cursor = c.benchmark_group("cycles/nps_timing_cursor");
    nps_cursor.throughput(Throughput::Elements(cursor_densities.len() as u64));
    nps_cursor.sample_size(100);
    nps_cursor.measurement_time(Duration::from_secs(2));
    nps_cursor.bench_function("measure_512", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_measure_nps_vec_with_timing(
                black_box(&cursor_densities),
                black_box(&cursor_timing),
            ));
        });
    });
    nps_cursor.finish();

    let bpm_cursor_densities: Vec<_> = (0..4_096)
        .map(|idx| [0, 16, 20, 24, 32][(idx * 7) % 5])
        .collect();
    let bpm_cursor_bpms: Vec<_> = (0..4_096)
        .map(|idx| (idx as f64 * 4.0, 60.0 + ((idx * 37) % 300) as f64))
        .collect();
    let mut nps_bpm_cursor = c.benchmark_group("cycles/nps_bpm_cursor");
    nps_bpm_cursor.throughput(Throughput::Elements(bpm_cursor_densities.len() as u64));
    nps_bpm_cursor.sample_size(100);
    nps_bpm_cursor.measurement_time(Duration::from_secs(2));
    nps_bpm_cursor.bench_function("measure_4096", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_measure_nps_vec(
                black_box(&bpm_cursor_densities),
                black_box(&bpm_cursor_bpms),
            ));
        });
    });
    nps_bpm_cursor.finish();

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
    let bpm_stats_map: Vec<_> = (0..4_096)
        .map(|index| {
            (
                index as f64 * 4.0,
                60.125 + ((index * 977) % 1_000) as f64 / 8.0,
            )
        })
        .collect();
    let mut bpm_stats_values = Vec::with_capacity(bpm_stats_map.len());
    let mut nps_scratch = Vec::new();
    let custom_patterns = custom_pattern_input(256);
    let course_chart_patterns: Vec<_> = custom_patterns
        .chunks_exact(3)
        .map(|patterns| rssp::patterns::CustomPatternSummary {
            pattern: patterns[0].clone(),
            count: 1,
        })
        .collect();
    const BATCH_FIXTURE: &[u8] = b"#VERSION:0.83;#TITLE:Batch;#BPMS:0=120;\
#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Challenge;#METER:10;\
#NOTES:\n1000\n0100\n0010\n0001\n;";
    let batch_options = rssp::AnalysisOptions {
        custom_patterns: custom_patterns.clone(),
        compute_tech_counts: false,
        ..rssp::AnalysisOptions::default()
    };
    let prepared_batch = rssp::PreparedAnalysis::new(batch_options.clone());
    let mut batch_scratch = rssp::AnalysisScratch::default();
    let mut prepared_scratch = rssp::AnalysisScratch::default();
    let analysis_fixture = include_bytes!("fixtures/camellia_mix.ssc");
    let nps_fixture = include_bytes!("fixtures/watch_yo_step.ssc");
    let minimize_parsed = rssp::parse::extract_sections(nps_fixture, "ssc")
        .expect("minimizer cycle fixture should parse");
    let minimize_chart = minimize_parsed
        .notes_list
        .iter()
        .filter_map(|entry| {
            Some((
                entry.note_data,
                rssp::supported_stepstype_lanes_bytes(entry.fields[0])?,
            ))
        })
        .max_by_key(|(data, _)| data.len())
        .expect("minimizer cycle fixture should contain a supported chart");
    let minimize_typed_chart = minimize_parsed
        .notes_list
        .iter()
        .filter(|entry| rssp::supported_stepstype_lanes_bytes(entry.fields[0]) == Some(4))
        .max_by_key(|entry| entry.note_data.len())
        .map(|entry| entry.note_data)
        .expect("minimizer cycle fixture should contain a 4-lane chart");
    let mut typed_rows = rssp::stats::TypedRowsScratch::<4>::default();
    rssp::stats::minimize_rows_typed_in::<4>(minimize_typed_chart, &mut typed_rows);
    let analysis_options = rssp::AnalysisOptions::default();
    let mut allocating_analysis_scratch = rssp::AnalysisScratch::default();
    let mut analysis_scratch = rssp::AnalysisScratch::default();
    let mut stream_tokens = Vec::new();
    let course_fixture = course_bench::CourseFixture::new();
    let banner_fixture = course_bench::BannerFixture::new();
    let resolve_fixture = course_bench::ResolveFixture::new();
    banner_fixture.assert_behavior();
    resolve_fixture.assert_behavior();
    course_bench::assert_step_norm_behavior();
    course_bench::assert_title_match_behavior();
    let course_input =
        std::fs::read(course_fixture.course_path()).expect("benchmark course should be readable");
    let legacy_course = rssp::course::profile_parse_crs(&course_input, true)
        .expect("legacy benchmark course should parse");
    let current_course = rssp::course::profile_parse_crs(&course_input, false)
        .expect("benchmark course should parse");
    assert_eq!(current_course.name, legacy_course.name);
    assert_eq!(current_course.name_translit, legacy_course.name_translit);
    assert_eq!(current_course.scripter, legacy_course.scripter);
    assert_eq!(current_course.description, legacy_course.description);
    assert_eq!(current_course.banner, legacy_course.banner);
    assert_eq!(current_course.background, legacy_course.background);
    assert_eq!(current_course.repeat, legacy_course.repeat);
    assert_eq!(current_course.lives, legacy_course.lives);
    assert_eq!(current_course.meters, legacy_course.meters);
    assert_eq!(current_course.entries, legacy_course.entries);
    let select_input = course_bench::select_input();
    let course_options = course_bench::fast_options();
    let pack_fixture = pack_bench::PackFixture::new();
    let pack_image_fixture = pack_bench::ImageHintFixture::new();
    pack_fixture.assert_root_behavior();
    pack_fixture.assert_song_behavior();
    pack_fixture.assert_tree_behavior();
    pack_fixture.assert_songs_behavior();
    pack_fixture.assert_parent_img_behavior();
    pack_image_fixture.assert_behavior();
    let asset_fixture = assets_bench::AssetFixture::with_movies(1);
    asset_fixture.assert_song_assets_behavior();
    asset_fixture.assert_music_behavior();
    let delimiter_fields = assets_bench::delimiter_fields();
    let delimiter_bytes = delimiter_fields.iter().map(String::len).sum::<usize>();
    let timing_text_fixture = report_timing_bench::timing_text();
    let legacy_timing_text = rssp::profile::timing_text(
        &timing_text_fixture.time_signatures,
        &timing_text_fixture.labels,
        &timing_text_fixture.tickcounts,
        &timing_text_fixture.combos,
        true,
    );
    let current_timing_text = rssp::profile::timing_text(
        &timing_text_fixture.time_signatures,
        &timing_text_fixture.labels,
        &timing_text_fixture.tickcounts,
        &timing_text_fixture.combos,
        false,
    );
    assert_eq!(current_timing_text, legacy_timing_text);
    let [time_signatures, labels, tickcounts, combos] = report_timing_bench::TIMING_TEXT_EDGE;
    assert_eq!(
        rssp::profile::timing_text(time_signatures, labels, tickcounts, combos, false),
        rssp::profile::timing_text(time_signatures, labels, tickcounts, combos, true),
        "timing text edge behavior must not change"
    );
    let serialize_fixture = serialize_bench::SerializeFixture::new();
    serialize_bench::assert_behavior(&serialize_fixture);
    let serialize_buffer_fixture = serialize_bench::BufferFixture::new();
    serialize_bench::assert_buffer_behavior(&serialize_buffer_fixture);
    let serialize_escape_fixture = serialize_bench::EscapeFixture::new();
    serialize_bench::assert_escape_behavior(&serialize_escape_fixture);
    let report_fixture = report_timing_bench::fixture();
    let report_summary = rssp::analyze(
        report_fixture.as_bytes(),
        "ssc",
        &report_timing_bench::options(),
    )
    .expect("timing JSON cycle fixture should analyze");
    let report_chart = report_summary
        .charts
        .first()
        .expect("timing JSON cycle fixture should contain a chart");
    let hash_bpms_fixture = report_timing_bench::chart_bpm_fixture();
    let hash_bpms_summary = rssp::analyze(
        hash_bpms_fixture.as_bytes(),
        "ssc",
        &report_timing_bench::options(),
    )
    .expect("hash BPM JSON cycle fixture should analyze");
    let mut legacy_hash_bpms = hash_bpms_summary.clone();
    legacy_hash_bpms
        .charts
        .first_mut()
        .expect("hash BPM cycle fixture should contain a chart")
        .chart_bpms_norm = None;
    let hash_bpms_chart = hash_bpms_summary
        .charts
        .first()
        .expect("hash BPM cycle fixture should contain a chart");
    let legacy_hash_bpms_chart = legacy_hash_bpms
        .charts
        .first()
        .expect("hash BPM cycle fixture should contain a legacy chart");
    let custom_report_summary = report_patterns_bench::summary();
    let custom_report_chart = custom_report_summary
        .charts
        .first()
        .expect("custom pattern cycle fixture should contain a chart");
    let custom_report_patterns = custom_report_summary
        .charts
        .iter()
        .map(|chart| chart.custom_patterns.len())
        .sum::<usize>();
    let nps_report_fixture = report_nps_bench::fixture();
    let nps_report_summary =
        rssp::analyze(&nps_report_fixture, "ssc", &report_nps_bench::options())
            .expect("NPS JSON cycle fixture should analyze");
    let nps_report_chart = nps_report_summary
        .charts
        .first()
        .expect("NPS JSON cycle fixture should contain a chart");

    let mut optimizations = c.benchmark_group("cycles/optimizations");
    optimizations.sample_size(100);
    optimizations.measurement_time(Duration::from_secs(2));
    const STEP_CASES: [(&str, &str); 8] = [
        ("dance-single", "dance-single"),
        (" DANCE_SINGLE ", "dance-single"),
        ("dance-double", "dance-single"),
        ("DANCE-SOLO", "dance-single"),
        ("pump_single", "pump-single"),
        ("lights-cabinet", "lights-cabinet"),
        ("kb7-single", "dance-single"),
        ("非ASCII-single", "dance-single"),
    ];
    const STEP_BATCH: usize = 512;
    optimizations.throughput(Throughput::Elements((STEP_CASES.len() * STEP_BATCH) as u64));
    optimizations.bench_function("stepstype_allocating", |b| {
        b.iter(|| {
            for _ in 0..STEP_BATCH {
                for (raw, normalized) in STEP_CASES {
                    black_box(rssp::course::profile_stepstype_eq_legacy(
                        black_box(raw),
                        black_box(normalized),
                    ));
                }
            }
        });
    });
    optimizations.bench_function("stepstype_bytes", |b| {
        b.iter(|| {
            for _ in 0..STEP_BATCH {
                for (raw, normalized) in STEP_CASES {
                    black_box(rssp::course::profile_stepstype_eq(
                        black_box(raw),
                        black_box(normalized),
                    ));
                }
            }
        });
    });
    optimizations.throughput(Throughput::Elements(
        (course_bench::STEP_NORM_CASES.len() * course_bench::STEP_NORM_BATCH) as u64,
    ));
    optimizations.bench_function("stepstype_normalize_two_owned", |b| {
        b.iter(|| {
            for _ in 0..course_bench::STEP_NORM_BATCH {
                for raw in course_bench::STEP_NORM_CASES {
                    black_box(rssp::course::profile_normalize_stepstype(
                        black_box(raw),
                        true,
                    ));
                }
            }
        });
    });
    optimizations.bench_function("stepstype_normalize_borrowed", |b| {
        b.iter(|| {
            for _ in 0..course_bench::STEP_NORM_BATCH {
                for raw in course_bench::STEP_NORM_CASES {
                    black_box(rssp::course::profile_normalize_stepstype(
                        black_box(raw),
                        false,
                    ));
                }
            }
        });
    });
    optimizations.throughput(Throughput::Elements(course_bench::TITLE_MATCH_BATCH as u64));
    optimizations.bench_function("title_match_owned", |b| {
        b.iter(|| {
            for _ in 0..course_bench::TITLE_MATCH_BATCH {
                black_box(rssp::course::profile_simfile_title_eq(
                    black_box(course_bench::TITLE_MATCH_INPUT),
                    black_box("ssc"),
                    black_box(course_bench::TITLE_MATCH_EXPECTED),
                    true,
                ));
            }
        });
    });
    optimizations.bench_function("title_match_borrowed", |b| {
        b.iter(|| {
            for _ in 0..course_bench::TITLE_MATCH_BATCH {
                black_box(rssp::course::profile_simfile_title_eq(
                    black_box(course_bench::TITLE_MATCH_INPUT),
                    black_box("ssc"),
                    black_box(course_bench::TITLE_MATCH_EXPECTED),
                    false,
                ));
            }
        });
    });
    pack_bench::assert_hint_norm_behavior();
    optimizations.throughput(Throughput::Elements(pack_bench::HINT_NORM_BATCH as u64));
    for (name, legacy) in [
        ("pack_hint_normalize_owned", true),
        ("pack_hint_normalize_borrowed", false),
    ] {
        optimizations.bench_function(name, |b| {
            b.iter(|| {
                for _ in 0..pack_bench::HINT_NORM_BATCH {
                    black_box(rssp::pack::profile_normalized_img_hint(
                        black_box(pack_bench::HINT_NORM_INPUT),
                        legacy,
                    ));
                }
            });
        });
    }
    translate_bench::assert_behavior();
    let marker_input = translate_bench::alias_input();
    optimizations.throughput(Throughput::Elements(translate_bench::MARKER_COUNT as u64));
    for (name, legacy) in [
        ("marker_translate_allocating", true),
        ("marker_translate_compact", false),
    ] {
        optimizations.bench_function(name, |b| {
            b.iter_batched(
                || marker_input.clone(),
                |mut input| {
                    rssp::translate::profile_replace_markers(black_box(&mut input), legacy);
                    black_box(input);
                },
                BatchSize::SmallInput,
            );
        });
    }
    optimizations.throughput(Throughput::Elements(stream_densities.len() as u64));
    optimizations.bench_function("stream_outputs", |b| {
        b.iter(|| {
            black_box(rssp::stats::compute_stream_outputs(black_box(
                &stream_densities,
            )));
        });
    });
    optimizations.bench_function("stream_outputs_reused", |b| {
        b.iter(|| {
            black_box(rssp::stats::compute_stream_outputs_with_scratch(
                black_box(&stream_densities),
                black_box(&mut stream_tokens),
            ));
        });
    });
    optimizations.throughput(Throughput::Elements(matrix_densities.len() as u64));
    optimizations.bench_function("matrix_profile_build_legacy", |b| {
        b.iter(|| {
            black_box(rssp::matrix::compute_matrix_profile_legacy_for_bench(
                black_box(&matrix_densities),
                black_box(&matrix_bpms),
            ));
        });
    });
    optimizations.bench_function("matrix_profile_build_reserved", |b| {
        b.iter(|| {
            black_box(rssp::matrix::compute_matrix_profile_reserved_for_bench(
                black_box(&matrix_densities),
                black_box(&matrix_bpms),
            ));
        });
    });
    optimizations.bench_function("matrix_profile_build_optimized", |b| {
        b.iter(|| {
            black_box(rssp::matrix::compute_matrix_profile(
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
    optimizations.throughput(Throughput::Elements(custom_patterns.len() as u64));
    optimizations.bench_function("custom_patterns_legacy", |b| {
        b.iter(|| {
            black_box(rssp::patterns::compile_custom_patterns_legacy_for_bench(
                black_box(&custom_patterns),
            ));
        });
    });
    optimizations.bench_function("custom_patterns_open_addressed", |b| {
        b.iter(|| {
            black_box(rssp::patterns::compile_custom_patterns(black_box(
                &custom_patterns,
            )));
        });
    });
    optimizations.throughput(Throughput::Elements(1));
    optimizations.bench_function("custom_patterns_rebuild_file", |b| {
        b.iter(|| {
            black_box(
                rssp::analyze_with_scratch(
                    black_box(BATCH_FIXTURE),
                    "ssc",
                    black_box(&batch_options),
                    black_box(&mut batch_scratch),
                )
                .expect("batch fixture should analyze"),
            );
        });
    });
    optimizations.bench_function("custom_patterns_prepared_file", |b| {
        b.iter(|| {
            black_box(
                rssp::analyze_prepared_in(
                    black_box(BATCH_FIXTURE),
                    "ssc",
                    black_box(&prepared_batch),
                    black_box(&mut prepared_scratch),
                )
                .expect("batch fixture should analyze"),
            );
        });
    });
    const COURSE_CHARTS: usize = 64;
    optimizations.throughput(Throughput::Elements(
        (course_chart_patterns.len() * COURSE_CHARTS) as u64,
    ));
    optimizations.bench_function("course_patterns_linear_find_sort", |b| {
        b.iter(|| {
            let mut total = Vec::new();
            for _ in 0..COURSE_CHARTS {
                rssp::profile::merge_course_patterns_legacy(
                    black_box(&mut total),
                    black_box(&course_chart_patterns),
                );
            }
            black_box(total);
        });
    });
    optimizations.bench_function("course_patterns_binary_insert", |b| {
        b.iter(|| {
            let mut total = Vec::new();
            for _ in 0..COURSE_CHARTS {
                rssp::profile::merge_course_patterns(
                    black_box(&mut total),
                    black_box(&course_chart_patterns),
                );
            }
            black_box(total);
        });
    });
    optimizations.throughput(Throughput::Bytes(nps_fixture.len() as u64));
    optimizations.bench_function("peak_nps_materialized", |b| {
        b.iter(|| {
            black_box(
                rssp::nps::compute_chart_peak_nps_legacy_for_bench(
                    black_box(nps_fixture),
                    black_box("ssc"),
                )
                .expect("fixture should analyze"),
            );
        });
    });
    optimizations.bench_function("peak_nps_reused", |b| {
        b.iter(|| {
            black_box(
                rssp::compute_chart_peak_nps(black_box(nps_fixture), black_box("ssc"))
                    .expect("fixture should analyze"),
            );
        });
    });
    const MINIMIZE_BATCH: usize = 128;
    optimizations.throughput(Throughput::Bytes(
        (minimize_chart.0.len() * MINIMIZE_BATCH) as u64,
    ));
    optimizations.bench_function("minimize_materialized", |b| {
        b.iter(|| {
            for _ in 0..MINIMIZE_BATCH {
                black_box(rssp::stats::minimize_chart_count_rows_legacy_for_bench(
                    black_box(minimize_chart.0),
                    black_box(minimize_chart.1),
                ));
            }
        });
    });
    optimizations.bench_function("minimize_output_backed", |b| {
        b.iter(|| {
            for _ in 0..MINIMIZE_BATCH {
                black_box(rssp::stats::minimize_chart_count_rows(
                    black_box(minimize_chart.0),
                    black_box(minimize_chart.1),
                ));
            }
        });
    });
    optimizations.throughput(Throughput::Bytes(
        (minimize_typed_chart.len() * MINIMIZE_BATCH) as u64,
    ));
    optimizations.bench_function("typed_rows_owned", |b| {
        b.iter(|| {
            for _ in 0..MINIMIZE_BATCH {
                black_box(typed_rows_owned(black_box(minimize_typed_chart)));
            }
        });
    });
    optimizations.bench_function("typed_rows_reused", |b| {
        b.iter(|| {
            for _ in 0..MINIMIZE_BATCH {
                black_box(typed_rows_reused(
                    black_box(minimize_typed_chart),
                    &mut typed_rows,
                ));
            }
        });
    });
    optimizations.finish();

    nps_stats_bench::assert_behavior();
    let owned_nps = nps_stats_bench::values();
    let mut owned_nps_stats = c.benchmark_group("cycles/nps_stats_owned_16385");
    owned_nps_stats.sample_size(50);
    owned_nps_stats.measurement_time(Duration::from_secs(3));
    owned_nps_stats.throughput(Throughput::Elements(nps_stats_bench::VALUE_COUNT));
    owned_nps_stats.bench_function("copy_to_scratch", |b| {
        b.iter(|| black_box(rssp::bpm::get_nps_stats(black_box(&owned_nps))));
    });
    owned_nps_stats.bench_function("select_in_place", |b| {
        b.iter_batched(
            || owned_nps.clone(),
            |mut values| {
                black_box(rssp::bpm::get_nps_stats_in_place(black_box(&mut values)));
            },
            BatchSize::SmallInput,
        );
    });
    owned_nps_stats.finish();

    let mut bpm_stats = c.benchmark_group("cycles/bpm_range_stats");
    bpm_stats.sample_size(20);
    bpm_stats.measurement_time(Duration::from_secs(3));
    bpm_stats.throughput(Throughput::Elements(bpm_stats_map.len() as u64));
    bpm_stats.bench_function("allocating", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_bpm_range_and_stats(black_box(
                &bpm_stats_map,
            )))
        });
    });
    bpm_stats.bench_function("reused", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_bpm_range_and_stats_with_scratch(
                black_box(&bpm_stats_map),
                black_box(&mut bpm_stats_values),
            ))
        });
    });
    bpm_stats.finish();

    let elapsed_fixture = elapsed_bench::ElapsedFixture::new();
    elapsed_bench::assert_behavior(&elapsed_fixture);
    let mut elapsed_events = c.benchmark_group("cycles/elapsed_events_512");
    elapsed_events.sample_size(100);
    elapsed_events.measurement_time(Duration::from_secs(3));
    elapsed_events.throughput(Throughput::Elements(elapsed_bench::EVENT_COUNT));
    for (name, legacy) in [("collect_sort", true), ("stable_merge", false)] {
        elapsed_events.bench_function(name, |b| {
            b.iter(|| {
                black_box(rssp::bpm::get_elapsed_time_for_bench(
                    black_box(elapsed_fixture.target),
                    black_box(&elapsed_fixture.bpms),
                    black_box(&elapsed_fixture.stops),
                    black_box(&elapsed_fixture.delays),
                    black_box(&elapsed_fixture.warps),
                    legacy,
                ));
            });
        });
    }
    elapsed_events.finish();

    let mut course_parse = c.benchmark_group("cycles/course_parse");
    course_parse.sample_size(50);
    course_parse.measurement_time(Duration::from_secs(3));
    course_parse.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    course_parse.bench_function("legacy_control_allocs", |b| {
        b.iter(|| {
            black_box(
                rssp::course::profile_parse_crs(black_box(&course_input), true)
                    .expect("benchmark course should parse"),
            );
        });
    });
    course_parse.bench_function("stream_control_fields", |b| {
        b.iter(|| {
            black_box(
                rssp::course::profile_parse_crs(black_box(&course_input), false)
                    .expect("benchmark course should parse"),
            );
        });
    });
    course_parse.finish();

    let mut course_mods = c.benchmark_group("cycles/course_song_mods");
    course_mods.sample_size(100);
    course_mods.measurement_time(Duration::from_secs(3));
    course_mods.throughput(Throughput::Elements(course_bench::MOD_COUNT));
    course_mods.bench_function("apply", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_song_mods(
                black_box(true),
                black_box(course_bench::MODS),
            ));
        });
    });
    course_mods.finish();

    let mut select_mods = c.benchmark_group("cycles/course_select_mods");
    select_mods.sample_size(100);
    select_mods.measurement_time(Duration::from_secs(3));
    select_mods.throughput(Throughput::Elements(course_bench::SELECT_MOD_COUNT));
    select_mods.bench_function("apply", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_select_mods(black_box(
                course_bench::SELECT_MODS,
            )));
        });
    });
    select_mods.finish();

    let mut select_parse = c.benchmark_group("cycles/course_select_parse");
    select_parse.sample_size(50);
    select_parse.measurement_time(Duration::from_secs(3));
    select_parse.throughput(Throughput::Elements(
        course_bench::SELECT_COUNT as u64 * course_bench::SELECT_PARAMS,
    ));
    select_parse.bench_function("parse_64", |b| {
        b.iter(|| {
            black_box(
                rssp::course::parse_crs(black_box(&select_input))
                    .expect("selection benchmark should parse"),
            );
        });
    });
    select_parse.finish();

    let mut course = c.benchmark_group("cycles/course_cache");
    course.sample_size(20);
    course.measurement_time(Duration::from_secs(3));
    course.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    course.bench_function("cache_all", |b| {
        b.iter(|| {
            black_box(
                rssp::course::analyze_crs_path_cache_all_for_bench(
                    black_box(course_fixture.course_path()),
                    Some(black_box(course_fixture.songs_dir())),
                    black_box("dance-single"),
                    black_box("Medium"),
                    black_box(course_options.clone()),
                )
                .expect("benchmark course should analyze"),
            );
        });
    });
    course.bench_function("cache_repeated", |b| {
        b.iter(|| {
            black_box(
                rssp::course::analyze_crs_path(
                    black_box(course_fixture.course_path()),
                    Some(black_box(course_fixture.songs_dir())),
                    black_box("dance-single"),
                    black_box("Medium"),
                    black_box(course_options.clone()),
                )
                .expect("benchmark course should analyze"),
            );
        });
    });
    course.finish();

    let mut course_banner = c.benchmark_group("cycles/course_banner_258");
    course_banner.sample_size(20);
    course_banner.measurement_time(Duration::from_secs(3));
    course_banner.throughput(Throughput::Elements(
        course_bench::BANNER_ENTRY_COUNT as u64,
    ));
    course_banner.bench_function("legacy_five_scans", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_course_banner(
                black_box(banner_fixture.course_path()),
                black_box(""),
                true,
            ))
        });
    });
    course_banner.bench_function("one_scan_full_path_stats", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_course_banner_full_paths(
                black_box(banner_fixture.course_path()),
                black_box(""),
            ))
        });
    });
    course_banner.bench_function("one_scan_entry_types", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_course_banner(
                black_box(banner_fixture.course_path()),
                black_box(""),
                false,
            ))
        });
    });
    course_banner.finish();

    let mut course_resolve = c.benchmark_group("cycles/course_song_resolve_384");
    course_resolve.sample_size(20);
    course_resolve.measurement_time(Duration::from_secs(3));
    course_resolve.throughput(Throughput::Elements(
        course_bench::RESOLVE_ENTRY_COUNT as u64,
    ));
    course_resolve.bench_function("full_paths_metadata_keys", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_resolve_song_dir(
                black_box(resolve_fixture.songs_dir()),
                None,
                black_box(course_bench::RESOLVE_SONG),
                true,
            ))
        });
    });
    course_resolve.bench_function("entry_types_names", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_resolve_song_dir(
                black_box(resolve_fixture.songs_dir()),
                None,
                black_box(course_bench::RESOLVE_SONG),
                false,
            ))
        });
    });
    course_resolve.finish();

    let mut pack = c.benchmark_group("cycles/pack_root_discovery");
    pack.sample_size(20);
    pack.measurement_time(Duration::from_secs(3));
    pack.throughput(Throughput::Elements(pack_bench::ROOT_ENTRY_COUNT as u64));
    pack.bench_function("legacy_repeated_scans", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::pack_root_legacy(
                    black_box(pack_fixture.pack_dir()),
                    black_box(rssp::pack::ScanOpt::default()),
                    black_box(pack_bench::BANNER_HINT),
                    black_box(pack_bench::BACKGROUND_HINT),
                )
                .expect("benchmark pack root should scan"),
            )
        });
    });
    pack.bench_function("full_path_stats", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::pack_root_full_paths(
                    black_box(pack_fixture.pack_dir()),
                    black_box(rssp::pack::ScanOpt::default()),
                    black_box(pack_bench::BANNER_HINT),
                    black_box(pack_bench::BACKGROUND_HINT),
                )
                .expect("benchmark pack root should scan"),
            )
        });
    });
    pack.bench_function("cached_entry_types", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::pack_root(
                    black_box(pack_fixture.pack_dir()),
                    black_box(rssp::pack::ScanOpt::default()),
                    black_box(pack_bench::BANNER_HINT),
                    black_box(pack_bench::BACKGROUND_HINT),
                )
                .expect("benchmark pack root should scan"),
            )
        });
    });
    pack.finish();

    let mut songs_root = c.benchmark_group("cycles/songs_root_discovery");
    songs_root.sample_size(20);
    songs_root.measurement_time(Duration::from_secs(3));
    songs_root.throughput(Throughput::Elements(
        pack_bench::SONGS_ROOT_ENTRY_COUNT as u64,
    ));
    songs_root.bench_function("probe_every_entry", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::scan_songs_dir_legacy(
                    black_box(pack_fixture.tree_root()),
                    black_box(rssp::pack::ScanOpt::default()),
                )
                .expect("benchmark Songs root should scan"),
            )
        });
    });
    songs_root.bench_function("cached_dir_types", |b| {
        b.iter(|| {
            black_box(
                rssp::pack::scan_songs_dir(
                    black_box(pack_fixture.tree_root()),
                    black_box(rssp::pack::ScanOpt::default()),
                )
                .expect("benchmark Songs root should scan"),
            )
        });
    });
    songs_root.finish();

    let mut parent_img = c.benchmark_group("cycles/pack_parent_image");
    parent_img.sample_size(20);
    parent_img.measurement_time(Duration::from_secs(3));
    parent_img.throughput(Throughput::Elements(
        pack_bench::SONGS_ROOT_ENTRY_COUNT as u64,
    ));
    parent_img.bench_function("full_path_stats", |b| {
        b.iter(|| {
            black_box(rssp::profile::pack_parent_img_legacy(
                black_box(pack_fixture.pack_dir()),
                black_box("Performance Pack"),
            ))
        });
    });
    parent_img.bench_function("candidate_names", |b| {
        b.iter(|| {
            black_box(rssp::profile::pack_parent_img(
                black_box(pack_fixture.pack_dir()),
                black_box("Performance Pack"),
            ))
        });
    });
    parent_img.finish();

    let mut subdir_img = c.benchmark_group("cycles/pack_subdir_image");
    subdir_img.sample_size(20);
    subdir_img.measurement_time(Duration::from_secs(3));
    subdir_img.throughput(Throughput::Elements(pack_bench::HINT_ENTRY_COUNT as u64));
    subdir_img.bench_function("full_paths", |b| {
        b.iter(|| {
            black_box(rssp::profile::pack_subdir_img_legacy(
                black_box(pack_image_fixture.pack_dir()),
                black_box(pack_bench::SUBDIR_HINT),
            ))
        });
    });
    subdir_img.bench_function("candidate_names", |b| {
        b.iter(|| {
            black_box(rssp::profile::pack_subdir_img(
                black_box(pack_image_fixture.pack_dir()),
                black_box(pack_bench::SUBDIR_HINT),
            ))
        });
    });
    subdir_img.finish();

    let mut song_scan = c.benchmark_group("cycles/song_simfile_discovery");
    song_scan.sample_size(20);
    song_scan.measurement_time(Duration::from_secs(3));
    song_scan.throughput(Throughput::Elements(pack_bench::SONG_ENTRY_COUNT as u64));
    song_scan.bench_function("full_paths", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::scan_song_dir_full_paths(
                    black_box(pack_fixture.song_dir()),
                    black_box(rssp::pack::ScanOpt::default()),
                )
                .expect("benchmark song should scan"),
            )
        });
    });
    song_scan.bench_function("candidate_names", |b| {
        b.iter(|| {
            black_box(
                rssp::pack::scan_song_dir(
                    black_box(pack_fixture.song_dir()),
                    black_box(rssp::pack::ScanOpt::default()),
                )
                .expect("benchmark song should scan"),
            )
        });
    });
    song_scan.finish();

    let mut simfile_tree = c.benchmark_group("cycles/simfile_tree_discovery");
    simfile_tree.sample_size(20);
    simfile_tree.measurement_time(Duration::from_secs(3));
    simfile_tree.throughput(Throughput::Elements(pack_bench::TREE_ENTRY_COUNT as u64));
    simfile_tree.bench_function("rescan_subdirs", |b| {
        b.iter(|| {
            black_box(rssp::profile::find_simfiles_legacy(
                black_box(pack_fixture.tree_root()),
                black_box(rssp::pack::ScanOpt::default()),
            ))
        });
    });
    simfile_tree.bench_function("one_snapshot", |b| {
        b.iter(|| {
            black_box(rssp::pack::find_simfiles(
                black_box(pack_fixture.tree_root()),
                black_box(rssp::pack::ScanOpt::default()),
            ))
        });
    });
    simfile_tree.finish();

    let mut background = c.benchmark_group("cycles/background_changes");
    background.sample_size(20);
    background.measurement_time(Duration::from_secs(3));
    background.throughput(Throughput::Elements(assets_bench::CHANGE_COUNT as u64));
    background.bench_function("root_rescan", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_legacy(
                black_box(asset_fixture.song_dir()),
                black_box(asset_fixture.simfile()),
            ))
        });
    });
    background.bench_function("double_find", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_double_find(
                black_box(asset_fixture.song_dir()),
                black_box(asset_fixture.simfile()),
            ))
        });
    });
    background.bench_function("catalog_movie", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_background_changes_like_itg(
                black_box(asset_fixture.song_dir()),
                black_box(asset_fixture.simfile()),
            ))
        });
    });
    background.finish();

    let mut music = c.benchmark_group("cycles/music_fallback");
    music.sample_size(20);
    music.measurement_time(Duration::from_secs(3));
    music.throughput(Throughput::Elements(assets_bench::SOUND_COUNT as u64));
    music.bench_function("full_paths", |b| {
        b.iter(|| {
            black_box(rssp::profile::music_path_legacy(
                black_box(asset_fixture.song_dir()),
                black_box(""),
            ))
        });
    });
    music.bench_function("candidate_names", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_music_path_like_itg(
                black_box(asset_fixture.song_dir()),
                black_box(""),
            ))
        });
    });
    music.finish();

    let mut song_assets = c.benchmark_group("cycles/song_assets");
    song_assets.sample_size(20);
    song_assets.measurement_time(Duration::from_secs(3));
    song_assets.throughput(Throughput::Elements(
        (assets_bench::IMAGE_COUNT + assets_bench::NON_IMAGE_COUNT) as u64,
    ));
    song_assets.bench_function("full_candidate_paths", |b| {
        b.iter(|| {
            black_box(rssp::profile::song_assets_legacy(
                black_box(asset_fixture.image_dir()),
                black_box(""),
                black_box(""),
            ))
        });
    });
    song_assets.bench_function("candidate_names", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_song_assets(
                black_box(asset_fixture.image_dir()),
                black_box(""),
                black_box(""),
            ))
        });
    });
    song_assets.finish();

    let mut delimiter = c.benchmark_group("cycles/background_delimiter_scan");
    delimiter.throughput(Throughput::Bytes(delimiter_bytes as u64));
    delimiter.bench_function("double_find", |b| {
        b.iter(|| {
            let sum = delimiter_fields
                .iter()
                .filter_map(|field| rssp::profile::bg_delimiter_legacy(black_box(field)))
                .sum::<usize>();
            black_box(sum)
        });
    });
    delimiter.bench_function("memchr2", |b| {
        b.iter(|| {
            let sum = delimiter_fields
                .iter()
                .filter_map(|field| rssp::profile::bg_delimiter(black_box(field)))
                .sum::<usize>();
            black_box(sum)
        });
    });
    delimiter.finish();

    let native_bpms = &report_chart.timing_segments.bpms;
    let mut bpm_format = c.benchmark_group("cycles/native_bpm_format");
    bpm_format.sample_size(20);
    bpm_format.measurement_time(Duration::from_secs(3));
    bpm_format.throughput(Throughput::Elements(native_bpms.len() as u64));
    bpm_format.bench_function("materialized", |b| {
        b.iter(|| {
            black_box(rssp::timing::format_bpm_segments_f32_like_itg(black_box(
                native_bpms,
            )))
        });
    });
    bpm_format.bench_function("streamed", |b| {
        let mut output = String::with_capacity(native_bpms.len() * 24);
        b.iter(|| {
            output.clear();
            write!(
                &mut output,
                "{}",
                rssp::timing::native_bpms_display(black_box(native_bpms))
            )
            .expect("BPM display should write to String");
            black_box(output.len())
        });
    });
    bpm_format.finish();

    let mut bpm_text = c.benchmark_group("cycles/report_json_bpm_text");
    bpm_text.sample_size(20);
    bpm_text.measurement_time(Duration::from_secs(3));
    bpm_text.throughput(Throughput::Elements(
        report_timing_bench::SEGMENT_COUNT as u64,
    ));
    bpm_text.bench_function("materialized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_bpm_text_materialized(
                black_box(&mut output),
                black_box(report_chart),
                black_box(&report_summary),
            )
            .expect("materialized BPM text JSON should write");
            black_box(output)
        });
    });
    bpm_text.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_bpm_text(
                black_box(&mut output),
                black_box(report_chart),
                black_box(&report_summary),
            )
            .expect("streamed BPM text JSON should write");
            black_box(output)
        });
    });
    bpm_text.finish();

    let mut bpm_text_full = c.benchmark_group("cycles/report_json_bpm_text_full");
    bpm_text_full.sample_size(20);
    bpm_text_full.measurement_time(Duration::from_secs(3));
    bpm_text_full.throughput(Throughput::Elements(
        report_timing_bench::SEGMENT_COUNT as u64,
    ));
    bpm_text_full.bench_function("materialized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_bpm_text_report_materialized(
                black_box(&report_summary),
                black_box(&mut output),
            )
            .expect("materialized BPM text JSON report should write");
            black_box(output)
        });
    });
    bpm_text_full.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&report_summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("streamed BPM text JSON report should write");
            black_box(output)
        });
    });
    bpm_text_full.finish();

    let mut hash_bpms = c.benchmark_group("cycles/report_json_hash_bpms");
    hash_bpms.sample_size(20);
    hash_bpms.measurement_time(Duration::from_secs(3));
    hash_bpms.throughput(Throughput::Elements(
        report_timing_bench::SEGMENT_COUNT as u64,
    ));
    hash_bpms.bench_function("renormalized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_timing(
                black_box(&mut output),
                black_box(legacy_hash_bpms_chart),
                black_box(&legacy_hash_bpms),
            )
            .expect("renormalized hash BPM JSON should write");
            black_box(output)
        });
    });
    hash_bpms.bench_function("precomputed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_timing(
                black_box(&mut output),
                black_box(hash_bpms_chart),
                black_box(&hash_bpms_summary),
            )
            .expect("precomputed hash BPM JSON should write");
            black_box(output)
        });
    });
    hash_bpms.finish();

    let mut hash_bpms_full = c.benchmark_group("cycles/report_json_hash_bpms_full");
    hash_bpms_full.sample_size(20);
    hash_bpms_full.measurement_time(Duration::from_secs(3));
    hash_bpms_full.throughput(Throughput::Elements(
        report_timing_bench::SEGMENT_COUNT as u64,
    ));
    hash_bpms_full.bench_function("renormalized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&legacy_hash_bpms),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("renormalized hash BPM report should write");
            black_box(output)
        });
    });
    hash_bpms_full.bench_function("precomputed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&hash_bpms_summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("precomputed hash BPM report should write");
            black_box(output)
        });
    });
    hash_bpms_full.finish();

    let mut custom_patterns = c.benchmark_group("cycles/report_json_custom_patterns");
    custom_patterns.sample_size(20);
    custom_patterns.measurement_time(Duration::from_secs(3));
    custom_patterns.throughput(Throughput::Elements(
        custom_report_chart.custom_patterns.len() as u64,
    ));
    custom_patterns.bench_function("materialized_map", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_custom_patterns_materialized(
                black_box(&mut output),
                black_box(custom_report_chart),
            )
            .expect("materialized custom pattern JSON should write");
            black_box(output)
        });
    });
    custom_patterns.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_custom_patterns(
                black_box(&mut output),
                black_box(custom_report_chart),
            )
            .expect("streamed custom pattern JSON should write");
            black_box(output)
        });
    });
    custom_patterns.finish();

    let mut custom_patterns_full = c.benchmark_group("cycles/report_json_custom_patterns_full");
    custom_patterns_full.sample_size(20);
    custom_patterns_full.measurement_time(Duration::from_secs(3));
    custom_patterns_full.throughput(Throughput::Elements(custom_report_patterns as u64));
    custom_patterns_full.bench_function("materialized_map", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_custom_report_materialized(
                black_box(&custom_report_summary),
                black_box(&mut output),
            )
            .expect("materialized custom pattern report should write");
            black_box(output)
        });
    });
    custom_patterns_full.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&custom_report_summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("streamed custom pattern report should write");
            black_box(output)
        });
    });
    custom_patterns_full.finish();

    let mut timing_text = c.benchmark_group("cycles/timing_text_2048");
    timing_text.sample_size(100);
    timing_text.measurement_time(Duration::from_secs(3));
    timing_text.throughput(Throughput::Elements(
        (report_timing_bench::SEGMENT_COUNT * 4) as u64,
    ));
    timing_text.bench_function("legacy_staged", |b| {
        b.iter(|| {
            black_box(rssp::profile::timing_text(
                black_box(&timing_text_fixture.time_signatures),
                black_box(&timing_text_fixture.labels),
                black_box(&timing_text_fixture.tickcounts),
                black_box(&timing_text_fixture.combos),
                true,
            ))
        });
    });
    timing_text.bench_function("streamed_presized", |b| {
        b.iter(|| {
            black_box(rssp::profile::timing_text(
                black_box(&timing_text_fixture.time_signatures),
                black_box(&timing_text_fixture.labels),
                black_box(&timing_text_fixture.tickcounts),
                black_box(&timing_text_fixture.combos),
                false,
            ))
        });
    });
    timing_text.finish();

    let mut serialize = c.benchmark_group("cycles/serialize_ssc_3584_timing_segments");
    serialize.sample_size(50);
    serialize.measurement_time(Duration::from_secs(3));
    serialize.throughput(Throughput::Bytes(serialize_fixture.output_len as u64));
    serialize.bench_function("temporary_strings", |b| {
        let mut output = Vec::with_capacity(serialize_fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write(
                black_box(&serialize_fixture.summary),
                black_box(&mut output),
                true,
            ));
            black_box(&output);
        });
    });
    serialize.bench_function("direct_writer", |b| {
        let mut output = Vec::with_capacity(serialize_fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write(
                black_box(&serialize_fixture.summary),
                black_box(&mut output),
                false,
            ));
            black_box(&output);
        });
    });
    serialize.finish();

    let mut serialize_buffer = c.benchmark_group("cycles/serialize_stack_buffer");
    serialize_buffer.sample_size(50);
    serialize_buffer.measurement_time(Duration::from_secs(3));
    serialize_buffer.throughput(Throughput::Bytes(
        serialize_buffer_fixture.output_len as u64,
    ));
    serialize_buffer.bench_function("unbuffered", |b| {
        let mut output = Vec::with_capacity(serialize_buffer_fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_buffered(
                black_box(&serialize_buffer_fixture.summary),
                black_box(&mut output),
                true,
            ));
            black_box(&output);
        });
    });
    serialize_buffer.bench_function("stack_buffered", |b| {
        let mut output = Vec::with_capacity(serialize_buffer_fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_buffered(
                black_box(&serialize_buffer_fixture.summary),
                black_box(&mut output),
                false,
            ));
            black_box(&output);
        });
    });
    serialize_buffer.finish();

    let mut serialize_escape = c.benchmark_group("cycles/serialize_escape_metadata");
    serialize_escape.sample_size(50);
    serialize_escape.measurement_time(Duration::from_secs(3));
    serialize_escape.throughput(Throughput::Bytes(
        serialize_escape_fixture.output_len as u64,
    ));
    serialize_escape.bench_function("byte_at_a_time", |b| {
        let mut output = Vec::with_capacity(serialize_escape_fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_escape(
                black_box(&serialize_escape_fixture.summary),
                black_box(&mut output),
                true,
            ));
            black_box(&output);
        });
    });
    serialize_escape.bench_function("batched_spans", |b| {
        let mut output = Vec::with_capacity(serialize_escape_fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_escape(
                black_box(&serialize_escape_fixture.summary),
                black_box(&mut output),
                false,
            ));
            black_box(&output);
        });
    });
    serialize_escape.finish();

    let escape_field = serialize_escape_fixture.summary.title_str.as_bytes();
    let mut sm_escape = c.benchmark_group("cycles/sm_escape_metadata");
    sm_escape.sample_size(50);
    sm_escape.measurement_time(Duration::from_secs(3));
    sm_escape.throughput(Throughput::Bytes(escape_field.len() as u64));
    sm_escape.bench_function("byte_at_a_time", |b| {
        let mut output = Vec::with_capacity(serialize_escape_fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_escape_field(
                black_box(escape_field),
                black_box(&mut output),
                true,
            ));
            black_box(&output);
        });
    });
    sm_escape.bench_function("batched_spans", |b| {
        let mut output = Vec::with_capacity(serialize_escape_fixture.output_len);
        b.iter(|| {
            output.clear();
            black_box(serialize_bench::write_escape_field(
                black_box(escape_field),
                black_box(&mut output),
                false,
            ));
            black_box(&output);
        });
    });
    sm_escape.finish();

    let mut timing_arrays = c.benchmark_group("cycles/report_json_timing_arrays");
    timing_arrays.sample_size(20);
    timing_arrays.measurement_time(Duration::from_secs(3));
    timing_arrays.throughput(Throughput::Elements(
        report_timing_bench::SEGMENT_COUNT as u64,
    ));
    timing_arrays.bench_function("materialized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_timing_materialized(
                black_box(&mut output),
                black_box(report_chart),
                black_box(&report_summary),
            )
            .expect("materialized timing JSON should write");
            black_box(output)
        });
    });
    timing_arrays.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_timing(
                black_box(&mut output),
                black_box(report_chart),
                black_box(&report_summary),
            )
            .expect("streamed timing JSON should write");
            black_box(output)
        });
    });
    timing_arrays.finish();

    let mut timing_report = c.benchmark_group("cycles/report_json_timing");
    timing_report.sample_size(20);
    timing_report.measurement_time(Duration::from_secs(3));
    timing_report.throughput(Throughput::Elements(
        report_timing_bench::SEGMENT_COUNT as u64,
    ));
    timing_report.bench_function("materialized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_materialized(
                black_box(&report_summary),
                black_box(&mut output),
            )
            .expect("materialized timing JSON report should write");
            black_box(output)
        });
    });
    timing_report.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&report_summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("streamed timing JSON report should write");
            black_box(output)
        });
    });
    timing_report.finish();

    let mut nps_spacing = c.benchmark_group("cycles/report_json_nps_spacing");
    nps_spacing.sample_size(20);
    nps_spacing.measurement_time(Duration::from_secs(3));
    nps_spacing.throughput(Throughput::Elements(report_nps_bench::MEASURE_COUNT as u64));
    nps_spacing.bench_function("materialized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_nps_materialized(
                black_box(&mut output),
                black_box(nps_report_chart),
            )
            .expect("materialized NPS JSON should write");
            black_box(output)
        });
    });
    nps_spacing.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_nps(black_box(&mut output), black_box(nps_report_chart))
                .expect("streamed NPS JSON should write");
            black_box(output)
        });
    });
    nps_spacing.finish();

    let mut nps_report = c.benchmark_group("cycles/report_json_nps");
    nps_report.sample_size(20);
    nps_report.measurement_time(Duration::from_secs(3));
    nps_report.throughput(Throughput::Elements(report_nps_bench::MEASURE_COUNT as u64));
    nps_report.bench_function("materialized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_nps_report_materialized(
                black_box(&nps_report_summary),
                black_box(&mut output),
            )
            .expect("materialized NPS JSON report should write");
            black_box(output)
        });
    });
    nps_report.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&nps_report_summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("streamed NPS JSON report should write");
            black_box(output)
        });
    });
    nps_report.finish();

    let mut stream_sequences = c.benchmark_group("cycles/report_json_stream_sequences");
    stream_sequences.sample_size(20);
    stream_sequences.measurement_time(Duration::from_secs(3));
    stream_sequences.throughput(Throughput::Elements(report_nps_bench::MEASURE_COUNT as u64));
    stream_sequences.bench_function("materialized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_streams_materialized(
                black_box(&mut output),
                black_box(nps_report_chart),
            )
            .expect("materialized stream JSON should write");
            black_box(output)
        });
    });
    stream_sequences.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_streams(black_box(&mut output), black_box(nps_report_chart))
                .expect("streamed stream JSON should write");
            black_box(output)
        });
    });
    stream_sequences.finish();

    let mut stream_report = c.benchmark_group("cycles/report_json_streams");
    stream_report.sample_size(20);
    stream_report.measurement_time(Duration::from_secs(3));
    stream_report.throughput(Throughput::Elements(report_nps_bench::MEASURE_COUNT as u64));
    stream_report.bench_function("materialized", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::profile::write_json_streams_report_materialized(
                black_box(&nps_report_summary),
                black_box(&mut output),
            )
            .expect("materialized stream JSON report should write");
            black_box(output)
        });
    });
    stream_report.bench_function("streamed", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            rssp::report::write_reports(
                black_box(&nps_report_summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("streamed stream JSON report should write");
            black_box(output)
        });
    });
    stream_report.finish();

    let mut analysis = c.benchmark_group("cycles/analysis_scratch");
    analysis.sample_size(10);
    analysis.measurement_time(Duration::from_secs(3));
    analysis.throughput(Throughput::Bytes(analysis_fixture.len() as u64));
    analysis.bench_function("fresh", |b| {
        b.iter(|| {
            black_box(
                rssp::analyze(
                    black_box(analysis_fixture),
                    black_box("ssc"),
                    black_box(&analysis_options),
                )
                .expect("fixture should analyze"),
            );
        });
    });
    analysis.bench_function("reused_bpm_allocating", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::analyze_with_allocating_bpms(
                    black_box(analysis_fixture),
                    black_box("ssc"),
                    black_box(&analysis_options),
                    black_box(&mut allocating_analysis_scratch),
                )
                .expect("fixture should analyze"),
            );
        });
    });
    analysis.bench_function("reused_bpm_buffers", |b| {
        b.iter(|| {
            black_box(
                rssp::analyze_with_scratch(
                    black_box(analysis_fixture),
                    black_box("ssc"),
                    black_box(&analysis_options),
                    black_box(&mut analysis_scratch),
                )
                .expect("fixture should analyze"),
            );
        });
    });
    analysis.finish();

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
    let mut legacy_single =
        rssp::step_parity::legacy_timing_rows_scratch::<4>().expect("dance-single parity layout");
    let mut legacy_double =
        rssp::step_parity::legacy_timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut annotation_scratch =
        rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
    let mut reused_annotations = Vec::new();
    rssp::step_parity::analyze_and_annotate_timing_rows_known_holds_in(
        &single_rows,
        &single_beats,
        &parity_timing,
        false,
        &mut annotation_scratch,
        &mut reused_annotations,
    );

    let mut parity = c.benchmark_group("cycles/step_parity");
    parity.sample_size(50);
    parity.measurement_time(Duration::from_secs(3));
    parity.throughput(Throughput::Elements(
        step_parity_bench::SINGLE_ROW_COUNT as u64,
    ));
    parity.bench_function("dense_single_legacy", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_legacy_for_bench(
                black_box(&single_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                false,
                black_box(&mut legacy_single),
            ));
        });
    });
    parity.bench_function("dense_single_compact", |b| {
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
    parity.bench_function("annotations_single_owned", |b| {
        b.iter(|| {
            black_box(
                rssp::step_parity::analyze_and_annotate_timing_rows_known_holds(
                    black_box(&single_rows),
                    black_box(&single_beats),
                    black_box(&parity_timing),
                    false,
                    black_box(&mut annotation_scratch),
                ),
            );
        });
    });
    parity.bench_function("annotations_single_reused", |b| {
        b.iter(|| {
            black_box(
                rssp::step_parity::analyze_and_annotate_timing_rows_known_holds_in(
                    black_box(&single_rows),
                    black_box(&single_beats),
                    black_box(&parity_timing),
                    false,
                    black_box(&mut annotation_scratch),
                    black_box(&mut reused_annotations),
                ),
            );
        });
    });
    parity.bench_function("dense_single_cold", |b| {
        b.iter(|| {
            let mut scratch =
                rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
            black_box(rssp::step_parity::analyze_timing_rows_known_holds(
                black_box(&single_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                false,
                black_box(&mut scratch),
            ));
        });
    });
    parity.bench_function("dense_single_holds_legacy", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_legacy_for_bench(
                black_box(&single_hold_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                true,
                black_box(&mut legacy_single),
            ));
        });
    });
    parity.bench_function("dense_single_holds_compact", |b| {
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
    parity.bench_function("dense_double_legacy", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_legacy_for_bench(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                black_box(&mut legacy_double),
            ));
        });
    });
    parity.bench_function("dense_double_compact", |b| {
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
    parity.bench_function("dense_double_holds_legacy", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_legacy_for_bench(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                black_box(&mut legacy_double),
            ));
        });
    });
    parity.bench_function("dense_double_holds_compact", |b| {
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
