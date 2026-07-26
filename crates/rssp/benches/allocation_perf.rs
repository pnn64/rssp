use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

#[path = "support/step_parity.rs"]
mod step_parity_bench;

const FIXTURES: [(&str, &str); 4] = [
    ("fixtures/camellia_mix.ssc", "ssc"),
    ("fixtures/hash_fixture.ssc", "ssc"),
    ("fixtures/200000_step_challenge.sm", "sm"),
    ("fixtures/24h_of_100bpm_stream.sm", "sm"),
];

struct CountingAllocator;

static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static DEALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static REALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn update_peak(live: usize) {
    PEAK_LIVE_BYTES.fetch_max(live, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            update_peak(live);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            REALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            let live = if new_size >= layout.size() {
                LIVE_BYTES.fetch_add(new_size - layout.size(), Ordering::Relaxed) + new_size
                    - layout.size()
            } else {
                LIVE_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed)
                    - (layout.size() - new_size)
            };
            update_peak(live);
        }
        new_ptr
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Parse,
    Fast,
    Full,
    Annotations,
    Hashes,
    Durations,
    Nps,
    Bpms,
    Tech,
    Snapshot,
    Csv,
    Json,
    JsonFull,
    CourseJson,
    ParitySingle,
    ParityDouble,
    ParitySingleHolds,
    ParityDoubleHolds,
}

struct SimInput {
    extension: &'static str,
    raw: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Counters {
    alloc_calls: u64,
    dealloc_calls: u64,
    realloc_calls: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    live_bytes: usize,
    peak_live_bytes: usize,
}

impl Counters {
    fn read() -> Self {
        Self {
            alloc_calls: ALLOC_CALLS.load(Ordering::Relaxed),
            dealloc_calls: DEALLOC_CALLS.load(Ordering::Relaxed),
            realloc_calls: REALLOC_CALLS.load(Ordering::Relaxed),
            alloc_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            realloc_bytes: REALLOC_BYTES.load(Ordering::Relaxed),
            live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
            peak_live_bytes: PEAK_LIVE_BYTES.load(Ordering::Relaxed),
        }
    }
}

fn reset_counters() {
    ALLOC_CALLS.store(0, Ordering::Relaxed);
    DEALLOC_CALLS.store(0, Ordering::Relaxed);
    REALLOC_CALLS.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
    REALLOC_BYTES.store(0, Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn parse_args() -> (Mode, usize) {
    let mut mode = Mode::Full;
    let mut iterations = 1usize;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" if i + 1 < args.len() => {
                mode = match args[i + 1].as_str() {
                    "parse" => Mode::Parse,
                    "fast" => Mode::Fast,
                    "annotations" => Mode::Annotations,
                    "hashes" => Mode::Hashes,
                    "durations" => Mode::Durations,
                    "nps" => Mode::Nps,
                    "bpms" => Mode::Bpms,
                    "tech" => Mode::Tech,
                    "snapshot" => Mode::Snapshot,
                    "csv" => Mode::Csv,
                    "json" => Mode::Json,
                    "json-full" => Mode::JsonFull,
                    "course-json" => Mode::CourseJson,
                    "parity-single" => Mode::ParitySingle,
                    "parity-double" => Mode::ParityDouble,
                    "parity-single-holds" => Mode::ParitySingleHolds,
                    "parity-double-holds" => Mode::ParityDoubleHolds,
                    _ => Mode::Full,
                };
                i += 2;
            }
            "--iters" if i + 1 < args.len() => {
                iterations = args[i + 1].parse().unwrap_or(1).max(1);
                i += 2;
            }
            _ => i += 1,
        }
    }
    (mode, iterations)
}

fn load_corpus() -> Vec<SimInput> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches");
    let mut corpus = Vec::with_capacity(FIXTURES.len());
    for (path, extension) in FIXTURES {
        let raw = fs::read(root.join(path)).expect("benchmark fixture should be readable");
        corpus.push(SimInput { extension, raw });
    }
    corpus
}

fn options_for(mode: Mode) -> rssp::AnalysisOptions {
    match mode {
        Mode::Fast
        | Mode::Parse
        | Mode::Hashes
        | Mode::Durations
        | Mode::Nps
        | Mode::Bpms
        | Mode::Tech
        | Mode::Snapshot
        | Mode::Csv
        | Mode::Json => rssp::AnalysisOptions {
            mono_threshold: 6,
            compute_tech_counts: false,
            compute_pattern_counts: false,
            ..rssp::AnalysisOptions::default()
        },
        Mode::Full => rssp::AnalysisOptions {
            mono_threshold: 6,
            ..rssp::AnalysisOptions::default()
        },
        Mode::CourseJson => rssp::AnalysisOptions {
            mono_threshold: 6,
            ..rssp::AnalysisOptions::default()
        },
        Mode::JsonFull => rssp::AnalysisOptions {
            mono_threshold: 6,
            ..rssp::AnalysisOptions::default()
        },
        Mode::Annotations => rssp::AnalysisOptions {
            mono_threshold: 6,
            compute_note_annotations: true,
            ..rssp::AnalysisOptions::default()
        },
        Mode::ParitySingle
        | Mode::ParityDouble
        | Mode::ParitySingleHolds
        | Mode::ParityDoubleHolds => rssp::AnalysisOptions::default(),
    }
}

fn run_once(mode: Mode, corpus: &[SimInput], options: &rssp::AnalysisOptions) -> usize {
    let mut checksum = 0usize;
    for sim in corpus {
        match mode {
            Mode::Parse => {
                let parsed = rssp::parse::extract_sections(
                    black_box(sim.raw.as_slice()),
                    black_box(sim.extension),
                )
                .expect("fixture should parse");
                checksum = checksum.wrapping_add(parsed.notes_list.len());
            }
            Mode::Hashes => {
                let hashes = rssp::compute_all_hashes(
                    black_box(sim.raw.as_slice()),
                    black_box(sim.extension),
                )
                .expect("fixture should hash");
                checksum = checksum.wrapping_add(hashes.len());
                black_box(hashes);
            }
            Mode::Durations => {
                let durations = rssp::compute_chart_durations(
                    black_box(sim.raw.as_slice()),
                    black_box(sim.extension),
                    rssp::TimingOffsets::default(),
                )
                .expect("fixture durations should compute");
                checksum = checksum.wrapping_add(durations.len());
                black_box(durations);
            }
            Mode::Nps => {
                let nps = rssp::compute_chart_peak_nps(
                    black_box(sim.raw.as_slice()),
                    black_box(sim.extension),
                )
                .expect("fixture NPS should compute");
                checksum = checksum.wrapping_add(nps.len());
                black_box(nps);
            }
            Mode::Bpms => {
                let bpms = rssp::bpm::chart_bpm_snapshots(
                    black_box(sim.raw.as_slice()),
                    black_box(sim.extension),
                )
                .expect("fixture BPM snapshots should compute");
                checksum = checksum.wrapping_add(bpms.len());
                black_box(bpms);
            }
            Mode::Tech => {
                for _ in 0..256 {
                    let notation = rssp::tech::parse_tech_notation(
                        black_box("BR+ FS- 24ths XO+ SKT-"),
                        black_box("32nds DS++ JA- WA+ BXF- No Tech"),
                    );
                    checksum = checksum.wrapping_add(notation.len());
                    black_box(notation);
                }
            }
            Mode::Snapshot => {
                unreachable!("report modes use their dedicated allocation runner")
            }
            Mode::Csv => {
                unreachable!("report modes use their dedicated allocation runner")
            }
            Mode::Json => {
                unreachable!("report modes use their dedicated allocation runner")
            }
            Mode::JsonFull => {
                unreachable!("report modes use their dedicated allocation runner")
            }
            Mode::CourseJson => {
                unreachable!("course report mode uses its dedicated allocation runner")
            }
            Mode::ParitySingle
            | Mode::ParityDouble
            | Mode::ParitySingleHolds
            | Mode::ParityDoubleHolds => {
                unreachable!("step-parity modes use their dedicated allocation runner")
            }
            _ => {
                let summary = rssp::analyze(
                    black_box(sim.raw.as_slice()),
                    black_box(sim.extension),
                    black_box(options),
                )
                .expect("fixture should analyze");
                checksum = checksum.wrapping_add(
                    summary
                        .charts
                        .iter()
                        .map(|chart| chart.stats.total_steps as usize)
                        .sum::<usize>(),
                );
                black_box(summary);
            }
        }
    }
    checksum
}

fn build_report_summaries(
    corpus: &[SimInput],
    options: &rssp::AnalysisOptions,
) -> Vec<rssp::report::SimfileSummary> {
    corpus
        .iter()
        .map(|sim| {
            rssp::analyze(sim.raw.as_slice(), sim.extension, options)
                .expect("fixture should analyze")
        })
        .collect()
}

fn build_course_summary(
    corpus: &[SimInput],
    options: &rssp::AnalysisOptions,
    entry_count: usize,
) -> rssp::CourseSummary {
    let sim = corpus
        .first()
        .expect("benchmark corpus should not be empty");
    let mut simfile =
        rssp::analyze(sim.raw.as_slice(), sim.extension, options).expect("fixture should analyze");
    let chart = simfile
        .charts
        .pop()
        .expect("fixture should contain a chart");
    let entries = (0..entry_count)
        .map(|index| rssp::CourseEntrySummary {
            song: format!("Song {index} \"Special\""),
            song_dir: format!("Group/Song {index}"),
            step_type: "dance-single".to_string(),
            difficulty: "Challenge".to_string(),
            rating: (10 + index % 20).to_string(),
            sha1: format!("{index:016x}"),
            bpm_neutral_sha1: format!("{:016x}", index.wrapping_mul(31)),
        })
        .collect();
    let sha1_hashes = (0..entry_count)
        .map(|index| format!("{index:016x}"))
        .collect();
    let bpm_neutral_sha1_hashes = (0..entry_count)
        .map(|index| format!("{:016x}", index.wrapping_mul(31)))
        .collect();

    rssp::CourseSummary {
        course: "Performance \"Course\"".to_string(),
        course_difficulty: "Challenge".to_string(),
        step_type: "dance-single".to_string(),
        total_length: 7_200,
        entries,
        chart,
        sha1_hashes,
        bpm_neutral_sha1_hashes,
        pattern_counts_enabled: true,
        tech_counts_enabled: true,
        total_elapsed: std::time::Duration::ZERO,
    }
}

fn run_report_once(mode: Mode, summaries: &[rssp::report::SimfileSummary]) -> usize {
    let mut checksum = 0usize;
    for summary in summaries {
        match mode {
            Mode::Snapshot => {
                for chart in &summary.charts {
                    let snapshot = rssp::report::build_timing_snapshot(chart, summary);
                    checksum = checksum
                        .wrapping_add(snapshot.bpms.len())
                        .wrapping_add(snapshot.stops.len())
                        .wrapping_add(snapshot.speeds.len());
                    black_box(snapshot);
                }
            }
            Mode::Csv => {
                let mut output = Vec::new();
                rssp::report::write_reports(summary, rssp::report::OutputMode::CSV, &mut output)
                    .expect("CSV report should write");
                checksum = checksum.wrapping_add(output.len());
                black_box(output);
            }
            Mode::Json | Mode::JsonFull => {
                let mut output = Vec::new();
                rssp::report::write_reports(summary, rssp::report::OutputMode::JSON, &mut output)
                    .expect("JSON report should write");
                checksum = checksum.wrapping_add(output.len());
                black_box(output);
            }
            _ => unreachable!("only report modes use report summaries"),
        }
    }
    checksum
}

fn run_benchmark_once(
    mode: Mode,
    corpus: &[SimInput],
    options: &rssp::AnalysisOptions,
    summaries: &[rssp::report::SimfileSummary],
    course: Option<&rssp::CourseSummary>,
) -> usize {
    if matches!(mode, Mode::CourseJson) {
        let mut output = Vec::new();
        rssp::report::write_course_reports(
            course.expect("course summary should be built"),
            rssp::report::OutputMode::JSON,
            &mut output,
        )
        .expect("course JSON report should write");
        let len = output.len();
        black_box(output);
        return len;
    }
    if matches!(
        mode,
        Mode::Snapshot | Mode::Csv | Mode::Json | Mode::JsonFull
    ) {
        run_report_once(mode, summaries)
    } else {
        run_once(mode, corpus, options)
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Parse => "parse",
        Mode::Fast => "fast",
        Mode::Full => "full",
        Mode::Annotations => "annotations",
        Mode::Hashes => "hashes",
        Mode::Durations => "durations",
        Mode::Nps => "nps",
        Mode::Bpms => "bpms",
        Mode::Tech => "tech",
        Mode::Snapshot => "snapshot",
        Mode::Csv => "csv",
        Mode::Json => "json",
        Mode::JsonFull => "json-full",
        Mode::CourseJson => "course-json",
        Mode::ParitySingle => "parity-single",
        Mode::ParityDouble => "parity-double",
        Mode::ParitySingleHolds => "parity-single-holds",
        Mode::ParityDoubleHolds => "parity-double-holds",
    }
}

fn print_parity_alloc(
    mode: &str,
    phase: &str,
    iterations: usize,
    rows: usize,
    elapsed: std::time::Duration,
    before: Counters,
    after: Counters,
) {
    let divisor = iterations as f64;
    let seconds = elapsed.as_secs_f64();
    let total_rows = rows as f64 * divisor;
    println!(
        concat!(
            "mode={} phase={} iters={} elapsed_s={:.6} throughput_mrows_s={:.3} ",
            "alloc_calls_per_iter={:.1} dealloc_calls_per_iter={:.1} ",
            "realloc_calls_per_iter={:.1} alloc_bytes_per_iter={:.1} ",
            "realloc_bytes_per_iter={:.1} live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        mode,
        phase,
        iterations,
        seconds,
        total_rows / seconds / 1_000_000.0,
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_parity_alloc<const LANES: usize>(
    mode: &str,
    row_count: usize,
    masks: &[u8],
    has_holds: bool,
    iterations: usize,
) {
    let rows = if has_holds {
        step_parity_bench::hold_rows::<LANES>(row_count, masks)
    } else {
        step_parity_bench::rows::<LANES>(row_count, masks)
    };
    let beats = step_parity_bench::beats(row_count);
    let timing = step_parity_bench::timing();

    // Initialize the immutable layout/permutation cache outside both samples.
    drop(rssp::step_parity::timing_rows_scratch::<LANES>());

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut scratch =
        rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
    black_box(rssp::step_parity::analyze_timing_rows_known_holds(
        black_box(&rows),
        black_box(&beats),
        black_box(&timing),
        has_holds,
        black_box(&mut scratch),
    ));
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(mode, "cold", 1, row_count, elapsed, before, after);

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(rssp::step_parity::analyze_timing_rows_known_holds(
            black_box(&rows),
            black_box(&beats),
            black_box(&timing),
            has_holds,
            black_box(&mut scratch),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode, "reused", iterations, row_count, elapsed, before, after,
    );
}

fn main() {
    let (mode, iterations) = parse_args();
    match mode {
        Mode::ParitySingle => {
            run_parity_alloc::<4>(
                mode_name(mode),
                step_parity_bench::SINGLE_ROW_COUNT,
                step_parity_bench::SINGLE_MASKS,
                false,
                iterations,
            );
            return;
        }
        Mode::ParityDouble => {
            run_parity_alloc::<8>(
                mode_name(mode),
                step_parity_bench::DOUBLE_ROW_COUNT,
                step_parity_bench::DOUBLE_MASKS,
                false,
                iterations,
            );
            return;
        }
        Mode::ParitySingleHolds => {
            run_parity_alloc::<4>(
                mode_name(mode),
                step_parity_bench::SINGLE_ROW_COUNT,
                step_parity_bench::SINGLE_MASKS,
                true,
                iterations,
            );
            return;
        }
        Mode::ParityDoubleHolds => {
            run_parity_alloc::<8>(
                mode_name(mode),
                step_parity_bench::DOUBLE_ROW_COUNT,
                step_parity_bench::DOUBLE_MASKS,
                true,
                iterations,
            );
            return;
        }
        _ => {}
    }

    let corpus = load_corpus();
    let options = options_for(mode);
    let bytes: usize = corpus.iter().map(|sim| sim.raw.len()).sum();
    let report_summaries = if matches!(
        mode,
        Mode::Snapshot | Mode::Csv | Mode::Json | Mode::JsonFull
    ) {
        build_report_summaries(&corpus, &options)
    } else {
        Vec::new()
    };
    let course_summary =
        matches!(mode, Mode::CourseJson).then(|| build_course_summary(&corpus, &options, 1_024));

    black_box(run_benchmark_once(
        mode,
        &corpus,
        &options,
        &report_summaries,
        course_summary.as_ref(),
    ));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(run_benchmark_once(
            mode,
            &corpus,
            &options,
            &report_summaries,
            course_summary.as_ref(),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();

    let seconds = elapsed.as_secs_f64();
    let total_bytes = bytes as f64 * iterations as f64;
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode={} iters={} checksum={} elapsed_s={:.6} throughput_mib_s={:.3} ",
            "alloc_calls_per_iter={:.1} dealloc_calls_per_iter={:.1} ",
            "realloc_calls_per_iter={:.1} alloc_bytes_per_iter={:.1} ",
            "realloc_bytes_per_iter={:.1} live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        mode_name(mode),
        iterations,
        black_box(checksum),
        seconds,
        total_bytes / seconds / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}
