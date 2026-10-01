// Timing, CPU cycles, and allocator churn are measured in separate passes.
#![allow(clippy::cast_precision_loss)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

struct CountingAllocator;
static COUNT: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

// SAFETY: Every operation forwards the caller's unchanged request to System.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies a valid layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && COUNT.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: ptr and layout are the allocation pair supplied by the caller.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: The caller supplies a valid allocation and new size.
        let ptr = unsafe { System.realloc(ptr, layout, size) };
        if !ptr.is_null() && COUNT.load(Ordering::Relaxed) {
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size as u64, Ordering::Relaxed);
        }
        ptr
    }
}

#[cfg(windows)]
mod cpu {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
        fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
    }

    pub fn pin() {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        let core = if cores > 2 { 2 } else { 0 };
        // SAFETY: This pseudo-handle is valid; the mask selects an available CPU.
        let prev = unsafe { SetThreadAffinityMask(GetCurrentThread(), 1usize << core) };
        assert_ne!(prev, 0, "SetThreadAffinityMask failed");
    }

    pub fn cycles() -> u64 {
        let mut cycles = 0;
        // SAFETY: The thread pseudo-handle and output storage are valid.
        let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &raw mut cycles) };
        assert_ne!(ok, 0, "QueryThreadCycleTime failed");
        cycles
    }
}

#[cfg(not(windows))]
mod cpu {
    pub fn pin() {}
    pub fn cycles() -> u64 {
        0
    }
}

fn measure(name: &str, items: usize, iters: usize, mut run: impl FnMut()) {
    if std::env::var("RSSP_HOT_FILTER").is_ok_and(|filter| !name.contains(&filter)) {
        return;
    }
    for _ in 0..4 {
        run();
    }
    let mut times = [0.0; 7];
    let mut cycles = [0.0; 7];
    for (time, cycle) in times.iter_mut().zip(&mut cycles) {
        let start = Instant::now();
        let before = cpu::cycles();
        for _ in 0..iters {
            run();
        }
        *cycle = (cpu::cycles() - before) as f64 / iters as f64;
        *time = start.elapsed().as_nanos() as f64 / iters as f64;
    }
    times.sort_unstable_by(f64::total_cmp);
    cycles.sort_unstable_by(f64::total_cmp);
    ALLOCS.store(0, Ordering::Relaxed);
    REALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    COUNT.store(true, Ordering::Relaxed);
    run();
    COUNT.store(false, Ordering::Relaxed);
    println!(
        "{name}: ns={:.0} cycles={:.0} items/s={:.0} allocs={} reallocs={} churn_bytes={}",
        times[3],
        cycles[3],
        items as f64 * 1e9 / times[3],
        ALLOCS.load(Ordering::Relaxed),
        REALLOCS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed)
    );
}

fn row_cases<const L: usize>(iters: usize) {
    for (name, phantom, measure_rows) in [
        ("taps", false, 16),
        ("dense_taps", false, 256),
        ("phantom", true, 16),
    ] {
        let mut data = Vec::with_capacity(4096 * (L + 1));
        for index in 0..4096 {
            let mut row = [b'0'; L];
            row[index % L] = b'1';
            if phantom && index % 16 == 0 {
                row[0] = b'2';
            }
            data.extend_from_slice(&row);
            data.push(b'\n');
            if index % measure_rows == measure_rows - 1 {
                data.extend_from_slice(b",\n");
            }
        }
        data.push(b';');
        let mut scratch = rssp::stats::TypedRowsScratch::<L>::default();
        measure(&format!("typed{L}/{name}"), 4096, iters, || {
            black_box(rssp::stats::minimize_rows_typed_in(
                black_box(&data),
                black_box(&mut scratch),
            ));
        });
        measure(&format!("typed{L}/cold_{name}"), 4096, iters, || {
            black_box(rssp::stats::minimize_rows_typed::<L>(black_box(&data)));
        });
        measure(&format!("direct{L}/{name}"), 4096, iters, || {
            black_box(rssp::stats::minimize_chart_count_rows(black_box(&data), L));
        });
    }
}

fn main() {
    cpu::pin();
    let iters = std::env::var("RSSP_HOT_ITERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200);
    row_cases::<4>(iters);
    row_cases::<8>(iters);
    let densities: Vec<_> = (0..16384).map(|i| [0, 16, 20, 24, 32][i % 5]).collect();
    for (name, step) in [("long_segments", 2048.0), ("short_segments", 4.0)] {
        let bpms: Vec<_> = (0..32)
            .map(|i| (f64::from(i) * step, 120.0 + f64::from(i % 8)))
            .collect();
        measure(&format!("matrix/{name}"), densities.len(), iters, || {
            black_box(rssp::matrix::compute_matrix_profile(
                black_box(&densities),
                black_box(&bpms),
            ));
        });
    }
    let data = include_bytes!("fixtures/camellia_mix.ssc");
    let opts = rssp::AnalysisOptions {
        mono_threshold: 6,
        ..Default::default()
    };
    let mut scratch = rssp::AnalysisScratch::default();
    measure("analyze/camellia", data.len(), (iters / 10).max(1), || {
        black_box(
            rssp::analyze_with_scratch(black_box(data), "ssc", black_box(&opts), &mut scratch)
                .expect("valid fixture"),
        );
    });
    let fast = rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..opts.clone()
    };
    measure(
        "analyze/fast_camellia",
        data.len(),
        (iters / 10).max(1),
        || {
            black_box(
                rssp::analyze_with_scratch(black_box(data), "ssc", black_box(&fast), &mut scratch)
                    .expect("valid fixture"),
            );
        },
    );
    let small = include_bytes!("fixtures/hash_fixture.ssc");
    measure("analyze/mixed_small", small.len(), iters, || {
        black_box(
            rssp::analyze_with_scratch(black_box(small), "ssc", black_box(&opts), &mut scratch)
                .expect("valid fixture"),
        );
    });
}
