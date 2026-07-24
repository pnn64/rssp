#[cfg(windows)]
use criterion::measurement::{Measurement, ValueFormatter};
#[cfg(windows)]
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
#[cfg(windows)]
use std::hint::black_box;
#[cfg(windows)]
use std::time::Duration;

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
        let cpu = if cpu_count > 2 { 2 } else { 0 };
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
fn bench_cycles(c: &mut Criterion<ThreadCycles>) {
    const ENTRIES: usize = 4_096;
    platform::stabilize_thread();

    let pair_map = large_pair_map(ENTRIES);
    let speed_map = large_speed_map(ENTRIES);
    let segments = rssp::timing::TimingSegments {
        beat0_offset_adjust: 0.0,
        bpms: vec![(0.0, 120.0)],
        stops: (0..ENTRIES).map(|idx| (idx as f32 * 4.0, 0.125)).collect(),
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
                black_box(&segments),
            ));
        });
    });
    cleanup.finish();
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
