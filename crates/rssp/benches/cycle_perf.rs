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
            Throughput::Elements(items) => (items as f64, "elem/cycle"),
            Throughput::Bytes(bytes) | Throughput::BytesDecimal(bytes) => {
                (bytes as f64, "bytes/cycle")
            }
            Throughput::Bits(bits) => (bits as f64, "bits/cycle"),
            Throughput::ElementsAndBytes { elements, .. } => (elements as f64, "elem/cycle"),
        };
        for value in values {
            *value = items / *value;
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

    pub fn stabilize_thread() {
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
fn bench_cycles(c: &mut Criterion<ThreadCycles>) {
    const ENTRIES: usize = 4_096;
    platform::stabilize_thread();

    let pair_map = large_pair_map(ENTRIES);
    let speed_map = large_speed_map(ENTRIES);
    let stop_map = large_stop_map(ENTRIES);
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

    let parity_timing = step_parity_bench::timing();
    let single_rows = step_parity_bench::rows::<4>(
        step_parity_bench::SINGLE_ROW_COUNT,
        step_parity_bench::SINGLE_MASKS,
    );
    let single_beats = step_parity_bench::beats(step_parity_bench::SINGLE_ROW_COUNT);
    let double_rows = step_parity_bench::rows::<8>(
        step_parity_bench::DOUBLE_ROW_COUNT,
        step_parity_bench::DOUBLE_MASKS,
    );
    let double_beats = step_parity_bench::beats(step_parity_bench::DOUBLE_ROW_COUNT);
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
