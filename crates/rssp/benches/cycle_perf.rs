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
#[path = "support/bpm_display.rs"]
mod bpm_display_bench;
#[cfg(windows)]
#[path = "support/bpm_summary.rs"]
mod bpm_summary_bench;
#[cfg(windows)]
#[allow(dead_code)]
#[path = "support/course.rs"]
mod course_bench;
#[cfg(windows)]
#[path = "support/elapsed.rs"]
mod elapsed_bench;
#[cfg(windows)]
#[path = "support/last_beat.rs"]
mod last_beat_bench;
#[cfg(windows)]
#[path = "support/metadata.rs"]
mod metadata_bench;
#[cfg(windows)]
#[path = "support/nps_stats.rs"]
mod nps_stats_bench;
#[cfg(windows)]
#[path = "support/pack.rs"]
mod pack_bench;
#[cfg(windows)]
#[path = "support/parse_dispatch.rs"]
mod parse_dispatch_bench;
#[cfg(windows)]
#[path = "support/path_sort.rs"]
mod path_sort_bench;
#[cfg(windows)]
#[path = "support/pattern_scratch.rs"]
mod pattern_scratch_bench;
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
#[path = "support/row_to_beat.rs"]
mod row_to_beat_bench;
#[cfg(windows)]
#[path = "support/selectable.rs"]
mod selectable_bench;
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
#[path = "support/tech_prefix.rs"]
mod tech_prefix_bench;
#[cfg(windows)]
#[path = "support/text_report.rs"]
mod text_report_bench;
#[cfg(windows)]
#[path = "support/timing_borrow.rs"]
mod timing_borrow_bench;
#[cfg(windows)]
#[path = "support/timing_merge.rs"]
mod timing_merge_bench;
#[cfg(windows)]
#[path = "support/timing_rows.rs"]
mod timing_rows_bench;
#[cfg(windows)]
#[path = "support/timing_segments.rs"]
mod timing_segments_bench;
#[cfg(windows)]
#[path = "support/timing_sort.rs"]
mod timing_sort_bench;
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
fn invalid_chart_notes(
    data: &[u8],
    legacy_invalid_heads: bool,
    scratch: &mut rssp::stats::ChartNotesScratch,
) -> usize {
    let (chart, stats, densities, beats, last) =
        rssp::stats::minimize_chart_count_rows_notes_for_bench(
            data,
            4,
            legacy_invalid_heads,
            scratch,
        );
    let note_checksum = scratch.drain().fold(0usize, |sum, note| {
        sum.wrapping_add(note.row_index)
            .wrapping_add(note.column)
            .wrapping_add(note.tail_row_index.unwrap_or(0))
    });
    let checksum = chart
        .len()
        .wrapping_add(stats.total_arrows as usize)
        .wrapping_add(densities.len())
        .wrapping_add(beats.len())
        .wrapping_add(last.to_bits() as usize)
        .wrapping_add(note_checksum);
    black_box((chart, stats, densities, beats, last));
    checksum
}

#[cfg(windows)]
fn phantom_hold_ends(data: &[u8], legacy_options: bool) -> usize {
    let (chart, stats, densities, beats, last) =
        rssp::stats::minimize_chart_count_rows_hold_ends_for_bench(data, 4, legacy_options);
    let checksum = chart
        .len()
        .wrapping_add(stats.total_arrows as usize)
        .wrapping_add(densities.len())
        .wrapping_add(beats.len())
        .wrapping_add(last.to_bits() as usize);
    black_box((chart, stats, densities, beats, last));
    checksum
}

#[cfg(windows)]
fn equally_spaced_count(data: &[u8], legacy_count: bool) -> usize {
    let values = rssp::nps::measure_equally_spaced_for_bench(data, 4, legacy_count);
    let checksum = values.len() + values.iter().filter(|value| **value).count();
    black_box(values);
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
fn hash_cycles(
    rows: &[[u8; 4]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    legacy_hash: bool,
    scratch: &mut rssp::step_parity::TimingRowsScratch<4>,
) -> u64 {
    let start = platform::read_cycles();
    black_box(rssp::step_parity::analyze_timing_rows_hash_for_bench(
        black_box(rows),
        black_box(beats),
        black_box(timing),
        true,
        legacy_hash,
        black_box(scratch),
    ));
    platform::read_cycles() - start
}

#[cfg(windows)]
#[allow(clippy::cast_precision_loss)]
fn print_hash_pairs(
    rows: &[[u8; 4]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    legacy_scratch: &mut rssp::step_parity::TimingRowsScratch<4>,
    folded_scratch: &mut rssp::step_parity::TimingRowsScratch<4>,
) {
    const SAMPLES: usize = 31;
    let mut legacy = [0u64; SAMPLES];
    let mut folded = [0u64; SAMPLES];
    let mut ratios = [0.0f64; SAMPLES];
    for sample in 0..SAMPLES {
        let (legacy_cycles, folded_cycles) = if sample.is_multiple_of(2) {
            (
                hash_cycles(rows, beats, timing, true, legacy_scratch),
                hash_cycles(rows, beats, timing, false, folded_scratch),
            )
        } else {
            let folded_cycles = hash_cycles(rows, beats, timing, false, folded_scratch);
            let legacy_cycles = hash_cycles(rows, beats, timing, true, legacy_scratch);
            (legacy_cycles, folded_cycles)
        };
        legacy[sample] = legacy_cycles;
        folded[sample] = folded_cycles;
        ratios[sample] = folded_cycles as f64 / legacy_cycles as f64;
    }
    legacy.sort_unstable();
    folded.sort_unstable();
    ratios.sort_by(f64::total_cmp);
    let mid = SAMPLES / 2;
    eprintln!(
        concat!(
            "step_parity_hash paired_samples={} legacy_median_cycles={} ",
            "folded_median_cycles={} median_change={:+.3}%"
        ),
        SAMPLES,
        legacy[mid],
        folded[mid],
        (ratios[mid] - 1.0) * 100.0,
    );
}

#[cfg(windows)]
fn note_data_cycles(data: &[u8], fused: bool, analyze: bool) -> u64 {
    let start = platform::read_cycles();
    if analyze {
        black_box(rssp::step_parity::analyze_note_data_for_bench(
            black_box(data),
            4,
            fused,
        ));
    } else {
        black_box(rssp::step_parity::parse_notes_for_bench(
            black_box(data),
            4,
            fused,
        ));
    }
    platform::read_cycles() - start
}

#[cfg(windows)]
#[allow(clippy::cast_precision_loss)]
fn print_note_data_pairs(data: &[u8], analyze: bool) {
    const SAMPLES: usize = 31;
    let mut materialized = [0u64; SAMPLES];
    let mut fused = [0u64; SAMPLES];
    let mut ratios = [0.0f64; SAMPLES];
    for sample in 0..SAMPLES {
        let (materialized_cycles, fused_cycles) = if sample.is_multiple_of(2) {
            (
                note_data_cycles(data, false, analyze),
                note_data_cycles(data, true, analyze),
            )
        } else {
            let fused_cycles = note_data_cycles(data, true, analyze);
            let materialized_cycles = note_data_cycles(data, false, analyze);
            (materialized_cycles, fused_cycles)
        };
        materialized[sample] = materialized_cycles;
        fused[sample] = fused_cycles;
        ratios[sample] = fused_cycles as f64 / materialized_cycles as f64;
    }
    materialized.sort_unstable();
    fused.sort_unstable();
    ratios.sort_by(f64::total_cmp);
    let mid = SAMPLES / 2;
    eprintln!(
        concat!(
            "parity_note_data stage={} paired_samples={} materialized_median_cycles={} ",
            "fused_median_cycles={} median_change={:+.3}%"
        ),
        if analyze { "analysis" } else { "parse" },
        SAMPLES,
        materialized[mid],
        fused[mid],
        (ratios[mid] - 1.0) * 100.0,
    );
}

#[cfg(windows)]
fn path_join_cycles(base: &std::path::Path, prealloc: bool) -> u64 {
    const ITERATIONS: usize = 4_096;
    let start = platform::read_cycles();
    for _ in 0..ITERATIONS {
        black_box(rssp::profile::relative_path_join(
            black_box(base),
            black_box("Visuals/Background,Layer.png"),
            prealloc,
        ));
    }
    platform::read_cycles() - start
}

#[cfg(windows)]
#[allow(clippy::cast_precision_loss)]
fn print_path_join_pairs(base: &std::path::Path) {
    const SAMPLES: usize = 31;
    let mut growing = [0u64; SAMPLES];
    let mut preallocated = [0u64; SAMPLES];
    let mut ratios = [0.0f64; SAMPLES];
    for sample in 0..SAMPLES {
        let (growing_cycles, preallocated_cycles) = if sample.is_multiple_of(2) {
            (path_join_cycles(base, false), path_join_cycles(base, true))
        } else {
            let preallocated_cycles = path_join_cycles(base, true);
            let growing_cycles = path_join_cycles(base, false);
            (growing_cycles, preallocated_cycles)
        };
        growing[sample] = growing_cycles;
        preallocated[sample] = preallocated_cycles;
        ratios[sample] = preallocated_cycles as f64 / growing_cycles as f64;
    }
    growing.sort_unstable();
    preallocated.sort_unstable();
    ratios.sort_by(f64::total_cmp);
    let mid = SAMPLES / 2;
    eprintln!(
        concat!(
            "background_path_join paired_samples={} growing_median_cycles={} ",
            "preallocated_median_cycles={} median_change={:+.3}%"
        ),
        SAMPLES,
        growing[mid],
        preallocated[mid],
        (ratios[mid] - 1.0) * 100.0,
    );
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
    let timing_maps = timing_borrow_bench::TimingMaps::new();
    timing_borrow_bench::assert_behavior(&timing_maps);
    let timing_sort_fixture = timing_sort_bench::fixture();
    timing_sort_bench::assert_behavior(&timing_sort_fixture);
    let timing_segments_fixture = timing_segments_bench::fixture();
    timing_segments_bench::assert_behavior(&timing_segments_fixture);
    let row_to_beat_fixture = row_to_beat_bench::fixture();
    row_to_beat_bench::assert_behavior(&row_to_beat_fixture);
    let legacy_metadata = cp1252_metadata(ENTRIES);
    let (valid_tech, valid_description) = tech_prefix_bench::valid_input();
    let invalid_tech = tech_prefix_bench::invalid_input();
    tech_prefix_bench::assert_behavior();
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
    let bpm_display_fixture = bpm_display_bench::fixture();
    bpm_display_bench::assert_behavior(&bpm_display_fixture);
    let parse_dispatch_fixture = parse_dispatch_bench::fixture();
    parse_dispatch_bench::assert_behavior(&parse_dispatch_fixture);
    parse_dispatch_bench::assert_reserve_behavior();
    selectable_bench::assert_behavior();
    let text_fixture = metadata_bench::fixture("0.83");
    let text_summary = rssp::analyze(text_fixture.as_bytes(), "ssc", &metadata_bench::options())
        .expect("text report fixture should analyze");
    text_report_bench::assert_behavior(&text_summary);
    let parse_reserve_typical =
        parse_dispatch_bench::fixture_with_charts(parse_dispatch_bench::TYPICAL_CHART_COUNT);
    let parse_reserve_sm =
        parse_dispatch_bench::sm_fixture(parse_dispatch_bench::TYPICAL_CHART_COUNT);

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

    let mut segment_parse = c.benchmark_group("cycles/timing_segments_3840");
    segment_parse.throughput(Throughput::Bytes(timing_segments_fixture.len() as u64));
    segment_parse.sample_size(100);
    segment_parse.measurement_time(Duration::from_secs(3));
    segment_parse.bench_function("scalar_capacity_scan", |b| {
        b.iter(|| {
            black_box(timing_segments_bench::parse(
                black_box(&timing_segments_fixture),
                true,
            ));
        });
    });
    segment_parse.bench_function("chunked_capacity_scan", |b| {
        b.iter(|| {
            black_box(timing_segments_bench::parse(
                black_box(&timing_segments_fixture),
                false,
            ));
        });
    });
    segment_parse.finish();

    let mut row_to_beat = c.benchmark_group("cycles/row_to_beat_26624_rows");
    row_to_beat.throughput(Throughput::Elements(row_to_beat_bench::ROW_COUNT as u64));
    row_to_beat.sample_size(100);
    row_to_beat.measurement_time(Duration::from_secs(3));
    row_to_beat.bench_function("growing", |b| {
        b.iter(|| {
            black_box(row_to_beat_bench::compute(
                black_box(&row_to_beat_fixture),
                true,
            ));
        });
    });
    row_to_beat.bench_function("preallocated", |b| {
        b.iter(|| {
            black_box(row_to_beat_bench::compute(
                black_box(&row_to_beat_fixture),
                false,
            ));
        });
    });
    row_to_beat.finish();

    let mut parse_dispatch = c.benchmark_group("cycles/parse_dispatch_128_charts");
    parse_dispatch.throughput(Throughput::Bytes(parse_dispatch_fixture.len() as u64));
    parse_dispatch.sample_size(100);
    parse_dispatch.measurement_time(Duration::from_secs(3));
    for (name, legacy) in [("sequential_tags", true), ("indexed_tags", false)] {
        parse_dispatch.bench_function(name, |b| {
            b.iter(|| {
                black_box(parse_dispatch_bench::parse(
                    black_box(&parse_dispatch_fixture),
                    "ssc",
                    legacy,
                ));
            });
        });
    }
    parse_dispatch.finish();

    parse_dispatch_bench::assert_append_behavior(&parse_dispatch_fixture, "ssc");
    let mut parse_append = c.benchmark_group("cycles/parse_attack_append_128_charts");
    parse_append.throughput(Throughput::Bytes(parse_dispatch_fixture.len() as u64));
    parse_append.sample_size(100);
    parse_append.measurement_time(Duration::from_secs(3));
    for (name, legacy) in [("allocate_then_grow", true), ("presized_copy", false)] {
        parse_append.bench_function(name, |b| {
            b.iter(|| {
                black_box(parse_dispatch_bench::parse_append(
                    black_box(&parse_dispatch_fixture),
                    "ssc",
                    legacy,
                ));
            });
        });
    }
    parse_append.finish();

    let real_parse_fixture = include_bytes!("fixtures/camellia_mix.ssc");
    parse_dispatch_bench::assert_pair(real_parse_fixture, "ssc");
    let mut real_parse = c.benchmark_group("cycles/parse_dispatch_real_ssc");
    real_parse.throughput(Throughput::Bytes(real_parse_fixture.len() as u64));
    real_parse.sample_size(100);
    real_parse.measurement_time(Duration::from_secs(3));
    for (name, legacy) in [("indexed_tags", false), ("sequential_tags", true)] {
        real_parse.bench_function(name, |b| {
            b.iter(|| {
                black_box(parse_dispatch_bench::parse(
                    black_box(real_parse_fixture),
                    "ssc",
                    legacy,
                ));
            });
        });
    }
    real_parse.finish();

    for (name, data, ext) in [
        (
            "cycles/parse_reserve_ssc_10_charts",
            parse_reserve_typical.as_slice(),
            "ssc",
        ),
        (
            "cycles/parse_reserve_ssc_128_charts",
            parse_dispatch_fixture.as_slice(),
            "ssc",
        ),
        (
            "cycles/parse_reserve_sm_10_charts",
            parse_reserve_sm.as_slice(),
            "sm",
        ),
    ] {
        let mut group = c.benchmark_group(name);
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.sample_size(100);
        group.measurement_time(Duration::from_secs(3));
        for (phase, legacy) in [("growing_vec", true), ("presized_vec", false)] {
            group.bench_function(phase, |b| {
                b.iter(|| {
                    black_box(parse_dispatch_bench::parse_reserved(
                        black_box(data),
                        ext,
                        legacy,
                    ));
                });
            });
        }
        group.finish();
    }

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

    let mut display_tags = c.benchmark_group("cycles/bpm_display_tags_256");
    display_tags.throughput(Throughput::Elements(bpm_display_bench::CHART_COUNT as u64));
    display_tags.sample_size(100);
    display_tags.measurement_time(Duration::from_secs(3));
    display_tags.bench_function("owned_temporary", |b| {
        b.iter(|| {
            black_box(bpm_display_bench::compute(
                black_box(&bpm_display_fixture),
                true,
            ));
        });
    });
    display_tags.bench_function("borrowed_tag", |b| {
        b.iter(|| {
            black_box(bpm_display_bench::compute(
                black_box(&bpm_display_fixture),
                false,
            ));
        });
    });
    display_tags.finish();

    for (name, credit, description) in [
        (
            "cycles/tech_prefix_valid",
            valid_tech.as_str(),
            valid_description.as_str(),
        ),
        ("cycles/tech_prefix_invalid", invalid_tech.as_str(), ""),
    ] {
        let mut group = c.benchmark_group(name);
        group.sample_size(100);
        group.measurement_time(Duration::from_secs(2));
        group.throughput(Throughput::Bytes((credit.len() + description.len()) as u64));
        for (phase, mode) in [
            ("runtime_index", 0),
            ("const_index_unicode", 1),
            ("const_index_ascii", 2),
        ] {
            group.bench_function(phase, |b| {
                b.iter(|| {
                    black_box(match mode {
                        0 => tech_prefix_bench::parse(
                            black_box(credit),
                            black_box(description),
                            true,
                        ),
                        1 => tech_prefix_bench::parse_unicode(
                            black_box(credit),
                            black_box(description),
                        ),
                        _ => tech_prefix_bench::parse(
                            black_box(credit),
                            black_box(description),
                            false,
                        ),
                    });
                });
            });
        }
        group.finish();
    }

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

    let mut selectable = c.benchmark_group("cycles/selectable_4096");
    selectable.throughput(Throughput::Elements(selectable_bench::BATCH as u64));
    selectable.sample_size(100);
    selectable.measurement_time(Duration::from_secs(3));
    selectable.bench_function("owned_compare", |b| {
        b.iter(|| black_box(selectable_bench::run::<true>()));
    });
    selectable.bench_function("borrowed_compare", |b| {
        b.iter(|| black_box(selectable_bench::run::<false>()));
    });
    selectable.finish();

    for (group_name, full) in [
        ("cycles/text_report_pretty_256", false),
        ("cycles/text_report_full_256", true),
    ] {
        let mut sizing = Vec::new();
        text_report_bench::write(&text_summary, &mut sizing, full, false);
        let mut group = c.benchmark_group(group_name);
        group.throughput(Throughput::Elements(metadata_bench::CHART_COUNT as u64));
        group.sample_size(100);
        group.measurement_time(Duration::from_secs(3));
        for (phase, legacy) in [("materialized", true), ("streamed", false)] {
            let mut output = Vec::with_capacity(sizing.len());
            group.bench_function(phase, |b| {
                b.iter(|| {
                    black_box(text_report_bench::write(
                        black_box(&text_summary),
                        black_box(&mut output),
                        full,
                        legacy,
                    ));
                });
            });
        }
        group.finish();
    }

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
    normalization.throughput(Throughput::Bytes(timing_maps.bytes()));
    normalization.bench_function("timing_maps_owned", |b| {
        b.iter(|| black_box(timing_maps.owned()));
    });
    normalization.bench_function("timing_maps_borrowed", |b| {
        b.iter(|| black_box(timing_maps.borrowed()));
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

    let mut timing_sort = c.benchmark_group("cycles/timing_segment_sort_4096");
    timing_sort.throughput(Throughput::Elements(timing_sort_bench::ENTRY_COUNT as u64));
    timing_sort.sample_size(100);
    timing_sort.measurement_time(Duration::from_secs(3));
    for (phase, legacy) in [("packed_records", true), ("key_indices", false)] {
        timing_sort.bench_function(phase, |b| {
            b.iter_batched(
                || timing_sort_fixture.clone(),
                |input| black_box(timing_sort_bench::tidy(input, legacy)),
                BatchSize::SmallInput,
            );
        });
    }
    timing_sort.finish();

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

    let timing_rows_fixture = timing_rows_bench::TimingRowsFixture::new();
    timing_rows_bench::assert_behavior(&timing_rows_fixture);
    let mut timing_rows = c.benchmark_group("cycles/timing_segment_rows_256");
    timing_rows.throughput(Throughput::Elements(timing_rows_bench::INPUT_COUNT));
    timing_rows.sample_size(100);
    timing_rows.measurement_time(Duration::from_secs(3));
    for (name, packed) in [("split", false), ("packed", true)] {
        timing_rows.bench_function(name, |b| {
            b.iter(|| {
                black_box(rssp::timing::build_segment_rows_for_bench(
                    black_box(&timing_rows_fixture.stops),
                    black_box(&timing_rows_fixture.delays),
                    black_box(&timing_rows_fixture.warps),
                    black_box(&timing_rows_fixture.fakes),
                    packed,
                ));
            });
        });
    }
    timing_rows.finish();

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

    let extra_warps = sm_timing_bench::extra_warps();
    let mut sm_warps = c.benchmark_group("cycles/sm_warp_merge_2048");
    sm_warps.throughput(Throughput::Elements(sm_timing_bench::WARP_COUNT as u64));
    sm_warps.sample_size(50);
    sm_warps.measurement_time(Duration::from_secs(3));
    sm_warps.bench_function("copy_into_empty", |b| {
        b.iter_batched(
            || extra_warps.clone(),
            |extra| {
                black_box(rssp::timing::merge_extra_warps_for_bench(
                    Vec::new(),
                    black_box(extra),
                    false,
                ));
            },
            BatchSize::SmallInput,
        );
    });
    sm_warps.bench_function("reuse_generated", |b| {
        b.iter_batched(
            || extra_warps.clone(),
            |extra| {
                black_box(rssp::timing::merge_extra_warps_for_bench(
                    Vec::new(),
                    black_box(extra),
                    true,
                ));
            },
            BatchSize::SmallInput,
        );
    });
    sm_warps.finish();

    let (warp_bpms, warp_stops) = sm_timing_bench::warp_inputs();
    let mut sm_warp_pipeline = c.benchmark_group("cycles/sm_warp_pipeline_2048");
    sm_warp_pipeline.throughput(Throughput::Elements(sm_timing_bench::WARP_COUNT as u64));
    sm_warp_pipeline.sample_size(50);
    sm_warp_pipeline.measurement_time(Duration::from_secs(3));
    sm_warp_pipeline.bench_function("copy_into_empty", |b| {
        b.iter(|| {
            black_box(rssp::timing::process_sm_warp_merge_for_bench(
                black_box(&warp_bpms),
                black_box(&warp_stops),
                false,
            ));
        });
    });
    sm_warp_pipeline.bench_function("reuse_generated", |b| {
        b.iter(|| {
            black_box(rssp::timing::process_sm_warp_merge_for_bench(
                black_box(&warp_bpms),
                black_box(&warp_stops),
                true,
            ));
        });
    });
    sm_warp_pipeline.finish();

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
    let matrix_profile = rssp::matrix::compute_matrix_profile(&matrix_densities, &matrix_bpms);
    let matrix_queries: Vec<_> = (0..4_096)
        .map(|index| {
            (
                40.0 + (index % 2_048) as f64 * 0.25,
                [1.0, 4.0, 16.0, 64.0, 256.0, 1_024.0][index % 6],
            )
        })
        .collect();
    let nps_values: Vec<_> = (0..1_025)
        .map(|idx| ((idx * 37) % 257) as f64 / 7.0)
        .collect();
    let bpm_stats_map = bpm_summary_bench::fixture();
    bpm_summary_bench::assert_behavior(&bpm_stats_map);
    let mut legacy_bpm_values = Vec::with_capacity(bpm_stats_map.len());
    let mut fused_bpm_values = Vec::with_capacity(bpm_stats_map.len());
    let mut nps_scratch = Vec::new();
    let custom_patterns = custom_pattern_input(256);
    let custom_pattern_rows = pattern_scratch_bench::rows();
    let default_pattern_masks: Vec<_> = custom_pattern_rows
        .iter()
        .map(|row| {
            u8::from(row[0] != b'0')
                | (u8::from(row[1] != b'0') << 1)
                | (u8::from(row[2] != b'0') << 2)
                | (u8::from(row[3] != b'0') << 3)
        })
        .collect();
    let default_expected =
        rssp::patterns::detect_default_patterns_runtime_build_for_bench(&default_pattern_masks);
    assert_eq!(
        rssp::patterns::detect_default_patterns_heap_for_bench(&default_pattern_masks),
        default_expected
    );
    assert_eq!(
        rssp::patterns::detect_default_patterns(&default_pattern_masks),
        default_expected
    );
    let custom_pattern_compiled = rssp::patterns::compile_custom_patterns(&custom_patterns);
    pattern_scratch_bench::assert_behavior(&custom_pattern_rows, 6, &custom_pattern_compiled);
    let mut custom_count_scratch = Vec::new();
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
    let mut prepared_scratch = rssp::AnalysisScratch::default();
    let expected_batch = rssp::analyze(BATCH_FIXTURE, "ssc", &batch_options.clone())
        .expect("fresh batch analysis should succeed");
    let actual_batch =
        rssp::analyze_prepared_in(BATCH_FIXTURE, "ssc", &prepared_batch, &mut prepared_scratch)
            .expect("prepared batch analysis should succeed");
    let (mut expected_batch_json, mut actual_batch_json) = (Vec::new(), Vec::new());
    rssp::report::write_reports(
        &expected_batch,
        rssp::report::OutputMode::JSON,
        &mut expected_batch_json,
    )
    .expect("fresh batch summary should serialize");
    rssp::report::write_reports(
        &actual_batch,
        rssp::report::OutputMode::JSON,
        &mut actual_batch_json,
    )
    .expect("prepared batch summary should serialize");
    assert_eq!(actual_batch_json, expected_batch_json);
    let analysis_fixture = include_bytes!("fixtures/camellia_mix.ssc");
    let nps_fixture = include_bytes!("fixtures/watch_yo_step.ssc");
    let duration_owned =
        rssp::duration::chart_durations_owned(nps_fixture, "ssc", rssp::TimingOffsets::default())
            .expect("owned duration fixture should analyze");
    let duration_borrowed =
        rssp::compute_chart_durations(nps_fixture, "ssc", rssp::TimingOffsets::default())
            .expect("borrowed duration fixture should analyze");
    assert_eq!(duration_borrowed.len(), duration_owned.len());
    for (actual, expected) in duration_borrowed.iter().zip(&duration_owned) {
        assert_eq!(actual.step_type, expected.step_type);
        assert_eq!(actual.difficulty, expected.difficulty);
        assert_eq!(actual.duration_seconds, expected.duration_seconds);
    }
    let nps_owned = rssp::nps::chart_peak_nps_owned(nps_fixture, "ssc")
        .expect("owned NPS fixture should analyze");
    let nps_borrowed =
        rssp::compute_chart_peak_nps(nps_fixture, "ssc").expect("NPS fixture should analyze");
    assert_eq!(nps_borrowed.len(), nps_owned.len());
    for (actual, expected) in nps_borrowed.iter().zip(&nps_owned) {
        assert_eq!(actual.step_type, expected.step_type);
        assert_eq!(actual.difficulty, expected.difficulty);
        assert_eq!(actual.peak_nps, expected.peak_nps);
    }
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
    let mut invalid_note_rows = Vec::with_capacity(4096 * 5 + 1);
    for row in 0usize..4096 {
        invalid_note_rows.extend_from_slice(if row.is_multiple_of(2) {
            b"2000\n"
        } else {
            b"1000\n"
        });
    }
    invalid_note_rows.push(b';');
    let mut spaced_rows = Vec::with_capacity(16_384 * 27);
    for measure in 0usize..16_384 {
        spaced_rows.extend_from_slice(b"1000\n0100\n0010\n0001\n");
        spaced_rows.extend_from_slice(if measure + 1 == 16_384 { b";" } else { b",\n" });
    }
    let analysis_options = rssp::AnalysisOptions::default();
    let mut allocating_analysis_scratch = rssp::AnalysisScratch::default();
    let mut owned_timing_scratch = rssp::AnalysisScratch::default();
    let mut analysis_scratch = rssp::AnalysisScratch::default();
    {
        let expected = rssp::profile::analyze_owned_timing(
            analysis_fixture,
            "ssc",
            &analysis_options,
            &mut owned_timing_scratch,
        )
        .expect("owned timing analysis should succeed");
        let actual = rssp::analyze_with_scratch(
            analysis_fixture,
            "ssc",
            &analysis_options,
            &mut analysis_scratch,
        )
        .expect("borrowed timing analysis should succeed");
        let (mut expected_json, mut actual_json) = (Vec::new(), Vec::new());
        rssp::report::write_reports(
            &expected,
            rssp::report::OutputMode::JSON,
            &mut expected_json,
        )
        .expect("owned timing summary should serialize");
        rssp::report::write_reports(&actual, rssp::report::OutputMode::JSON, &mut actual_json)
            .expect("borrowed timing summary should serialize");
        assert_eq!(actual_json, expected_json);
    }
    let mut stream_tokens = Vec::new();
    let course_fixture = course_bench::CourseFixture::new();
    let repeated_course = course_bench::CourseFixture::repeated();
    let banner_fixture = course_bench::BannerFixture::new();
    let resolve_fixture = course_bench::ResolveFixture::new();
    banner_fixture.assert_behavior();
    resolve_fixture.assert_behavior();
    course_bench::assert_step_norm_behavior();
    course_bench::assert_title_match_behavior();
    course_fixture.assert_group_cache();
    course_fixture.assert_group_catalog();
    course_fixture.assert_catalog_dirs();
    repeated_course.assert_song_cache();
    repeated_course.assert_nps_capacity();
    let course_hashes = course_bench::hash_values();
    course_bench::assert_hash_dedup_behavior(&course_hashes);
    let course_summary_hashes = course_bench::course_hash_values();
    course_bench::assert_hash_dedup_behavior(&course_summary_hashes);
    let typical_course_hashes = course_bench::typical_hash_values();
    course_bench::assert_hash_dedup_behavior(&typical_course_hashes);
    let course_input =
        std::fs::read(course_fixture.course_path()).expect("benchmark course should be readable");
    let legacy_course = rssp::course::profile_parse_crs(&course_input, true)
        .expect("legacy benchmark course should parse");
    let current_course = rssp::course::profile_parse_crs(&course_input, false)
        .expect("benchmark course should parse");
    let sequential_course = rssp::course::profile_parse_crs_dispatch(&course_input, true)
        .expect("sequential dispatch course should parse");
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
    course_bench::assert_same_course(&current_course, &sequential_course);
    let select_input = course_bench::select_input();
    let course_options = course_bench::fast_options();
    let pack_fixture = pack_bench::PackFixture::new();
    let pack_image_fixture = pack_bench::ImageHintFixture::new();
    pack_bench::assert_pack_ini_behavior();
    pack_fixture.assert_root_behavior();
    pack_fixture.assert_song_behavior();
    pack_fixture.assert_tree_behavior();
    pack_fixture.assert_songs_behavior();
    pack_fixture.assert_parent_img_behavior();
    pack_image_fixture.assert_behavior();
    let asset_fixture = assets_bench::AssetFixture::with_movies(1);
    asset_fixture.assert_background_behavior();
    asset_fixture.assert_catalog_behavior();
    assets_bench::assert_bgchange_sort_behavior();
    let mut ordered_sort_legacy = assets_bench::ordered_changes();
    let mut ordered_sort_current = assets_bench::ordered_changes();
    let unordered_bgchanges = asset_fixture.unordered_simfile();
    asset_fixture.assert_unordered_behavior(&unordered_bgchanges);
    asset_fixture.assert_song_assets_behavior();
    asset_fixture.assert_music_behavior();
    asset_fixture.assert_rel_path_behavior();
    let relative_asset_paths = assets_bench::relative_paths();
    let relative_component_paths = assets_bench::relative_component_paths();
    assets_bench::assert_rel_component_behavior(&relative_component_paths);
    let relative_component_bytes = relative_component_paths
        .iter()
        .map(String::len)
        .sum::<usize>();
    let delimiter_fields = assets_bench::delimiter_fields();
    let delimiter_bytes = delimiter_fields.iter().map(String::len).sum::<usize>();
    let bgchange_tags = assets_bench::bgchange_tags();
    assets_bench::assert_bgchange_values_behavior(&bgchange_tags);
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
    path_sort_bench::assert_behavior();
    let sort_paths = path_sort_bench::paths();
    optimizations.throughput(Throughput::Elements(path_sort_bench::PATH_COUNT as u64));
    for (name, legacy) in [
        ("path_sort_cached_strings", true),
        ("path_sort_contiguous_keys", false),
    ] {
        optimizations.bench_function(name, |b| {
            b.iter_batched(
                || sort_paths.clone(),
                |mut paths| {
                    rssp::profile::sort_paths_ci(black_box(&mut paths), legacy);
                    black_box(paths);
                },
                BatchSize::SmallInput,
            );
        });
    }
    translate_bench::assert_behavior();
    assert!(rssp::translate::profile_alias_tables_match());
    black_box(rssp::translate::profile_alias_table_sizes());
    const ALIAS_LOOKUPS: [&str; 12] = [
        "hka",
        "KRO",
        "rightarrow",
        "whiteheart",
        "kdot",
        "omega",
        "auxtriangle",
        "menuright",
        "unknown",
        "hkaa",
        "",
        "UP",
    ];
    let mut legacy_slot = 0usize;
    let mut static_slot = 0usize;
    optimizations.throughput(Throughput::Elements(1));
    optimizations.bench_function("alias_table_runtime_build", |b| {
        b.iter(|| {
            legacy_slot = legacy_slot.wrapping_add(1);
            black_box(rssp::translate::profile_alias_build(
                black_box(legacy_slot),
                true,
            ));
        });
    });
    optimizations.bench_function("alias_table_static", |b| {
        b.iter(|| {
            static_slot = static_slot.wrapping_add(1);
            black_box(rssp::translate::profile_alias_build(
                black_box(static_slot),
                false,
            ));
        });
    });
    optimizations.throughput(Throughput::Elements(ALIAS_LOOKUPS.len() as u64));
    optimizations.bench_function("alias_lookup_legacy", |b| {
        b.iter(|| {
            for alias in ALIAS_LOOKUPS {
                black_box(rssp::translate::profile_alias_lookup(
                    black_box(alias),
                    true,
                ));
            }
        });
    });
    optimizations.bench_function("alias_lookup_compact", |b| {
        b.iter(|| {
            for alias in ALIAS_LOOKUPS {
                black_box(rssp::translate::profile_alias_lookup(
                    black_box(alias),
                    false,
                ));
            }
        });
    });
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
    last_beat_bench::assert_behavior();
    let last_beat_chart =
        last_beat_bench::chart(last_beat_bench::MEASURE_COUNT, last_beat_bench::ROW_COUNT);
    optimizations.throughput(Throughput::Bytes(
        (last_beat_chart.len() * last_beat_bench::LAST_BEAT_BATCH) as u64,
    ));
    for (name, legacy) in [
        ("last_beat_heap_measure", true),
        ("last_beat_stack_measure", false),
    ] {
        optimizations.bench_function(name, |b| {
            b.iter(|| {
                for _ in 0..last_beat_bench::LAST_BEAT_BATCH {
                    black_box(rssp::stats::chart_last_beat_for_bench(
                        black_box(&last_beat_chart),
                        black_box(4),
                        legacy,
                    ));
                }
            });
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
    optimizations.throughput(Throughput::Elements(matrix_queries.len() as u64));
    optimizations.bench_function("matrix_difficulty_legacy", |b| {
        b.iter(|| {
            let mut total = 0.0;
            for &(bpm, measures) in black_box(&matrix_queries) {
                total += rssp::matrix::get_difficulty_legacy_for_bench(bpm, measures);
            }
            black_box(total);
        });
    });
    optimizations.bench_function("matrix_difficulty_lookup", |b| {
        b.iter(|| {
            let mut total = 0.0;
            for &(bpm, measures) in black_box(&matrix_queries) {
                total += rssp::matrix::get_difficulty(bpm, measures);
            }
            black_box(total);
        });
    });
    optimizations.throughput(Throughput::Elements(matrix_profile.len() as u64));
    optimizations.bench_function("matrix_rate_rating_legacy", |b| {
        b.iter(|| {
            black_box(rssp::matrix::matrix_rating_at_rate_legacy_for_bench(
                black_box(&matrix_profile),
                black_box(1.25),
            ));
        });
    });
    optimizations.bench_function("matrix_rate_rating_lookup", |b| {
        b.iter(|| {
            black_box(matrix_profile.rating_at_rate(black_box(1.25)));
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
    optimizations.bench_function("custom_patterns_growing_dfa", |b| {
        b.iter(|| {
            black_box(
                rssp::patterns::compile_custom_patterns_growing_dfa_for_bench(black_box(
                    &custom_patterns,
                )),
            );
        });
    });
    optimizations.bench_function("custom_patterns_presized_dfa", |b| {
        b.iter(|| {
            black_box(rssp::patterns::compile_custom_patterns(black_box(
                &custom_patterns,
            )));
        });
    });
    let default_init_masks = &default_pattern_masks[..256];
    optimizations.throughput(Throughput::Elements(1));
    optimizations.bench_function("default_dfa_runtime_build", |b| {
        b.iter(|| {
            black_box(
                rssp::patterns::detect_default_patterns_runtime_build_for_bench(black_box(
                    default_init_masks,
                )),
            );
        });
    });
    optimizations.bench_function("default_dfa_static", |b| {
        b.iter(|| {
            black_box(rssp::patterns::detect_default_patterns(black_box(
                default_init_masks,
            )));
        });
    });
    optimizations.throughput(Throughput::Elements(default_pattern_masks.len() as u64));
    optimizations.bench_function("default_dfa_heap_search", |b| {
        b.iter(|| {
            black_box(rssp::patterns::detect_default_patterns_heap_for_bench(
                black_box(&default_pattern_masks),
            ));
        });
    });
    optimizations.bench_function("default_dfa_compact_search", |b| {
        b.iter(|| {
            black_box(rssp::patterns::detect_default_patterns(black_box(
                &default_pattern_masks,
            )));
        });
    });
    optimizations.throughput(Throughput::Elements(custom_pattern_rows.len() as u64));
    optimizations.bench_function("custom_pattern_counts_allocating", |b| {
        b.iter(|| {
            black_box(rssp::patterns::analyze_patterns_from_rows(
                black_box(&custom_pattern_rows),
                black_box(6),
                black_box(&custom_pattern_compiled),
            ));
        });
    });
    optimizations.bench_function("custom_pattern_counts_reused", |b| {
        b.iter(|| {
            black_box(rssp::patterns::analyze_patterns_from_rows_with_scratch(
                black_box(&custom_pattern_rows),
                black_box(6),
                black_box(&custom_pattern_compiled),
                black_box(&mut custom_count_scratch),
            ));
        });
    });
    optimizations.throughput(Throughput::Elements(1));
    optimizations.bench_function("custom_patterns_fresh_file", |b| {
        b.iter(|| {
            black_box(
                rssp::analyze(
                    black_box(BATCH_FIXTURE),
                    "ssc",
                    black_box(&batch_options.clone()),
                )
                .expect("batch fixture should analyze"),
            );
        });
    });
    optimizations.bench_function("custom_patterns_prepared_reused", |b| {
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

    let mut legacy_invalid_notes = rssp::stats::ChartNotesScratch::default();
    let mut marked_invalid_notes = rssp::stats::ChartNotesScratch::default();
    assert_eq!(
        invalid_chart_notes(&invalid_note_rows, true, &mut legacy_invalid_notes),
        invalid_chart_notes(&invalid_note_rows, false, &mut marked_invalid_notes),
    );
    let mut invalid_notes = c.benchmark_group("cycles/invalid_chart_notes_4096");
    invalid_notes.sample_size(50);
    invalid_notes.measurement_time(Duration::from_secs(3));
    invalid_notes.throughput(Throughput::Bytes(invalid_note_rows.len() as u64));
    invalid_notes.bench_function("index_vec_sort", |b| {
        b.iter(|| {
            black_box(invalid_chart_notes(
                black_box(&invalid_note_rows),
                true,
                black_box(&mut legacy_invalid_notes),
            ));
        });
    });
    invalid_notes.bench_function("in_place_mark", |b| {
        b.iter(|| {
            black_box(invalid_chart_notes(
                black_box(&invalid_note_rows),
                false,
                black_box(&mut marked_invalid_notes),
            ));
        });
    });
    invalid_notes.finish();

    assert_eq!(
        phantom_hold_ends(&invalid_note_rows, true),
        phantom_hold_ends(&invalid_note_rows, false),
    );
    let mut phantom_ends = c.benchmark_group("cycles/phantom_hold_ends_4096");
    phantom_ends.sample_size(50);
    phantom_ends.measurement_time(Duration::from_secs(3));
    phantom_ends.throughput(Throughput::Bytes(invalid_note_rows.len() as u64));
    phantom_ends.bench_function("option_table", |b| {
        b.iter(|| {
            black_box(phantom_hold_ends(
                black_box(&invalid_note_rows),
                black_box(true),
            ));
        });
    });
    phantom_ends.bench_function("sentinel_table", |b| {
        b.iter(|| {
            black_box(phantom_hold_ends(
                black_box(&invalid_note_rows),
                black_box(false),
            ));
        });
    });
    phantom_ends.finish();

    assert_eq!(
        equally_spaced_count(&spaced_rows, true),
        equally_spaced_count(&spaced_rows, false),
    );
    let mut spacing_count = c.benchmark_group("cycles/equally_spaced_count_16384");
    spacing_count.sample_size(50);
    spacing_count.measurement_time(Duration::from_secs(3));
    spacing_count.throughput(Throughput::Bytes(spaced_rows.len() as u64));
    spacing_count.bench_function("scalar_prepass", |b| {
        b.iter(|| {
            black_box(equally_spaced_count(
                black_box(&spaced_rows),
                black_box(true),
            ));
        });
    });
    spacing_count.bench_function("chunked_prepass", |b| {
        b.iter(|| {
            black_box(equally_spaced_count(
                black_box(&spaced_rows),
                black_box(false),
            ));
        });
    });
    spacing_count.finish();

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
    for (phase, legacy) in [("sum_after_fill", true), ("sum_while_fill", false)] {
        let values = if legacy {
            &mut legacy_bpm_values
        } else {
            &mut fused_bpm_values
        };
        bpm_stats.bench_function(phase, |b| {
            b.iter(|| {
                black_box(bpm_summary_bench::compute(
                    black_box(&bpm_stats_map),
                    black_box(values),
                    legacy,
                ))
            });
        });
    }
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

    let mut course_hash_dedup = c.benchmark_group("cycles/course_hash_dedup_4096");
    course_hash_dedup.sample_size(100);
    course_hash_dedup.measurement_time(Duration::from_secs(3));
    course_hash_dedup.throughput(Throughput::Elements(course_bench::HASH_DEDUP_COUNT as u64));
    course_hash_dedup.bench_function("std_sip_hash", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_dedup_hashes(
                black_box(&course_hashes),
                true,
            ));
        });
    });
    course_hash_dedup.bench_function("fold_hash_growing", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_dedup_hashes(
                black_box(&course_hashes),
                false,
            ));
        });
    });
    course_hash_dedup.bench_function("fold_hash_bounded_8", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_dedup_hashes_reserved(black_box(
                &course_hashes,
            )));
        });
    });
    course_hash_dedup.finish();

    let mut course_hash_reserve = c.benchmark_group("cycles/course_hash_dedup_64");
    course_hash_reserve.sample_size(200);
    course_hash_reserve.measurement_time(Duration::from_secs(3));
    course_hash_reserve.throughput(Throughput::Elements(course_bench::COURSE_HASH_COUNT as u64));
    for (name, reserved) in [("fold_hash_growing", false), ("fold_hash_bounded_8", true)] {
        course_hash_reserve.bench_function(name, |b| {
            b.iter(|| {
                let values = black_box(&course_summary_hashes);
                black_box(if reserved {
                    rssp::course::profile_dedup_hashes_reserved(values)
                } else {
                    rssp::course::profile_dedup_hashes(values, false)
                });
            });
        });
    }
    course_hash_reserve.finish();

    let mut typical_hashes = c.benchmark_group("cycles/course_hash_dedup_typical_64");
    typical_hashes.sample_size(200);
    typical_hashes.measurement_time(Duration::from_secs(3));
    typical_hashes.throughput(Throughput::Elements(course_bench::COURSE_HASH_COUNT as u64));
    typical_hashes.bench_function("fold_hash_bounded_8", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_dedup_hashes_reserved(black_box(
                &typical_course_hashes,
            )));
        });
    });
    typical_hashes.bench_function("adaptive_linear", |b| {
        b.iter(|| {
            black_box(rssp::course::profile_dedup_hashes_adaptive(black_box(
                &typical_course_hashes,
            )));
        });
    });
    typical_hashes.finish();

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
    course_parse.bench_function("sequential_tag_dispatch", |b| {
        b.iter(|| {
            black_box(
                rssp::course::profile_parse_crs_dispatch(black_box(&course_input), true)
                    .expect("benchmark course should parse"),
            );
        });
    });
    course_parse.bench_function("indexed_tag_dispatch", |b| {
        b.iter(|| {
            black_box(
                rssp::course::profile_parse_crs_dispatch(black_box(&course_input), false)
                    .expect("benchmark course should parse"),
            );
        });
    });
    course_parse.finish();

    course_bench::assert_parse_reserve_behavior();
    let reserve_typical = course_bench::parse_input(course_bench::PARSE_TYPICAL_COUNT);
    let reserve_large = course_bench::parse_input(course_bench::PARSE_LARGE_COUNT);
    for (name, input, entry_count) in [
        (
            "cycles/course_entry_reserve_fixed_10",
            reserve_typical.as_slice(),
            course_bench::PARSE_TYPICAL_COUNT,
        ),
        (
            "cycles/course_entry_reserve_fixed_256",
            reserve_large.as_slice(),
            course_bench::PARSE_LARGE_COUNT,
        ),
    ] {
        let mut group = c.benchmark_group(name);
        group.sample_size(100);
        group.measurement_time(Duration::from_secs(3));
        group.throughput(Throughput::Elements(entry_count as u64));
        for (phase, legacy) in [("growing_vec", true), ("presized_vec", false)] {
            group.bench_function(phase, |b| {
                b.iter(|| {
                    black_box(course_bench::parse_reserved(black_box(input), legacy));
                });
            });
        }
        group.finish();
    }

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

    let mut course_nps = c.benchmark_group("cycles/course_nps_capacity");
    course_nps.sample_size(20);
    course_nps.measurement_time(Duration::from_secs(3));
    course_nps.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    for (name, prealloc_nps) in [("growing", false), ("preallocated", true)] {
        course_nps.bench_function(name, |b| {
            b.iter(|| {
                black_box(
                    rssp::course::profile_course_nps(
                        black_box(repeated_course.course_path()),
                        Some(black_box(repeated_course.songs_dir())),
                        black_box("dance-single"),
                        black_box("Medium"),
                        black_box(course_options.clone()),
                        prealloc_nps,
                    )
                    .expect("NPS capacity benchmark course should analyze"),
                );
            });
        });
    }
    course_nps.finish();

    let mut course_dir_check = c.benchmark_group("cycles/course_catalog_dir_check");
    course_dir_check.sample_size(20);
    course_dir_check.measurement_time(Duration::from_secs(3));
    course_dir_check.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    for (name, trust_catalog) in [("recheck_dir", false), ("trust_catalog", true)] {
        course_dir_check.bench_function(name, |b| {
            b.iter(|| {
                black_box(
                    rssp::course::profile_catalog_dirs(
                        black_box(course_fixture.course_path()),
                        Some(black_box(course_fixture.songs_dir())),
                        black_box("dance-single"),
                        black_box("Medium"),
                        black_box(course_options.clone()),
                        trust_catalog,
                    )
                    .expect("catalog directory benchmark course should analyze"),
                );
            });
        });
    }
    course_dir_check.finish();

    let mut course_catalog = c.benchmark_group("cycles/course_group_catalog");
    course_catalog.sample_size(20);
    course_catalog.measurement_time(Duration::from_secs(3));
    course_catalog.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    for (name, group_catalog) in [("catalog_off", false), ("catalog_on", true)] {
        course_catalog.bench_function(name, |b| {
            b.iter(|| {
                black_box(
                    rssp::course::profile_analyze_catalog(
                        black_box(course_fixture.course_path()),
                        Some(black_box(course_fixture.songs_dir())),
                        black_box("dance-single"),
                        black_box("Medium"),
                        black_box(course_options.clone()),
                        group_catalog,
                    )
                    .expect("group catalog benchmark course should analyze"),
                );
            });
        });
    }
    course_catalog.finish();

    let mut course_groups = c.benchmark_group("cycles/course_group_cache");
    course_groups.sample_size(20);
    course_groups.measurement_time(Duration::from_secs(3));
    course_groups.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    for (name, group_cache) in [("last_group_off", false), ("last_group_on", true)] {
        course_groups.bench_function(name, |b| {
            b.iter(|| {
                black_box(
                    rssp::course::profile_analyze_groups(
                        black_box(course_fixture.course_path()),
                        Some(black_box(course_fixture.songs_dir())),
                        black_box("dance-single"),
                        black_box("Medium"),
                        black_box(course_options.clone()),
                        group_cache,
                    )
                    .expect("group cache benchmark course should analyze"),
                );
            });
        });
    }
    course_groups.finish();

    let mut course_keys = c.benchmark_group("cycles/course_repeat_cache");
    course_keys.sample_size(20);
    course_keys.measurement_time(Duration::from_secs(3));
    course_keys.throughput(Throughput::Elements(course_bench::SONG_COUNT as u64));
    for (name, song_key_cache) in [("path_key", false), ("song_key", true)] {
        course_keys.bench_function(name, |b| {
            b.iter(|| {
                black_box(
                    rssp::course::profile_analyze_crs(
                        black_box(repeated_course.course_path()),
                        Some(black_box(repeated_course.songs_dir())),
                        black_box("dance-single"),
                        black_box("Medium"),
                        black_box(course_options.clone()),
                        song_key_cache,
                    )
                    .expect("repeated benchmark course should analyze"),
                );
            });
        });
    }
    course_keys.finish();

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

    let mut pack_ini = c.benchmark_group("cycles/pack_ini_parse");
    pack_ini.sample_size(100);
    pack_ini.measurement_time(Duration::from_secs(3));
    pack_ini.throughput(Throughput::Bytes(pack_bench::PACK_INI_INPUT.len() as u64));
    pack_ini.bench_function("owned_fields", |b| {
        b.iter(|| {
            black_box(rssp::pack::profile_parse_pack_ini(
                black_box(pack_bench::PACK_INI_INPUT),
                true,
            ))
        });
    });
    pack_ini.bench_function("sequential_key_dispatch", |b| {
        b.iter(|| {
            black_box(rssp::pack::profile_parse_pack_ini_dispatch(
                black_box(pack_bench::PACK_INI_INPUT),
                true,
            ))
        });
    });
    pack_ini.bench_function("indexed_key_dispatch", |b| {
        b.iter(|| {
            black_box(rssp::pack::profile_parse_pack_ini_dispatch(
                black_box(pack_bench::PACK_INI_INPUT),
                false,
            ))
        });
    });
    pack_ini.finish();

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
    let duplicate_opt = rssp::pack::ScanOpt {
        dup: rssp::pack::DupPolicy::Error,
    };
    song_scan.bench_function("joined_paths_error", |b| {
        b.iter(|| {
            black_box(rssp::profile::scan_song_dir_joined_paths(
                black_box(pack_fixture.song_dir()),
                black_box(duplicate_opt),
            ))
        });
    });
    song_scan.bench_function("deferred_paths_error", |b| {
        b.iter(|| {
            black_box(rssp::pack::scan_song_dir(
                black_box(pack_fixture.song_dir()),
                black_box(duplicate_opt),
            ))
        });
    });
    song_scan.finish();

    let mut strict_single = c.benchmark_group("cycles/song_simfile_strict_single");
    strict_single.sample_size(20);
    strict_single.measurement_time(Duration::from_secs(3));
    strict_single.throughput(Throughput::Elements(
        pack_bench::SINGLE_SONG_ENTRY_COUNT as u64,
    ));
    strict_single.bench_function("growing_names", |b| {
        b.iter(|| {
            black_box(rssp::profile::scan_song_dir_growing_names(
                black_box(pack_fixture.single_song_dir()),
                black_box(duplicate_opt),
            ))
        });
    });
    strict_single.bench_function("inline_first", |b| {
        b.iter(|| {
            black_box(rssp::pack::scan_song_dir(
                black_box(pack_fixture.single_song_dir()),
                black_box(duplicate_opt),
            ))
        });
    });
    strict_single.finish();

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

    let mut background_path = c.benchmark_group("cycles/background_path_join");
    background_path.sample_size(100);
    background_path.measurement_time(Duration::from_secs(3));
    background_path.throughput(Throughput::Elements(1));
    background_path.bench_function("growing", |b| {
        b.iter(|| {
            black_box(rssp::profile::relative_path_join(
                black_box(asset_fixture.song_dir()),
                black_box("Visuals/Background,Layer.png"),
                false,
            ))
        });
    });
    background_path.bench_function("preallocated", |b| {
        b.iter(|| {
            black_box(rssp::profile::relative_path_join(
                black_box(asset_fixture.song_dir()),
                black_box("Visuals/Background,Layer.png"),
                true,
            ))
        });
    });
    background_path.finish();
    print_path_join_pairs(asset_fixture.song_dir());

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
    background.bench_function("materialized_values", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_materialized(
                black_box(asset_fixture.song_dir()),
                black_box(asset_fixture.simfile()),
            ))
        });
    });
    background.bench_function("always_sort", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_always_sort(
                black_box(asset_fixture.song_dir()),
                black_box(asset_fixture.simfile()),
            ))
        });
    });
    background.bench_function("growing_paths", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_growing_paths(
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

    let mut background_sort = c.benchmark_group("cycles/background_ordered_sort");
    background_sort.sample_size(100);
    background_sort.measurement_time(Duration::from_secs(2));
    background_sort.throughput(Throughput::Elements(
        assets_bench::ORDERED_CHANGE_COUNT as u64,
    ));
    background_sort.bench_function("always_sort", |b| {
        b.iter(|| {
            rssp::profile::sort_background_changes(
                black_box(&mut ordered_sort_legacy),
                black_box(true),
                true,
            );
        });
    });
    background_sort.bench_function("ordered_fast_path", |b| {
        b.iter(|| {
            rssp::profile::sort_background_changes(
                black_box(&mut ordered_sort_current),
                black_box(true),
                false,
            );
        });
    });
    background_sort.finish();

    let mut background_catalog = c.benchmark_group("cycles/background_catalog_entry_type");
    background_catalog.sample_size(20);
    background_catalog.measurement_time(Duration::from_secs(3));
    background_catalog.throughput(Throughput::Elements(assets_bench::CHANGE_COUNT as u64));
    background_catalog.bench_function("path_metadata", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_path_metadata(
                black_box(asset_fixture.song_dir()),
                black_box(asset_fixture.simfile()),
            ))
        });
    });
    background_catalog.bench_function("cached_entry_type", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_background_changes_like_itg(
                black_box(asset_fixture.song_dir()),
                black_box(asset_fixture.simfile()),
            ))
        });
    });
    background_catalog.finish();

    let mut background_order = c.benchmark_group("cycles/background_change_order");
    background_order.sample_size(20);
    background_order.measurement_time(Duration::from_secs(3));
    background_order.throughput(Throughput::Elements(
        assets_bench::UNORDERED_PAIR_COUNT as u64,
    ));
    background_order.bench_function("linear_upsert", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_linear_upsert(
                black_box(asset_fixture.song_dir()),
                black_box(&unordered_bgchanges),
            ))
        });
    });
    background_order.bench_function("growing_paths", |b| {
        b.iter(|| {
            black_box(rssp::profile::background_changes_growing_paths(
                black_box(asset_fixture.song_dir()),
                black_box(&unordered_bgchanges),
            ))
        });
    });
    background_order.bench_function("filtered_upsert", |b| {
        b.iter(|| {
            black_box(rssp::assets::resolve_background_changes_like_itg(
                black_box(asset_fixture.song_dir()),
                black_box(&unordered_bgchanges),
            ))
        });
    });
    background_order.finish();

    let mut bg_values = c.benchmark_group("cycles/background_change_values");
    bg_values.sample_size(100);
    bg_values.measurement_time(Duration::from_secs(3));
    bg_values.throughput(Throughput::Elements(assets_bench::BG_TAG_COUNT as u64));
    bg_values.bench_function("materialized", |b| {
        b.iter(|| {
            black_box(rssp::parse::extract_bgchanges_values(black_box(
                &bgchange_tags,
            )))
        });
    });
    bg_values.bench_function("streamed", |b| {
        b.iter(|| {
            let count = rssp::parse::bgchanges_values(black_box(&bgchange_tags)).count();
            black_box(count)
        });
    });
    bg_values.finish();

    let mut relative_assets = c.benchmark_group("cycles/asset_relative_paths");
    relative_assets.sample_size(20);
    relative_assets.measurement_time(Duration::from_secs(3));
    relative_assets.throughput(Throughput::Elements(assets_bench::REL_PATH_COUNT as u64));
    for (name, legacy) in [
        ("materialized_components", true),
        ("inline_components", false),
    ] {
        relative_assets.bench_function(name, |b| {
            b.iter(|| {
                let mut found = 0usize;
                for path in black_box(&relative_asset_paths) {
                    found += usize::from(
                        rssp::profile::relative_asset_path(
                            black_box(asset_fixture.relative_dir()),
                            black_box(path),
                            legacy,
                        )
                        .is_some(),
                    );
                }
                black_box(found);
            });
        });
    }
    relative_assets.finish();

    let mut relative_components = c.benchmark_group("cycles/asset_relative_components");
    relative_components.sample_size(100);
    relative_components.measurement_time(Duration::from_secs(3));
    relative_components.throughput(Throughput::Bytes(relative_component_bytes as u64));
    for (name, legacy) in [
        ("materialized_components", true),
        ("inline_components", false),
    ] {
        relative_components.bench_function(name, |b| {
            b.iter(|| {
                let checksum =
                    black_box(&relative_component_paths)
                        .iter()
                        .fold(0u64, |checksum, path| {
                            checksum.rotate_left(1)
                                ^ rssp::profile::relative_asset_parts_hash(black_box(path), legacy)
                        });
                black_box(checksum);
            });
        });
    }
    relative_components.finish();

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
    analysis.bench_function("reused_bpm_owned_timing", |b| {
        b.iter(|| {
            black_box(
                rssp::profile::analyze_owned_timing(
                    black_box(analysis_fixture),
                    black_box("ssc"),
                    black_box(&analysis_options),
                    black_box(&mut owned_timing_scratch),
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

    let mut timing_pipelines = c.benchmark_group("cycles/timing_map_pipelines");
    timing_pipelines.sample_size(20);
    timing_pipelines.measurement_time(Duration::from_secs(3));
    timing_pipelines.throughput(Throughput::Bytes(nps_fixture.len() as u64));
    timing_pipelines.bench_function("duration_owned", |b| {
        b.iter(|| {
            black_box(
                rssp::duration::chart_durations_owned(
                    black_box(nps_fixture),
                    black_box("ssc"),
                    rssp::TimingOffsets::default(),
                )
                .expect("fixture should analyze"),
            );
        });
    });
    timing_pipelines.bench_function("duration_borrowed", |b| {
        b.iter(|| {
            black_box(
                rssp::compute_chart_durations(
                    black_box(nps_fixture),
                    black_box("ssc"),
                    rssp::TimingOffsets::default(),
                )
                .expect("fixture should analyze"),
            );
        });
    });
    timing_pipelines.bench_function("nps_owned", |b| {
        b.iter(|| {
            black_box(
                rssp::nps::chart_peak_nps_owned(black_box(nps_fixture), black_box("ssc"))
                    .expect("fixture should analyze"),
            );
        });
    });
    timing_pipelines.bench_function("nps_borrowed", |b| {
        b.iter(|| {
            black_box(
                rssp::compute_chart_peak_nps(black_box(nps_fixture), black_box("ssc"))
                    .expect("fixture should analyze"),
            );
        });
    });
    timing_pipelines.finish();

    let note_data = step_parity_bench::note_data();
    step_parity_bench::assert_note_data_behavior(&note_data);
    print_note_data_pairs(&note_data, false);
    print_note_data_pairs(&note_data, true);
    let mut note_parse = c.benchmark_group("cycles/parity_note_parse_2048");
    note_parse.sample_size(100);
    note_parse.measurement_time(Duration::from_secs(3));
    note_parse.throughput(Throughput::Elements(
        step_parity_bench::NOTE_DATA_ROW_COUNT as u64,
    ));
    for (name, fused) in [("materialized", false), ("fused", true)] {
        note_parse.bench_function(name, |b| {
            b.iter(|| {
                black_box(rssp::step_parity::parse_notes_for_bench(
                    black_box(&note_data),
                    4,
                    fused,
                ));
            });
        });
    }
    note_parse.finish();

    let mut note_analysis = c.benchmark_group("cycles/parity_note_analysis_2048");
    note_analysis.sample_size(30);
    note_analysis.measurement_time(Duration::from_secs(3));
    note_analysis.throughput(Throughput::Elements(
        step_parity_bench::NOTE_DATA_ROW_COUNT as u64,
    ));
    for (name, fused) in [("materialized", false), ("fused", true)] {
        note_analysis.bench_function(name, |b| {
            b.iter(|| {
                black_box(rssp::step_parity::analyze_note_data_for_bench(
                    black_box(&note_data),
                    4,
                    fused,
                ));
            });
        });
    }
    note_analysis.finish();

    assert!(rssp::step_parity::perm_builds_match_for_bench(4));
    assert!(rssp::step_parity::perm_builds_match_for_bench(8));
    let mut parity_cache = c.benchmark_group("cycles/step_parity_cache");
    parity_cache.sample_size(50);
    parity_cache.measurement_time(Duration::from_secs(3));
    parity_cache.throughput(Throughput::Elements(16));
    parity_cache.bench_function("legacy_single", |b| {
        b.iter(|| black_box(rssp::step_parity::legacy_perm_build_for_bench(black_box(4))));
    });
    parity_cache.bench_function("packed_single", |b| {
        b.iter(|| black_box(rssp::step_parity::packed_perm_build_for_bench(black_box(4))));
    });
    parity_cache.throughput(Throughput::Elements(256));
    parity_cache.bench_function("legacy_double", |b| {
        b.iter(|| black_box(rssp::step_parity::legacy_perm_build_for_bench(black_box(8))));
    });
    parity_cache.bench_function("packed_double", |b| {
        b.iter(|| black_box(rssp::step_parity::packed_perm_build_for_bench(black_box(8))));
    });
    parity_cache.finish();

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
    let double_tap_rows = step_parity_bench::rows::<8>(
        step_parity_bench::DOUBLE_ROW_COUNT,
        &step_parity_bench::DOUBLE_MASKS[..4],
    );
    let double_beats = step_parity_bench::beats(step_parity_bench::DOUBLE_ROW_COUNT);
    let double_hold_rows = step_parity_bench::hold_rows::<8>(
        step_parity_bench::DOUBLE_ROW_COUNT,
        step_parity_bench::DOUBLE_MASKS,
    );
    let mut single_scratch =
        rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
    let mut wide_hold_scratch = rssp::step_parity::wide_hold_timing_rows_scratch::<4>()
        .expect("dance-single parity layout");
    let mut dense_hold_scratch =
        rssp::step_parity::dense_hold_timing_scratch::<4>().expect("dance-single parity layout");
    let mut legacy_tap_scratch =
        rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
    let mut current_tap_scratch =
        rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
    let mut legacy_hash_scratch =
        rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
    let mut folded_hash_scratch =
        rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
    let mut double_scratch =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut scalar_double =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut chunked_double =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut scalar_double_holds =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut chunked_double_holds =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut materialized_double =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut packed_double =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut materialized_double_holds =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut packed_double_holds =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut general_double_taps =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut direct_double_taps =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut general_double_key =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut direct_double_key =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut general_double_hold_key =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut direct_double_hold_key =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut general_double_tap_cost =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut direct_double_tap_cost =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut general_double_cost =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut direct_double_cost =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut general_double_hold_cost =
        rssp::step_parity::timing_rows_scratch::<8>().expect("dance-double parity layout");
    let mut direct_double_hold_cost =
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
    let legacy_tap_counts = rssp::step_parity::analyze_timing_rows_tap_path_for_bench(
        &single_rows,
        &single_beats,
        &parity_timing,
        false,
        true,
        &mut legacy_tap_scratch,
    );
    let current_tap_counts = rssp::step_parity::analyze_timing_rows_tap_path_for_bench(
        &single_rows,
        &single_beats,
        &parity_timing,
        false,
        false,
        &mut current_tap_scratch,
    );
    assert_eq!(current_tap_counts, legacy_tap_counts);
    let legacy_hash_counts = rssp::step_parity::analyze_timing_rows_hash_for_bench(
        &single_hold_rows,
        &single_beats,
        &parity_timing,
        true,
        true,
        &mut legacy_hash_scratch,
    );
    let folded_hash_counts = rssp::step_parity::analyze_timing_rows_hash_for_bench(
        &single_hold_rows,
        &single_beats,
        &parity_timing,
        true,
        false,
        &mut folded_hash_scratch,
    );
    assert_eq!(folded_hash_counts, legacy_hash_counts);
    assert_eq!(
        rssp::step_parity::analyze_double_decode_for_bench(
            &double_rows,
            &double_beats,
            &parity_timing,
            false,
            true,
            &mut scalar_double,
        ),
        rssp::step_parity::analyze_double_decode_for_bench(
            &double_rows,
            &double_beats,
            &parity_timing,
            false,
            false,
            &mut chunked_double,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_double_decode_for_bench(
            &double_hold_rows,
            &double_beats,
            &parity_timing,
            true,
            true,
            &mut scalar_double_holds,
        ),
        rssp::step_parity::analyze_double_decode_for_bench(
            &double_hold_rows,
            &double_beats,
            &parity_timing,
            true,
            false,
            &mut chunked_double_holds,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_double_result_for_bench(
            &double_rows,
            &double_beats,
            &parity_timing,
            false,
            true,
            &mut materialized_double,
        ),
        rssp::step_parity::analyze_double_result_for_bench(
            &double_rows,
            &double_beats,
            &parity_timing,
            false,
            false,
            &mut packed_double,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_double_tap_key_for_bench(
            &double_tap_rows,
            &double_beats,
            &parity_timing,
            false,
            true,
            &mut general_double_taps,
        ),
        rssp::step_parity::analyze_double_tap_key_for_bench(
            &double_tap_rows,
            &double_beats,
            &parity_timing,
            false,
            false,
            &mut direct_double_taps,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_double_tap_cost_for_bench(
            &double_tap_rows,
            &double_beats,
            &parity_timing,
            false,
            true,
            &mut general_double_tap_cost,
        ),
        rssp::step_parity::analyze_double_tap_cost_for_bench(
            &double_tap_rows,
            &double_beats,
            &parity_timing,
            false,
            false,
            &mut direct_double_tap_cost,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_double_tap_cost_for_bench(
            &double_rows,
            &double_beats,
            &parity_timing,
            false,
            true,
            &mut general_double_cost,
        ),
        rssp::step_parity::analyze_double_tap_cost_for_bench(
            &double_rows,
            &double_beats,
            &parity_timing,
            false,
            false,
            &mut direct_double_cost,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_double_tap_cost_for_bench(
            &double_hold_rows,
            &double_beats,
            &parity_timing,
            true,
            true,
            &mut general_double_hold_cost,
        ),
        rssp::step_parity::analyze_double_tap_cost_for_bench(
            &double_hold_rows,
            &double_beats,
            &parity_timing,
            true,
            false,
            &mut direct_double_hold_cost,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_double_tap_key_for_bench(
            &double_rows,
            &double_beats,
            &parity_timing,
            false,
            true,
            &mut general_double_key,
        ),
        rssp::step_parity::analyze_double_tap_key_for_bench(
            &double_rows,
            &double_beats,
            &parity_timing,
            false,
            false,
            &mut direct_double_key,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_double_tap_key_for_bench(
            &double_hold_rows,
            &double_beats,
            &parity_timing,
            true,
            true,
            &mut general_double_hold_key,
        ),
        rssp::step_parity::analyze_double_tap_key_for_bench(
            &double_hold_rows,
            &double_beats,
            &parity_timing,
            true,
            false,
            &mut direct_double_hold_key,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_double_result_for_bench(
            &double_hold_rows,
            &double_beats,
            &parity_timing,
            true,
            true,
            &mut materialized_double_holds,
        ),
        rssp::step_parity::analyze_double_result_for_bench(
            &double_hold_rows,
            &double_beats,
            &parity_timing,
            true,
            false,
            &mut packed_double_holds,
        ),
    );
    let arena_warm_len = step_parity_bench::SINGLE_ROW_COUNT / 8;
    let mut sampled_arena =
        rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
    let mut learned_arena =
        rssp::step_parity::timing_rows_scratch::<4>().expect("dance-single parity layout");
    let _ = rssp::step_parity::analyze_arena_for_bench(
        &single_rows[..arena_warm_len],
        &single_beats[..arena_warm_len],
        &parity_timing,
        false,
        true,
        &mut sampled_arena,
    );
    let _ = rssp::step_parity::analyze_arena_for_bench(
        &single_rows[..arena_warm_len],
        &single_beats[..arena_warm_len],
        &parity_timing,
        false,
        false,
        &mut learned_arena,
    );
    assert_eq!(
        rssp::step_parity::analyze_arena_for_bench(
            &single_rows,
            &single_beats,
            &parity_timing,
            false,
            true,
            &mut sampled_arena,
        ),
        rssp::step_parity::analyze_arena_for_bench(
            &single_rows,
            &single_beats,
            &parity_timing,
            false,
            false,
            &mut learned_arena,
        ),
    );
    let mut growing_scratch =
        rssp::step_parity::growing_timing_scratch::<4>().expect("dance-single parity layout");
    assert_eq!(
        rssp::step_parity::analyze_growing_for_bench(
            &single_rows,
            &single_beats,
            &parity_timing,
            false,
            &mut growing_scratch,
        ),
        rssp::step_parity::analyze_timing_rows_known_holds(
            &single_rows,
            &single_beats,
            &parity_timing,
            false,
            &mut single_scratch,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_timing_rows_wide_holds_for_bench(
            &single_hold_rows,
            &single_beats,
            &parity_timing,
            true,
            &mut wide_hold_scratch,
        ),
        rssp::step_parity::analyze_timing_rows_known_holds(
            &single_hold_rows,
            &single_beats,
            &parity_timing,
            true,
            &mut single_scratch,
        ),
    );
    assert_eq!(
        rssp::step_parity::analyze_dense_holds_for_bench(
            &single_hold_rows,
            &single_beats,
            &parity_timing,
            true,
            &mut dense_hold_scratch,
        ),
        rssp::step_parity::analyze_timing_rows_known_holds(
            &single_hold_rows,
            &single_beats,
            &parity_timing,
            true,
            &mut single_scratch,
        ),
    );
    if std::env::args().any(|arg| arg.contains("dense_single_holds_hash")) {
        print_hash_pairs(
            &single_hold_rows,
            &single_beats,
            &parity_timing,
            &mut legacy_hash_scratch,
            &mut folded_hash_scratch,
        );
    }

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
    parity.bench_function("dense_single_tap_path_legacy", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_tap_path_for_bench(
                black_box(&single_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                false,
                true,
                black_box(&mut legacy_tap_scratch),
            ));
        });
    });
    parity.bench_function("dense_single_tap_path_specialized", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_tap_path_for_bench(
                black_box(&single_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                false,
                false,
                black_box(&mut current_tap_scratch),
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
    parity.bench_function("dense_single_cold_growing_workspace", |b| {
        b.iter(|| {
            let mut scratch = rssp::step_parity::growing_timing_scratch::<4>()
                .expect("dance-single parity layout");
            black_box(rssp::step_parity::analyze_growing_for_bench(
                black_box(&single_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                false,
                black_box(&mut scratch),
            ));
        });
    });
    for (name, legacy_growth) in [
        ("dense_single_growth_sampled", true),
        ("dense_single_growth_learned", false),
    ] {
        parity.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut scratch = rssp::step_parity::timing_rows_scratch::<4>()
                        .expect("dance-single parity layout");
                    let _ = rssp::step_parity::analyze_arena_for_bench(
                        &single_rows[..arena_warm_len],
                        &single_beats[..arena_warm_len],
                        &parity_timing,
                        false,
                        legacy_growth,
                        &mut scratch,
                    );
                    scratch
                },
                |mut scratch| {
                    black_box(rssp::step_parity::analyze_arena_for_bench(
                        black_box(&single_rows),
                        black_box(&single_beats),
                        black_box(&parity_timing),
                        false,
                        legacy_growth,
                        black_box(&mut scratch),
                    ));
                },
                BatchSize::SmallInput,
            );
        });
    }
    for (name, legacy_growth) in [
        ("dense_single_holds_growth_sampled", true),
        ("dense_single_holds_growth_learned", false),
    ] {
        parity.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut scratch = rssp::step_parity::timing_rows_scratch::<4>()
                        .expect("dance-single parity layout");
                    let _ = rssp::step_parity::analyze_arena_for_bench(
                        &single_hold_rows[..arena_warm_len],
                        &single_beats[..arena_warm_len],
                        &parity_timing,
                        true,
                        legacy_growth,
                        &mut scratch,
                    );
                    scratch
                },
                |mut scratch| {
                    black_box(rssp::step_parity::analyze_arena_for_bench(
                        black_box(&single_hold_rows),
                        black_box(&single_beats),
                        black_box(&parity_timing),
                        true,
                        legacy_growth,
                        black_box(&mut scratch),
                    ));
                },
                BatchSize::SmallInput,
            );
        });
    }
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
    parity.bench_function("dense_single_holds_wide_storage", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_wide_holds_for_bench(
                black_box(&single_hold_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                true,
                black_box(&mut wide_hold_scratch),
            ));
        });
    });
    parity.bench_function("dense_single_holds_dense_storage", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_dense_holds_for_bench(
                black_box(&single_hold_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                true,
                black_box(&mut dense_hold_scratch),
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
    parity.bench_function("dense_single_holds_hash_legacy", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_hash_for_bench(
                black_box(&single_hold_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                true,
                true,
                black_box(&mut legacy_hash_scratch),
            ));
        });
    });
    parity.bench_function("dense_single_holds_hash_folded", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_timing_rows_hash_for_bench(
                black_box(&single_hold_rows),
                black_box(&single_beats),
                black_box(&parity_timing),
                true,
                false,
                black_box(&mut folded_hash_scratch),
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
    parity.bench_function("dense_double_decode_scalar", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_decode_for_bench(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                true,
                black_box(&mut scalar_double),
            ));
        });
    });
    parity.bench_function("dense_double_decode_chunked", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_decode_for_bench(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                false,
                black_box(&mut chunked_double),
            ));
        });
    });
    parity.bench_function("dense_double_result_materialized", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_result_for_bench(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                true,
                black_box(&mut materialized_double),
            ));
        });
    });
    parity.bench_function("dense_double_result_packed", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_result_for_bench(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                false,
                black_box(&mut packed_double),
            ));
        });
    });
    parity.bench_function("dense_double_tap_key_general", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_key_for_bench(
                black_box(&double_tap_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                true,
                black_box(&mut general_double_taps),
            ));
        });
    });
    parity.bench_function("dense_double_tap_key_direct", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_key_for_bench(
                black_box(&double_tap_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                false,
                black_box(&mut direct_double_taps),
            ));
        });
    });
    parity.bench_function("dense_double_tap_cost_general", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_cost_for_bench(
                black_box(&double_tap_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                true,
                black_box(&mut general_double_tap_cost),
            ));
        });
    });
    parity.bench_function("dense_double_tap_cost_direct", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_cost_for_bench(
                black_box(&double_tap_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                false,
                black_box(&mut direct_double_tap_cost),
            ));
        });
    });
    parity.bench_function("dense_double_mixed_tap_key_general", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_key_for_bench(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                true,
                black_box(&mut general_double_key),
            ));
        });
    });
    parity.bench_function("dense_double_mixed_tap_key_direct", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_key_for_bench(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                false,
                black_box(&mut direct_double_key),
            ));
        });
    });
    parity.bench_function("dense_double_mixed_tap_cost_general", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_cost_for_bench(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                true,
                black_box(&mut general_double_cost),
            ));
        });
    });
    parity.bench_function("dense_double_mixed_tap_cost_direct", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_cost_for_bench(
                black_box(&double_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                false,
                false,
                black_box(&mut direct_double_cost),
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
    parity.bench_function("dense_double_holds_decode_scalar", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_decode_for_bench(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                true,
                black_box(&mut scalar_double_holds),
            ));
        });
    });
    parity.bench_function("dense_double_holds_decode_chunked", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_decode_for_bench(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                false,
                black_box(&mut chunked_double_holds),
            ));
        });
    });
    parity.bench_function("dense_double_holds_result_materialized", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_result_for_bench(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                true,
                black_box(&mut materialized_double_holds),
            ));
        });
    });
    parity.bench_function("dense_double_holds_result_packed", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_result_for_bench(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                false,
                black_box(&mut packed_double_holds),
            ));
        });
    });
    parity.bench_function("dense_double_holds_tap_key_general", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_key_for_bench(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                true,
                black_box(&mut general_double_hold_key),
            ));
        });
    });
    parity.bench_function("dense_double_holds_tap_key_direct", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_key_for_bench(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                false,
                black_box(&mut direct_double_hold_key),
            ));
        });
    });
    parity.bench_function("dense_double_holds_tap_cost_general", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_cost_for_bench(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                true,
                black_box(&mut general_double_hold_cost),
            ));
        });
    });
    parity.bench_function("dense_double_holds_tap_cost_direct", |b| {
        b.iter(|| {
            black_box(rssp::step_parity::analyze_double_tap_cost_for_bench(
                black_box(&double_hold_rows),
                black_box(&double_beats),
                black_box(&parity_timing),
                true,
                false,
                black_box(&mut direct_double_hold_cost),
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
