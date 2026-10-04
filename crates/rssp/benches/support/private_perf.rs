// Explicit ignored benchmarks run alone; fixture setup is outside measurement.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[path = "course_fixtures.rs"]
pub mod fixtures;

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

pub fn measure(name: &str, items: usize, mut run: impl FnMut()) {
    measure_prepared(name, items, || (), |()| run());
}

pub fn measure_prepared<T>(
    name: &str,
    items: usize,
    mut setup: impl FnMut() -> T,
    mut run: impl FnMut(&mut T),
) {
    if std::env::var("RSSP_PASS_FILTER").is_ok_and(|filter| !name.contains(&filter)) {
        return;
    }
    let iters = std::env::var("RSSP_PASS_ITERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(4000);
    cpu::pin();
    for _ in 0..4 {
        run(&mut setup());
    }
    let mut times = [0.0; 7];
    let mut cycles = [0.0; 7];
    for (time, cycle) in times.iter_mut().zip(&mut cycles) {
        let mut inputs: Vec<_> = (0..iters).map(|_| setup()).collect();
        let start = Instant::now();
        let before = cpu::cycles();
        for input in &mut inputs {
            run(input);
        }
        *cycle = (cpu::cycles() - before) as f64 / iters as f64;
        *time = start.elapsed().as_nanos() as f64 / iters as f64;
    }
    times.sort_unstable_by(f64::total_cmp);
    cycles.sort_unstable_by(f64::total_cmp);
    ALLOCS.store(0, Ordering::Relaxed);
    REALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    let mut input = setup();
    COUNT.store(true, Ordering::Relaxed);
    run(&mut input);
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
