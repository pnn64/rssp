// Benchmark reporting intentionally converts bounded counters to floating point.
#![allow(clippy::cast_precision_loss)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "support/nps_cases.rs"]
mod nps_cases;

struct CountingAllocator;

static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static REALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Delegates the exact allocation request to the system allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` are the pair supplied by the caller.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: Delegates the exact reallocation request to the system allocator.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            REALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        new_ptr
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    alloc_calls: u64,
    realloc_calls: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
}

impl AllocSnapshot {
    fn capture() -> Self {
        Self {
            alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
            realloc_calls: REALLOC_CALLS.load(Ordering::Relaxed),
            alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            realloc_bytes: REALLOC_BYTES.load(Ordering::Relaxed),
        }
    }

    fn delta(self, start: Self) -> Self {
        Self {
            alloc_calls: self.alloc_calls - start.alloc_calls,
            realloc_calls: self.realloc_calls - start.realloc_calls,
            alloc_bytes: self.alloc_bytes - start.alloc_bytes,
            realloc_bytes: self.realloc_bytes - start.realloc_bytes,
        }
    }
}

fn measure(name: &str, iterations: usize, mut run: impl FnMut()) {
    let start = AllocSnapshot::capture();
    for _ in 0..iterations {
        run();
    }
    let total = AllocSnapshot::capture().delta(start);
    let n = iterations as f64;
    println!(
        "{name}: allocs={:.2} reallocs={:.2} alloc_bytes={:.2} realloc_bytes={:.2} per iteration",
        total.alloc_calls as f64 / n,
        total.realloc_calls as f64 / n,
        total.alloc_bytes as f64 / n,
        total.realloc_bytes as f64 / n,
    );
}

fn main() {
    let iterations = std::env::var("RSSP_ALLOC_ITERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100);

    for (name, values) in nps_cases::cases() {
        measure(&format!("nps/cold/{name}"), iterations, || {
            black_box(rssp::nps::get_nps_stats(black_box(&values)));
        });
        let mut scratch = Vec::new();
        black_box(rssp::nps::get_nps_stats_with_scratch(&values, &mut scratch));
        measure(&format!("nps/reused/{name}"), iterations, || {
            black_box(rssp::nps::get_nps_stats_with_scratch(
                black_box(&values),
                black_box(&mut scratch),
            ));
        });
    }
    let data = include_bytes!("fixtures/camellia_mix.ssc");
    let options = rssp::AnalysisOptions {
        mono_threshold: 6,
        ..rssp::AnalysisOptions::default()
    };

    // Initialize process-lifetime tables before measuring request-local work.
    let _ = rssp::analyze(data, "ssc", &options).expect("fixture should analyze");

    measure("parse", iterations, || {
        black_box(
            rssp::parse::extract_sections(black_box(data), "ssc").expect("fixture should parse"),
        );
    });
    measure("analyze", iterations, || {
        black_box(
            rssp::analyze(black_box(data), "ssc", black_box(&options))
                .expect("fixture should analyze"),
        );
    });

    let mut scratch = rssp::AnalysisScratch::default();
    measure("analyze_reused", iterations, || {
        black_box(
            rssp::analyze_with_scratch(
                black_box(data),
                "ssc",
                black_box(&options),
                black_box(&mut scratch),
            )
            .expect("fixture should analyze"),
        );
    });
}
