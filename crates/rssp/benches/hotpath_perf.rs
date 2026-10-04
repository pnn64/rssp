// Timing, CPU cycles, and allocator churn are measured in separate passes.
#![allow(clippy::cast_precision_loss)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
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

fn encode_rows<const L: usize>(rows: &[[u8; L]], measure_rows: usize) -> Vec<u8> {
    let mut text = Vec::with_capacity(rows.len() * (L + 1) + 2 * rows.len().div_ceil(measure_rows));
    for chunk in rows.chunks(measure_rows) {
        for row in chunk {
            text.extend_from_slice(row);
            text.push(b'\n');
        }
        text.extend_from_slice(b",\n");
    }
    text
}

fn timing_cases<const L: usize>(iters: usize) {
    let plain = timing_data("");
    let fake = timing_data("8=4,32=8,128=16,512=32");
    for (name, holds) in [("taps", false), ("holds", true), ("phantom", true)] {
        let rows: Vec<_> = (0..4096)
            .map(|index| {
                let mut row = [b'0'; L];
                row[1 + index % (L - 1)] = if index % 31 == 0 { b'L' } else { b'1' };
                if holds {
                    row[0] = match index % 16 {
                        0 => b'2',
                        2 => b'4',
                        6 if name == "phantom" => b'M',
                        7 | 15 => b'3',
                        _ => b'0',
                    };
                }
                row
            })
            .collect();
        let beats: Vec<_> = (0..4096).map(|index| index as f32 / 4.0).collect();
        let text = encode_rows(&rows, 16);
        for (tag, timing) in [("plain", &plain), ("fake", &fake)] {
            measure(
                &format!("timing{L}/{name}_{tag}_rows"),
                rows.len(),
                iters,
                || {
                    black_box(
                        rssp::stats::compute_timing_aware_stats_from_rows_with_row_to_beat(
                            black_box(&rows),
                            black_box(timing),
                            black_box(&beats),
                        ),
                    );
                },
            );
            measure(
                &format!("timing{L}/{name}_{tag}_text"),
                rows.len(),
                iters,
                || {
                    black_box(rssp::stats::compute_timing_aware_stats_with_row_to_beat(
                        black_box(&text),
                        L,
                        black_box(timing),
                        black_box(&beats),
                    ));
                },
            );
            if !holds {
                measure(
                    &format!("timing{L}/{name}_{tag}_no_hold_text"),
                    rows.len(),
                    iters,
                    || {
                        black_box(rssp::stats::compute_no_hold_stats(
                            black_box(&text),
                            L,
                            black_box(timing),
                            black_box(&beats),
                        ));
                    },
                );
                measure(
                    &format!("timing{L}/{name}_{tag}_no_holds"),
                    rows.len(),
                    iters,
                    || {
                        black_box(rssp::stats::compute_timing_aware_stats_no_holds_from_rows(
                            black_box(&rows),
                            black_box(timing),
                            black_box(&beats),
                        ));
                    },
                );
            }
        }
        measure(&format!("timing{L}/{name}_raw"), rows.len(), iters, || {
            black_box(rssp::stats::compute_timing_aware_stats(
                black_box(&text),
                L,
                black_box(&fake),
            ));
        });
        let dense = encode_rows(&rows, 256);
        measure(
            &format!("timing{L}/{name}_dense_raw"),
            rows.len(),
            iters,
            || {
                black_box(rssp::stats::compute_timing_aware_stats(
                    black_box(&dense),
                    L,
                    black_box(&fake),
                ));
            },
        );
    }
}

fn timing_data(fakes: &str) -> rssp::timing::TimingData {
    rssp::timing::timing_data_from_chart_data(
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
        "",
        Some(fakes),
        "",
        rssp::timing::TimingFormat::Ssc,
        true,
    )
}

fn fast_timing_case(iters: usize) {
    let mut data = b"#VERSION:0.83;\n#TITLE:Fake intervals;\n#BPMS:0=120;\n#FAKES:8=4,32=8,128=16,512=32;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:10;\n#NOTES:\n".to_vec();
    for idx in 0..4096 {
        let mut row = [b'0'; 4];
        row[idx % 4] = if idx % 31 == 0 { b'L' } else { b'1' };
        data.extend_from_slice(&row);
        data.push(b'\n');
        if idx % 16 == 15 {
            data.extend_from_slice(b",\n");
        }
    }
    data.push(b';');
    let opts = rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..Default::default()
    };
    let mut scratch = rssp::AnalysisScratch::default();
    measure("analyze/fast_fake_lifts", 4096, iters, || {
        black_box(
            rssp::analyze_with_scratch(black_box(&data), "ssc", black_box(&opts), &mut scratch)
                .expect("valid fixture"),
        );
    });
}

fn batch_timing_cases(iters: usize) {
    for (name, count, local) in [
        ("plain", 0, false),
        ("global_aux", 128, false),
        ("local_aux", 128, true),
    ] {
        let mut data = String::from("#VERSION:0.83;\n#TITLE:Timing batch;\n#BPMS:0=120;\n");
        let mut aux = String::new();
        for (tag, value) in [("SPEEDS", "2=1=0"), ("SCROLLS", "0.5"), ("FAKES", "0.25")] {
            write!(aux, "#{tag}:").expect("String write cannot fail");
            for i in 0..count {
                if i != 0 {
                    aux.push(',');
                }
                write!(aux, "{}={value}", i * 4).expect("String write cannot fail");
            }
            aux.push_str(";\n");
        }
        if !local {
            data.push_str(&aux);
        }
        for difficulty in ["Easy", "Medium", "Hard", "Challenge"] {
            write!(
                data,
                "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:{difficulty};\n#METER:10;\n"
            )
            .expect("String write cannot fail");
            if local {
                data.push_str(
                    "#BPMS:0=180,16=150;\n#STOPS:8=0.25;\n#DELAYS:12=0.125;\n#WARPS:20=1;\n",
                );
                data.push_str(&aux);
            }
            data.push_str("#NOTES:\n");
            for m in 0..32 {
                if m != 0 {
                    data.push_str(",\n");
                }
                data.push_str("1000\n0100\n0010\n0001\n");
            }
            data.push_str(";\n");
        }
        measure(&format!("peak/{name}"), 4, iters, || {
            black_box(
                rssp::nps::compute_chart_peak_nps(black_box(data.as_bytes()), "ssc")
                    .expect("valid fixture"),
            );
        });
        measure(&format!("snapshot/{name}"), 4, iters, || {
            black_box(
                rssp::bpm::chart_bpm_snapshots(black_box(data.as_bytes()), "ssc")
                    .expect("valid fixture"),
            );
        });
    }
    let data = include_bytes!("fixtures/camellia_mix.ssc");
    measure("peak/camellia", 5, (iters / 10).max(1), || {
        black_box(
            rssp::nps::compute_chart_peak_nps(black_box(data), "ssc").expect("valid fixture"),
        );
    });
    measure("snapshot/camellia", 5, iters, || {
        black_box(rssp::bpm::chart_bpm_snapshots(black_box(data), "ssc").expect("valid fixture"));
    });
}

fn tier_cases(iters: usize) {
    for (name, densities) in [
        ("stream", vec![16; 16384]),
        (
            "mixed",
            (0..16384)
                .map(|i| [0, 16, 19, 16, 19, 20, 23, 20, 23, 32, 256, 32, 256][i % 13])
                .collect(),
        ),
    ] {
        let bpms = [(0.0, 137.125)];
        measure(&format!("tier/{name}"), densities.len(), iters, || {
            black_box(rssp::bpm::compute_tier_bpm(
                black_box(&densities),
                black_box(&bpms),
                4.0,
            ));
        });
    }
}

fn density_cases(iters: usize) {
    for lanes in [4, 5, 8, 10] {
        for (name, spacing, rows) in [("sparse", 16, 256), ("dense", 1, 16)] {
            let mut data = Vec::with_capacity(4096 * (lanes + 1));
            for i in 0..4096 {
                let start = data.len();
                data.resize(start + lanes, b'0');
                if i % spacing == 0 {
                    data[start + i % lanes] = b'1';
                }
                data.push(b'\n');
                if i % rows == rows - 1 {
                    data.extend_from_slice(b",\n");
                }
            }
            measure(&format!("density{lanes}/{name}"), 4096, iters, || {
                black_box(rssp::stats::measure_densities(black_box(&data), lanes));
            });
        }
    }
}

fn duration_cases(iters: usize) {
    for (name, dirty, miss) in [
        ("clean_hit", false, false),
        ("dirty_hit", true, false),
        ("dirty_miss", true, true),
    ] {
        let mut tags = String::new();
        for (tag, value) in [
            ("BPMS", "180"),
            ("STOPS", "0.01"),
            ("DELAYS", "0.01"),
            ("WARPS", "0.25"),
        ] {
            write!(tags, "#{tag}:").expect("String write cannot fail");
            for i in 0..128 {
                if i != 0 {
                    tags.push(',');
                }
                let pad = if dirty { " \u{1}" } else { "" };
                write!(tags, "{pad}{}={value}{pad}", i * 4).expect("String write cannot fail");
            }
            tags.push_str(";\n");
        }
        let mut data = String::from("#VERSION:0.83;\n#BPMS:0=120;\n");
        for (i, difficulty) in ["Easy", "Medium", "Hard", "Challenge"].iter().enumerate() {
            write!(data, "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:{difficulty};\n#METER:8;\n#OFFSET:{};\n", if miss { i } else { 0 })
                .expect("String write cannot fail");
            data.push_str(&tags);
            data.push_str("#NOTES:\n1000\n0100\n0010\n0001\n;\n");
        }
        measure(&format!("duration/{name}"), 4, iters, || {
            black_box(
                rssp::compute_chart_durations(
                    black_box(data.as_bytes()),
                    "ssc",
                    rssp::TimingOffsets::default(),
                )
                .expect("valid fixture"),
            );
        });
    }
    let data = include_bytes!("fixtures/camellia_mix.ssc");
    measure("duration/camellia", 5, (iters / 10).max(1), || {
        black_box(
            rssp::compute_chart_durations(black_box(data), "ssc", rssp::TimingOffsets::default())
                .expect("valid fixture"),
        );
    });
}

fn duration_row_cases(iters: usize) {
    for (lanes, step_type) in [
        (4, "dance-single"),
        (5, "pump-single"),
        (8, "dance-double"),
        (10, "pump-double"),
    ] {
        for (name, spacing, rows) in [("sparse", 16, 256), ("dense", 1, 16), ("odd", 7, 129)] {
            let mut data = format!("#BPMS:0=120;\n#NOTES:{step_type}::Hard:10::\n").into_bytes();
            for i in 0..4096 {
                let start = data.len();
                data.resize(start + lanes, b'0');
                if i % spacing == 0 {
                    data[start + i % lanes] = match (i / spacing) % 8 {
                        0 | 1 => b'2',
                        2 | 3 => b'3',
                        4 => b'M',
                        _ => b'1',
                    };
                }
                data.push(b'\n');
                if i % rows == rows - 1 {
                    data.extend_from_slice(b",\n");
                }
            }
            data.extend_from_slice(b";\n");
            measure(&format!("duration_rows{lanes}/{name}"), 4096, iters, || {
                black_box(
                    rssp::compute_chart_durations(
                        black_box(&data),
                        "sm",
                        rssp::TimingOffsets::default(),
                    )
                    .expect("valid fixture"),
                );
            });
        }
    }
}

fn breakdown_cases(iters: usize) {
    use rssp::streams::{BreakdownMode, StreamBreakdownLevel};
    for (name, len, pattern) in [
        ("uniform", 4096, &[16][..]),
        (
            "fragmented",
            4096,
            &[0, 16, 16, 0, 20, 20, 0, 0, 32, 32, 32, 0, 0, 0, 24][..],
        ),
        ("empty", 4096, &[0, 15][..]),
        ("short", 32, &[16, 0, 20, 0, 0, 24, 32][..]),
        ("leading", 4096, &[16][..]),
    ] {
        let densities: Vec<_> = (0..len)
            .map(|i| {
                if name == "leading" && i < 4000 {
                    0
                } else {
                    pattern[i % pattern.len()]
                }
            })
            .collect();
        for (mode, tag) in [
            (BreakdownMode::Detailed, "detailed"),
            (BreakdownMode::Partial, "partial"),
            (BreakdownMode::Simplified, "simple"),
        ] {
            measure(&format!("sn/{name}_{tag}"), len, iters, || {
                black_box(rssp::streams::generate_breakdown(
                    black_box(&densities),
                    mode,
                ));
            });
        }
        measure(&format!("sn/{name}_three"), len, iters, || {
            black_box(rssp::streams::generate_breakdowns(black_box(&densities)));
        });
        for (level, tag) in [
            (StreamBreakdownLevel::Detailed, "detailed"),
            (StreamBreakdownLevel::Partial, "partial"),
            (StreamBreakdownLevel::Simple, "simple"),
            (StreamBreakdownLevel::Total, "total"),
        ] {
            measure(&format!("standard/{name}_{tag}"), len, iters, || {
                black_box(rssp::streams::stream_breakdown(
                    black_box(&densities),
                    level,
                ));
            });
        }
        measure(&format!("standard/{name}_three"), len, iters, || {
            black_box(rssp::streams::stream_breakdowns(black_box(&densities)));
        });
        let mut tokens = Vec::new();
        measure(&format!("streams/{name}_combined"), len, iters, || {
            black_box(rssp::streams::compute_stream_outputs_with_scratch(
                black_box(&densities),
                &mut tokens,
            ));
        });
        measure(&format!("streams/{name}_cold"), len, iters, || {
            black_box(rssp::streams::compute_stream_outputs(black_box(&densities)));
        });
    }
}

fn cleanup_cases(iters: usize) {
    for len in [1, 128, 4096] {
        for (name, dirty) in [("clean", None), ("early", Some(0)), ("late", Some(len - 1))] {
            for speeds in [false, true] {
                let mut raw = String::new();
                for i in 0..len {
                    if i != 0 {
                        raw.push(',');
                    }
                    let pad = if dirty == Some(i) { " \u{b}" } else { "" };
                    if speeds {
                        write!(raw, "{pad}{}=1.25=0.5=0{pad}", i * 4).expect("String write");
                    } else {
                        write!(raw, "{pad}{}=120.125{pad}", i * 4).expect("String write");
                    }
                }
                let tag = if speeds { "speed" } else { "pair" };
                measure(&format!("cleanup/{tag}_{len}_{name}"), len, iters, || {
                    if speeds {
                        black_box(rssp::bpm::clean_norm_speeds_cow(black_box(&raw)));
                    } else {
                        black_box(rssp::bpm::clean_norm_map_cow(black_box(&raw)));
                    }
                });
            }
        }
    }
}

fn spacing_cases(iters: usize) {
    for lanes in [4, 5, 8, 10] {
        for (name, spacing, rows) in [("sparse", 16, 256), ("dense", 1, 16), ("odd", 7, 129)] {
            let mut data = Vec::new();
            for i in 0..4096 {
                let start = data.len();
                data.resize(start + lanes, b'0');
                if i % spacing == 0 {
                    data[start + i % lanes] = b'1';
                }
                data.push(b'\n');
                if i % rows == rows - 1 {
                    data.extend_from_slice(b",\n");
                }
            }
            measure(&format!("spacing/{lanes}_{name}"), 4096, iters, || {
                black_box(rssp::nps::measure_equally_spaced(black_box(&data), lanes));
            });
        }
    }
}

fn nps_stats_cases(iters: usize) {
    for len in [1, 2, 3, 8, 16, 32, 63, 64, 65, 256, 4096] {
        let values: Vec<_> = (0..len).map(|i| ((i * 37) % 23) as f64 / 3.0).collect();
        measure(&format!("nps_stats/{len}_cold"), len, iters * 10, || {
            black_box(rssp::nps::get_nps_stats(black_box(&values)));
        });
        let mut scratch = Vec::new();
        measure(&format!("nps_stats/{len}_warm"), len, iters * 10, || {
            black_box(rssp::nps::get_nps_stats_with_scratch(
                black_box(&values),
                &mut scratch,
            ));
        });
    }
}

// Keep explicit corpus fields together so the original/final comparison is auditable.
#[allow(clippy::too_many_lines)]
fn verify_corpus() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/packs");
    let mut files: Vec<_> = walkdir::WalkDir::new(&root)
        .into_iter()
        .map(|entry| entry.expect("read corpus"))
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| path.extension().is_some_and(|ext| ext == "zst"))
        .collect();
    files.sort_unstable();
    assert!(!files.is_empty(), "corpus must be populated");
    for path in files {
        let stem = std::path::Path::new(path.file_stem().expect("file stem"));
        let ext = stem
            .extension()
            .and_then(|ext| ext.to_str())
            .expect("inner extension");
        let bytes = std::fs::read(&path).expect("read simfile");
        let data = zstd::decode_all(bytes.as_slice()).expect("decode simfile");
        println!(
            "file {}",
            path.strip_prefix(&root).expect("corpus prefix").display()
        );
        if let Ok(parsed) = rssp::parse::extract_sections(&data, ext) {
            for chart in parsed.notes_list {
                if let Some(lanes) = rssp::supported_stepstype_lanes_bytes(chart.fields[0]) {
                    let densities = rssp::stats::measure_densities(chart.note_data, lanes);
                    println!("density {densities:?}");
                    println!(
                        "spacing {:?}",
                        rssp::nps::measure_equally_spaced(chart.note_data, lanes)
                    );
                    println!("sn {:?}", rssp::streams::generate_breakdowns(&densities));
                    println!(
                        "standard {:?} {:?}",
                        rssp::streams::stream_breakdowns(&densities),
                        rssp::streams::stream_breakdown(
                            &densities,
                            rssp::streams::StreamBreakdownLevel::Total
                        )
                    );
                }
            }
        }
        if let Ok(parsed) = rssp::parse::extract_sections(&data, ext) {
            for raw in [
                parsed.bpms,
                parsed.stops,
                parsed.delays,
                parsed.warps,
                parsed.scrolls,
                parsed.fakes,
            ] {
                if let Some(raw) = raw.and_then(|raw| std::str::from_utf8(raw).ok()) {
                    println!("norm {:?}", rssp::bpm::clean_norm_map_cow(raw));
                }
            }
            if let Some(raw) = parsed.speeds.and_then(|raw| std::str::from_utf8(raw).ok()) {
                println!("speed {:?}", rssp::bpm::clean_norm_speeds_cow(raw));
            }
        }
        match rssp::compute_chart_durations(&data, ext, rssp::TimingOffsets::default()) {
            Ok(charts) => {
                for chart in charts {
                    println!(
                        "duration {:?} {:?} {}",
                        chart.step_type,
                        chart.difficulty,
                        chart.duration_seconds.to_bits()
                    );
                }
            }
            Err(err) => println!("duration error {err:?}"),
        }
        match rssp::nps::compute_chart_peak_nps(&data, ext) {
            Ok(charts) => {
                for chart in charts {
                    println!(
                        "peak {:?} {:?} {}",
                        chart.step_type,
                        chart.difficulty,
                        chart.peak_nps.to_bits()
                    );
                }
            }
            Err(err) => println!("peak error {err:?}"),
        }
        match rssp::bpm::chart_bpm_snapshots(&data, ext) {
            Ok(charts) => {
                for chart in charts {
                    println!(
                        "bpm {:?} {:?} {:?} {:?} {} {} {:?} {} {}",
                        chart.step_type,
                        chart.difficulty,
                        chart.hash_bpms,
                        chart.bpms_formatted,
                        chart.bpm_min.to_bits(),
                        chart.bpm_max.to_bits(),
                        chart.display_bpm,
                        chart.display_bpm_min.to_bits(),
                        chart.display_bpm_max.to_bits()
                    );
                }
            }
            Err(err) => println!("bpm error {err:?}"),
        }
    }
}

fn custom_cases(iters: usize) {
    for count in [4, 32, 256] {
        let patterns: Vec<_> = (0..count)
            .map(|i| {
                (0..8)
                    .map(|shift| char::from(b"LDUR"[(i >> (shift * 2)) % 4]))
                    .collect::<String>()
            })
            .collect();
        let compiled = rssp::patterns::compile_custom_patterns(&patterns);
        measure(&format!("custom/{count}_compile"), count, iters, || {
            black_box(rssp::patterns::compile_custom_patterns(black_box(
                &patterns,
            )));
        });
        for rows in [128, 4096] {
            let masks: Vec<_> = (0..rows).map(|i| [1, 2, 4, 8][i % 4]).collect();
            measure(&format!("custom/{count}_{rows}_owned"), rows, iters, || {
                black_box(rssp::patterns::detect_custom_patterns(
                    black_box(&masks),
                    black_box(&patterns),
                ));
            });
            measure(
                &format!("custom/{count}_{rows}_compiled"),
                rows,
                iters,
                || {
                    black_box(rssp::patterns::detect_custom_patterns_compiled(
                        black_box(&masks),
                        black_box(&compiled),
                    ));
                },
            );
        }
    }
}

fn bpm_stats_cases(iters: usize) {
    for len in [0, 1, 2, 8, 32, 64] {
        let values: Vec<_> = (0..len).map(|i| 120.0 + (i % 5) as f64).collect();
        let map: Vec<_> = values
            .iter()
            .enumerate()
            .map(|(i, &bpm)| (i as f64 * 4.0, bpm))
            .collect();
        measure(&format!("bpm_stats/{len}_values"), len, iters * 10, || {
            black_box(rssp::bpm::compute_bpm_stats(black_box(&values)));
        });
        measure(&format!("bpm_stats/{len}_map"), len, iters * 10, || {
            black_box(rssp::bpm::compute_bpm_map_stats(black_box(&map)));
        });
        measure(
            &format!("bpm_stats/{len}_summary_cold"),
            len,
            iters * 10,
            || {
                black_box(rssp::bpm::compute_bpm_range_and_stats(black_box(&map)));
            },
        );
        let mut scratch = Vec::with_capacity(len);
        measure(
            &format!("bpm_stats/{len}_summary_warm"),
            len,
            iters * 10,
            || {
                black_box(rssp::bpm::compute_bpm_range_and_stats_with_scratch(
                    black_box(&map),
                    &mut scratch,
                ));
            },
        );
    }
}

fn serialize_cases(iters: usize) {
    let options = rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..Default::default()
    };
    for (name, data) in [
        ("small", &include_bytes!("fixtures/hash_fixture.ssc")[..]),
        ("timing", &include_bytes!("fixtures/bpm_fixture.ssc")[..]),
        ("large", &include_bytes!("fixtures/camellia_mix.ssc")[..]),
    ] {
        let summary = rssp::analyze(data, "ssc", &options).expect("valid fixture");
        for ext in ["sm", "ssc"] {
            let mut output = Vec::new();
            if rssp::serialize::serialize_simfile(&summary, ext, &mut output).is_err() {
                continue; // SM cannot represent chart-local timing.
            }
            let size = output.len();
            measure(&format!("serialize/{name}_{ext}"), size, iters, || {
                output.clear();
                black_box(
                    rssp::serialize::serialize_simfile(black_box(&summary), ext, &mut output)
                        .expect("valid output"),
                );
                black_box(&output);
            });
        }
    }
}

fn zero_duration_cases(iters: usize) {
    for len in [1, 128] {
        let mut pairs = String::new();
        for i in 0..len {
            if i != 0 {
                pairs.push(',');
            }
            write!(pairs, "{}=0.125", i * 4).expect("String write");
        }
        for (name, notes, local) in [
            ("empty_global", "0000\n", false),
            ("first_global", "1000\n", false),
            ("empty_local", "0000\n", true),
            ("first_local", "1000\n", true),
            ("nonzero_local", "0000\n1000\n", true),
        ] {
            let mut data =
                format!("#VERSION:0.83;\n#BPMS:0=120;\n#STOPS:{pairs};\n#DELAYS:{pairs};\n");
            for i in 0..4 {
                write!(
                    data,
                    "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:10;\n"
                )
                .expect("String write");
                if local {
                    write!(
                        data,
                        "#OFFSET:{i};\n#BPMS:0=180;\n#STOPS:{pairs};\n#DELAYS:{pairs};\n"
                    )
                    .expect("String write");
                }
                write!(data, "#NOTES:\n{notes};\n").expect("String write");
            }
            measure(&format!("zero_duration/{len}_{name}"), 4, iters, || {
                black_box(
                    rssp::compute_chart_durations(
                        black_box(data.as_bytes()),
                        "ssc",
                        rssp::TimingOffsets::default(),
                    )
                    .expect("valid fixture"),
                );
            });
        }
    }
}

fn peak_work_cases(iters: usize) {
    for len in [1, 128] {
        let mut pairs = String::new();
        for i in 0..len {
            if i != 0 {
                pairs.push(',');
            }
            write!(pairs, " {} = 0.125 ", i * 4).expect("String write");
        }
        for (name, notes, local, varying) in [
            ("empty_global", "0000\n", false, false),
            ("objects_local", "MFLK\n3000\n", true, false),
            ("first_local", "1000\n", true, false),
            ("repeat_local", "1000\n0100\n0010\n0001\n", true, false),
            ("vary_local", "1000\n0100\n0010\n0001\n", true, true),
        ] {
            let mut data =
                format!("#VERSION:0.83;\n#BPMS:0=120;\n#STOPS:{pairs};\n#DELAYS:{pairs};\n");
            for i in 0..4 {
                data.push_str(
                    "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:10;\n",
                );
                if local {
                    let offset = if varying { i } else { 0 };
                    write!(
                        data,
                        "#OFFSET:{offset};\n#BPMS: 0 = 180 ;\n#STOPS:{pairs};\n#DELAYS:{pairs};\n"
                    )
                    .expect("String write");
                }
                write!(data, "#NOTES:\n{notes};\n").expect("String write");
            }
            measure(&format!("peak_work/{len}_{name}"), 4, iters, || {
                black_box(
                    rssp::compute_chart_peak_nps(black_box(data.as_bytes()), "ssc")
                        .expect("valid fixture"),
                );
            });
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "emit an ordered component transcript for original/final byte comparison"
)]
fn verify_components() {
    verify_reports();
    for input in [
        "",
        "0=120",
        "0=120,4=180",
        "NaN=inf",
        "-0=-0",
        "1e308=1e308",
        "bad",
        " 1\u{1}=2 ",
    ] {
        println!(
            "tidy {input:?} {:?}",
            rssp::bpm::normalize_and_tidy_bpms(input)
        );
    }
    for kind in ["utf8", "utf8_escape", "cp1252", "cp1252_escape"] {
        let data = credit_fixture(kind, 8);
        let summary = rssp::analyze(
            &data,
            "ssc",
            &rssp::AnalysisOptions {
                compute_tech_counts: false,
                compute_pattern_counts: false,
                ..Default::default()
            },
        )
        .expect("valid credit fixture");
        for chart in summary.charts {
            println!(
                "credit {kind} {:?} {:?}",
                chart.step_artist_str, chart.tech_notation_str
            );
        }
    }
    for kind in ["global", "repeat", "vary", "distinct"] {
        let data = hash_batch_fixture(kind, 128);
        for chart in rssp::compute_all_hashes(&data, "ssc").expect("valid hash fixture") {
            println!(
                "hash {kind} {} {} {}",
                chart.step_type, chart.difficulty, chart.hash
            );
        }
    }
    let patterns: Vec<_> = ["", "l", "L", "LD", "ldu", "U", "?", "É", "ldurldur"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let masks: Vec<_> = (0..256).map(|i| [1, 2, 4, 8, 0, 17, 31][i % 7]).collect();
    println!(
        "custom {:?}",
        rssp::patterns::detect_custom_patterns(&masks, &patterns)
    );
    let compiled = rssp::patterns::compile_custom_patterns(&patterns);
    println!(
        "compiled {:?}",
        rssp::patterns::detect_custom_patterns_compiled(&masks, &compiled)
    );
    for bpm in [
        120.0,
        0.0,
        -0.0,
        -120.0,
        10_000.0,
        f64::MAX,
        f64::MIN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::from_bits(0x7ff8_0000_0000_1234),
    ] {
        let map = [(0.0, bpm)];
        let stats = rssp::bpm::compute_bpm_stats(&[bpm]);
        let mapped = rssp::bpm::compute_bpm_map_stats(&map);
        let summary = rssp::bpm::compute_bpm_range_and_stats(&map);
        let mut scratch = vec![180.0; 3];
        let warm = rssp::bpm::compute_bpm_range_and_stats_with_scratch(&map, &mut scratch);
        println!(
            "single {} {} {} {} {} {} {} {} {} {} {} {} {} {:?}",
            bpm.to_bits(),
            stats.0.to_bits(),
            stats.1.to_bits(),
            mapped.0.to_bits(),
            mapped.1.to_bits(),
            summary.0,
            summary.1,
            summary.2.to_bits(),
            summary.3.to_bits(),
            warm.0,
            warm.1,
            warm.2.to_bits(),
            warm.3.to_bits(),
            scratch
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        );
    }
    let options = rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..Default::default()
    };
    for data in [
        &include_bytes!("fixtures/hash_fixture.ssc")[..],
        &include_bytes!("fixtures/bpm_fixture.ssc")[..],
        &include_bytes!("fixtures/camellia_mix.ssc")[..],
    ] {
        let summary = rssp::analyze(data, "ssc", &options).expect("valid fixture");
        for ext in ["sm", "ssc"] {
            let mut output = Vec::new();
            println!(
                "serialized {ext} {:?} {:?}",
                rssp::serialize::serialize_simfile(&summary, ext, &mut output),
                output
            );
        }
    }
}

fn main() {
    if std::env::var_os("RSSP_HOT_VERIFY").is_some() {
        verify_components();
        verify_corpus();
        return;
    }
    cpu::pin();
    let iters = std::env::var("RSSP_HOT_ITERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200);
    row_cases::<4>(iters);
    row_cases::<8>(iters);
    timing_cases::<4>(iters);
    timing_cases::<8>(iters);
    fast_timing_case(iters);
    batch_timing_cases(iters);
    tier_cases(iters);
    density_cases(iters);
    duration_cases(iters);
    duration_row_cases(iters);
    breakdown_cases(iters);
    cleanup_cases(iters);
    spacing_cases(iters);
    nps_stats_cases(iters);
    custom_cases(iters);
    bpm_stats_cases(iters);
    zero_duration_cases(iters);
    peak_work_cases(iters);
    serialize_cases(iters);
    tidy_bpm_cases(iters);
    hash_batch_cases(iters);
    credit_cases(iters);
    report_cases(iters);
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

fn report_fixture(kind: &str, length: usize) -> rssp::SimfileSummary {
    let mut summary = rssp::analyze(
        include_bytes!("fixtures/hash_fixture.ssc"),
        "ssc",
        &rssp::AnalysisOptions::default(),
    )
    .expect("valid report fixture");
    let mut value = "a".repeat(length);
    match kind {
        "early" => value.replace_range(0..1, "\""),
        "late" => value.replace_range(length - 1..length, "\""),
        "comma" => value.replace_range(length - 1..length, ","),
        "comma_quote" => {
            value.replace_range(length / 2..=length / 2, ",");
            value.replace_range(length - 1..length, "\"");
        }
        "dense" => value = "\\\"\n,".repeat(length / 4),
        _ => {}
    }
    summary.title_str.clone_from(&value);
    summary.subtitle_str.clone_from(&value);
    summary.artist_str.clone_from(&value);
    summary.titletranslit_str.clone_from(&value);
    summary.subtitletranslit_str.clone_from(&value);
    summary.artisttranslit_str = value;
    if kind == "custom" {
        summary.charts[0].custom_patterns = ["é \" \\ \n \u{1}", "title", "ldur"]
            .into_iter()
            .map(|name| rssp::patterns::CustomPatternSummary {
                pattern: name.to_owned(),
                count: 7,
            })
            .collect();
    }
    summary
}

fn report_cases(iters: usize) {
    for length in [16, 4096] {
        for kind in [
            "clean",
            "early",
            "late",
            "dense",
            "comma",
            "comma_quote",
            "custom",
        ] {
            let summary = report_fixture(kind, length);
            for (mode, label) in [
                (rssp::report::OutputMode::JSON, "json"),
                (rssp::report::OutputMode::CSV, "csv"),
            ] {
                let mut output = Vec::with_capacity(65536);
                measure(&format!("report/{label}/{length}_{kind}"), 1, iters, || {
                    output.clear();
                    rssp::report::write_reports(black_box(&summary), mode, black_box(&mut output))
                        .expect("Vec write");
                    black_box(&output);
                });
            }
        }
    }
    let summary = rssp::analyze(
        include_bytes!("fixtures/camellia_mix.ssc"),
        "ssc",
        &rssp::AnalysisOptions::default(),
    )
    .expect("valid report fixture");
    for (mode, label) in [
        (rssp::report::OutputMode::JSON, "json"),
        (rssp::report::OutputMode::CSV, "csv"),
    ] {
        let mut output = Vec::with_capacity(262_144);
        measure(&format!("report/{label}/camellia"), 1, iters, || {
            output.clear();
            rssp::report::write_reports(black_box(&summary), mode, black_box(&mut output))
                .expect("Vec write");
            black_box(&output);
        });
    }
}

fn verify_reports() {
    for length in [16, 4096] {
        for kind in [
            "clean",
            "early",
            "late",
            "dense",
            "comma",
            "comma_quote",
            "custom",
        ] {
            let summary = report_fixture(kind, length);
            for mode in [
                rssp::report::OutputMode::JSON,
                rssp::report::OutputMode::CSV,
            ] {
                let mut output = Vec::new();
                rssp::report::write_reports(&summary, mode, &mut output).expect("Vec write");
                println!("report {length} {kind} {mode:?} {output:?}");
            }
        }
    }
}

fn tidy_bpm_cases(iters: usize) {
    for count in [1, 128, 4096] {
        let mut input = String::new();
        for i in 0..count {
            if i != 0 {
                input.push(',');
            }
            write!(input, "{}={}", i * 4, 120 + i % 17).expect("String write");
        }
        measure(&format!("tidy_bpm/{count}"), count, iters, || {
            black_box(rssp::bpm::normalize_and_tidy_bpms(black_box(&input)));
        });
    }
}

fn hash_batch_fixture(kind: &str, components: usize) -> Vec<u8> {
    let mut global = String::new();
    for i in 0..components {
        if i != 0 {
            global.push(',');
        }
        write!(global, "{}={}", i * 4, 120 + i % 17).expect("String write");
    }
    let local = global.replace("120", "160");
    let mut data = format!("#VERSION:0.83;\n#BPMS:{global};\n");
    for chart in 0..8 {
        data.push_str("#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:8;\n");
        let bpms = if kind == "global" || (kind == "vary" && chart % 2 == 0) {
            &global
        } else {
            &local
        };
        let distinct = local.replace("160", &(160 + chart).to_string());
        let bpms = if kind == "distinct" { &distinct } else { bpms };
        writeln!(data, "#BPMS:{bpms};\n#NOTES:\n1000\n0100\n0010\n0001\n;").expect("String write");
    }
    data.into_bytes()
}

fn hash_batch_cases(iters: usize) {
    for components in [1, 128] {
        for kind in ["global", "repeat", "vary", "distinct"] {
            let data = hash_batch_fixture(kind, components);
            measure(&format!("hash_batch/{components}_{kind}"), 8, iters, || {
                black_box(
                    rssp::compute_all_hashes(black_box(&data), "ssc").expect("valid fixture"),
                );
            });
        }
    }
}

fn credit_fixture(kind: &str, length: usize) -> Vec<u8> {
    let bytes = match kind {
        "utf8" => &b"Author "[..],
        "utf8_escape" => &b"Author\\: "[..],
        "cp1252" => &b"Author\x93 "[..],
        _ => &b"Author\\:\x93 "[..],
    };
    let mut data = b"#VERSION:0.83;\n#BPMS:0=120;\n".to_vec();
    for _ in 0..4 {
        data.extend_from_slice(
            b"#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:8;\n#CREDIT:",
        );
        for i in 0..length {
            data.push(bytes[i % bytes.len()]);
        }
        // Avoid a dangling escape swallowing the tag terminator.
        data.extend_from_slice(b" end;\n#NOTES:\n1000\n0100\n0010\n0001\n;\n");
    }
    data
}

fn credit_cases(iters: usize) {
    let options = rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..Default::default()
    };
    let mut scratch = rssp::AnalysisScratch::default();
    for length in [16, 4096] {
        for kind in ["utf8", "utf8_escape", "cp1252", "cp1252_escape"] {
            let data = credit_fixture(kind, length);
            measure(&format!("credit/{length}_{kind}"), 4, iters, || {
                black_box(
                    rssp::analyze_with_scratch(
                        black_box(&data),
                        "ssc",
                        black_box(&options),
                        &mut scratch,
                    )
                    .expect("valid fixture"),
                );
            });
        }
    }
}
