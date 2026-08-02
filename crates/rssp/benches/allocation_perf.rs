use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

#[path = "support/assets.rs"]
mod assets_bench;
#[path = "support/course.rs"]
mod course_bench;
#[path = "support/metadata.rs"]
mod metadata_bench;
#[path = "support/pack.rs"]
mod pack_bench;
#[path = "support/report_timing.rs"]
mod report_timing_bench;
#[path = "support/step_parity.rs"]
mod step_parity_bench;
#[path = "support/translate.rs"]
mod translate_bench;

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
    Matrix,
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
    JsonTiming,
    CourseJson,
    CourseCsv,
    CourseAnalyze,
    PackScan,
    BackgroundChanges,
    AssetFallbacks,
    SongAssets,
    TranslateMarkers,
    MetadataAnalyze,
    CustomCompile,
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
                    "matrix" => Mode::Matrix,
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
                    "json-timing" => Mode::JsonTiming,
                    "course-json" => Mode::CourseJson,
                    "course-csv" => Mode::CourseCsv,
                    "course-analyze" => Mode::CourseAnalyze,
                    "pack-scan" => Mode::PackScan,
                    "background-changes" => Mode::BackgroundChanges,
                    "asset-fallbacks" => Mode::AssetFallbacks,
                    "song-assets" => Mode::SongAssets,
                    "translate-markers" => Mode::TranslateMarkers,
                    "metadata-analyze" => Mode::MetadataAnalyze,
                    "custom-compile" => Mode::CustomCompile,
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
        | Mode::Matrix
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
        Mode::CourseJson | Mode::CourseCsv => rssp::AnalysisOptions {
            mono_threshold: 6,
            ..rssp::AnalysisOptions::default()
        },
        Mode::CourseAnalyze => rssp::AnalysisOptions::default(),
        Mode::PackScan => rssp::AnalysisOptions::default(),
        Mode::BackgroundChanges => rssp::AnalysisOptions::default(),
        Mode::AssetFallbacks => rssp::AnalysisOptions::default(),
        Mode::SongAssets => rssp::AnalysisOptions::default(),
        Mode::TranslateMarkers => rssp::AnalysisOptions::default(),
        Mode::MetadataAnalyze => rssp::AnalysisOptions::default(),
        Mode::CustomCompile => rssp::AnalysisOptions::default(),
        Mode::JsonFull => rssp::AnalysisOptions {
            mono_threshold: 6,
            ..rssp::AnalysisOptions::default()
        },
        Mode::JsonTiming => rssp::AnalysisOptions::default(),
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
            Mode::Matrix => {
                unreachable!("matrix mode uses its dedicated allocation runner")
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
            Mode::JsonTiming => {
                unreachable!("timing JSON mode uses its dedicated allocation runner")
            }
            Mode::BackgroundChanges => {
                unreachable!("background change mode uses its dedicated allocation runner")
            }
            Mode::AssetFallbacks => {
                unreachable!("asset fallback mode uses its dedicated allocation runner")
            }
            Mode::SongAssets => {
                unreachable!("song asset mode uses its dedicated allocation runner")
            }
            Mode::TranslateMarkers => {
                unreachable!("marker translation mode uses its dedicated allocation runner")
            }
            Mode::CourseJson => {
                unreachable!("course report mode uses its dedicated allocation runner")
            }
            Mode::CourseCsv => {
                unreachable!("course report mode uses its dedicated allocation runner")
            }
            Mode::CourseAnalyze => {
                unreachable!("course analysis mode uses its dedicated allocation runner")
            }
            Mode::PackScan => {
                unreachable!("pack scan mode uses its dedicated allocation runner")
            }
            Mode::MetadataAnalyze => {
                unreachable!("metadata analysis mode uses its dedicated allocation runner")
            }
            Mode::CustomCompile => {
                unreachable!("custom pattern modes use their dedicated allocation runner")
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
    if matches!(mode, Mode::CourseJson | Mode::CourseCsv) {
        let mut output = Vec::new();
        rssp::report::write_course_reports(
            course.expect("course summary should be built"),
            if matches!(mode, Mode::CourseJson) {
                rssp::report::OutputMode::JSON
            } else {
                rssp::report::OutputMode::CSV
            },
            &mut output,
        )
        .expect("course report should write");
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
        Mode::Matrix => "matrix",
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
        Mode::JsonTiming => "json-timing",
        Mode::CourseJson => "course-json",
        Mode::CourseCsv => "course-csv",
        Mode::CourseAnalyze => "course-analyze",
        Mode::PackScan => "pack-scan",
        Mode::BackgroundChanges => "background-changes",
        Mode::AssetFallbacks => "asset-fallbacks",
        Mode::SongAssets => "song-assets",
        Mode::TranslateMarkers => "translate-markers",
        Mode::MetadataAnalyze => "metadata-analyze",
        Mode::CustomCompile => "custom-compile",
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

fn run_custom_pattern_alloc(iterations: usize) {
    const UNIQUE_PATTERNS: usize = 256;
    let patterns = custom_pattern_input(UNIQUE_PATTERNS);
    black_box(rssp::patterns::compile_custom_patterns(&patterns));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let compiled = rssp::patterns::compile_custom_patterns(black_box(&patterns));
        checksum = checksum.wrapping_add(usize::from(!rssp::patterns::compiled_custom_is_empty(
            &compiled,
        )));
        black_box(compiled);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=custom-compile iters={} checksum={} elapsed_s={:.6} ",
            "throughput_patterns_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        patterns.len() as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_course_analyze_alloc(iterations: usize) {
    let fixture = course_bench::CourseFixture::new();
    let options = course_bench::clone_heavy_options();
    black_box(
        rssp::course::analyze_crs_path(
            fixture.course_path(),
            Some(fixture.songs_dir()),
            "dance-single",
            "Medium",
            options.clone(),
        )
        .expect("benchmark course should analyze"),
    );

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let summary = rssp::course::analyze_crs_path(
            black_box(fixture.course_path()),
            Some(black_box(fixture.songs_dir())),
            black_box("dance-single"),
            black_box("Medium"),
            black_box(options.clone()),
        )
        .expect("benchmark course should analyze");
        checksum = checksum.wrapping_add(summary.entries.len());
        black_box(summary);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-analyze iters={} checksum={} elapsed_s={:.6} ",
            "throughput_entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        course_bench::SONG_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_pack_scan_alloc(iterations: usize) {
    let fixture = pack_bench::PackFixture::new();
    black_box(
        rssp::pack::scan_pack_dir(fixture.pack_dir(), rssp::pack::ScanOpt::default())
            .expect("benchmark pack should scan"),
    );

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let scan = rssp::pack::scan_pack_dir(fixture.pack_dir(), rssp::pack::ScanOpt::default())
            .expect("benchmark pack should scan")
            .expect("benchmark pack should contain songs");
        checksum = checksum.wrapping_add(scan.songs.len());
        black_box(scan);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=pack-scan iters={} checksum={} elapsed_s={:.6} ",
            "throughput_songs_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        pack_bench::SONG_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_metadata_analyze_alloc(iterations: usize) {
    let fixture = metadata_bench::fixture("0.83");
    let options = metadata_bench::options();
    black_box(
        rssp::analyze(fixture.as_bytes(), "ssc", &options)
            .expect("metadata benchmark should analyze"),
    );

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let summary = rssp::analyze(
            black_box(fixture.as_bytes()),
            black_box("ssc"),
            black_box(&options),
        )
        .expect("metadata benchmark should analyze");
        checksum = checksum.wrapping_add(summary.charts.len());
        black_box(summary);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=metadata-analyze iters={} checksum={} elapsed_s={:.6} ",
            "throughput_charts_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        metadata_bench::CHART_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_timing_json_alloc(iterations: usize) {
    let fixture = report_timing_bench::fixture();
    let summary = rssp::analyze(fixture.as_bytes(), "ssc", &report_timing_bench::options())
        .expect("timing JSON benchmark should analyze");
    let mut warm_output = Vec::new();
    rssp::report::write_reports(&summary, rssp::report::OutputMode::JSON, &mut warm_output)
        .expect("timing JSON benchmark should write");
    black_box(warm_output);

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let mut output = Vec::new();
        rssp::report::write_reports(
            black_box(&summary),
            rssp::report::OutputMode::JSON,
            black_box(&mut output),
        )
        .expect("timing JSON benchmark should write");
        checksum = checksum.wrapping_add(output.len());
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=json-timing iters={} checksum={} elapsed_s={:.6} ",
            "throughput_segments_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        report_timing_bench::SEGMENT_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_background_changes_alloc(iterations: usize) {
    let fixture = assets_bench::AssetFixture::new();
    black_box(rssp::assets::resolve_background_changes_like_itg(
        fixture.song_dir(),
        fixture.simfile(),
    ));

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let changes = rssp::assets::resolve_background_changes_like_itg(
            black_box(fixture.song_dir()),
            black_box(fixture.simfile()),
        );
        checksum = checksum.wrapping_add(changes.len());
        black_box(changes);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=background-changes iters={} checksum={} elapsed_s={:.6} ",
            "throughput_changes_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        assets_bench::CHANGE_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_asset_fallbacks_alloc(iterations: usize) {
    let fixture = assets_bench::AssetFixture::new();

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let lookup = rssp::profile::file_ci(
            black_box(fixture.lookup_dir()),
            black_box(assets_bench::AssetFixture::lookup_name()),
        );
        let music =
            rssp::assets::resolve_music_path_like_itg(black_box(fixture.song_dir()), black_box(""));
        let movies = rssp::assets::resolve_background_changes_like_itg(
            black_box(fixture.song_dir()),
            black_box(b""),
        );
        checksum = checksum
            .wrapping_add(usize::from(lookup.is_some()))
            .wrapping_add(usize::from(music.is_some()))
            .wrapping_add(movies.len());
        black_box((lookup, music, movies));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=asset-fallbacks iters={} checksum={} elapsed_s={:.6} ",
            "operations_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        3.0 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_song_assets_alloc(iterations: usize) {
    let fixture = assets_bench::AssetFixture::new();
    black_box(rssp::assets::resolve_song_assets(
        fixture.image_dir(),
        "",
        "",
    ));

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let (banner, background) = rssp::assets::resolve_song_assets(
            black_box(fixture.image_dir()),
            black_box(""),
            black_box(""),
        );
        checksum = checksum
            .wrapping_add(usize::from(banner.is_some()))
            .wrapping_add(usize::from(background.is_some()));
        black_box((banner, background));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=song-assets iters={} checksum={} elapsed_s={:.6} ",
            "entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        (assets_bench::IMAGE_COUNT + assets_bench::NON_IMAGE_COUNT) as f64 * divisor
            / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_translate_markers_alloc(iterations: usize) {
    let unknown_input = translate_bench::unknown_input();
    let alias_input = translate_bench::alias_input();
    let mut unknown = unknown_input.clone();
    let mut aliases = String::with_capacity(alias_input.len());

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        rssp::translate::replace_markers_in_place(black_box(&mut unknown));
        aliases.clear();
        aliases.push_str(&alias_input);
        rssp::translate::replace_markers_in_place(black_box(&mut aliases));
        checksum = checksum
            .wrapping_add(unknown.len())
            .wrapping_add(aliases.len());
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=translate-markers iters={} checksum={} elapsed_s={:.6} ",
            "markers_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        2.0 * translate_bench::MARKER_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

struct MatrixAllocInput {
    densities: Vec<usize>,
    bpm_map: Vec<(f64, f64)>,
}

fn matrix_alloc_inputs() -> Vec<MatrixAllocInput> {
    vec![
        MatrixAllocInput {
            densities: vec![16; 4_096],
            bpm_map: vec![(0.0, 180.0)],
        },
        MatrixAllocInput {
            densities: (0..4_096).map(|index| [16, 20][index & 1]).collect(),
            bpm_map: vec![(0.0, 180.0)],
        },
        MatrixAllocInput {
            densities: (0..4_096)
                .map(|index| [16, 20, 24, 32][index & 3])
                .collect(),
            bpm_map: (0..16)
                .map(|index| (index as f64 * 1_024.0, 120.0 + (index % 7) as f64 * 15.0))
                .collect(),
        },
        MatrixAllocInput {
            densities: (0..4_096)
                .map(|index| [16, 20, 24, 32][index & 3])
                .collect(),
            bpm_map: (0..64)
                .map(|index| (index as f64 * 256.0, 80.0 + index as f64 * 2.5))
                .collect(),
        },
    ]
}

fn run_matrix_alloc_phase<T>(
    phase: &str,
    iterations: usize,
    inputs: &[MatrixAllocInput],
    mut compute: impl FnMut(&[usize], &[(f64, f64)]) -> T,
) where
    T: AsRef<[rssp::matrix::MatrixRatingInput]>,
{
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for input in inputs {
            let profile = compute(black_box(&input.densities), black_box(&input.bpm_map));
            checksum = checksum.wrapping_add(profile.as_ref().len());
            black_box(profile);
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let measures = inputs
        .iter()
        .map(|input| input.densities.len())
        .sum::<usize>() as f64
        * divisor;
    println!(
        concat!(
            "mode=matrix phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_measures_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        measures / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_matrix_alloc(iterations: usize) {
    let inputs = matrix_alloc_inputs();
    let legacy_profiles: Vec<_> = inputs
        .iter()
        .map(|input| {
            rssp::matrix::compute_matrix_profile_legacy_for_bench(&input.densities, &input.bpm_map)
        })
        .collect();
    let profiles: Vec<_> = inputs
        .iter()
        .map(|input| rssp::matrix::compute_matrix_profile(&input.densities, &input.bpm_map))
        .collect();
    let legacy_entries = legacy_profiles.iter().map(Vec::len).sum::<usize>();
    let entries = profiles
        .iter()
        .map(|profile| profile.as_ref().len())
        .sum::<usize>();
    assert_eq!(legacy_entries, entries);
    let input_bytes = std::mem::size_of::<rssp::matrix::MatrixRatingInput>();
    let legacy_storage = legacy_profiles
        .iter()
        .map(|profile| std::mem::size_of_val(profile) + profile.capacity() * input_bytes)
        .sum::<usize>();
    let storage = profiles
        .iter()
        .map(|profile| std::mem::size_of_val(profile) + profile.len() * input_bytes)
        .sum::<usize>();
    println!(
        "mode=matrix storage legacy_bytes={legacy_storage} optimized_bytes={storage} entries={entries}"
    );
    run_matrix_alloc_phase("legacy", iterations, &inputs, |densities, bpm_map| {
        rssp::matrix::compute_matrix_profile_legacy_for_bench(densities, bpm_map)
    });
    run_matrix_alloc_phase("reserved", iterations, &inputs, |densities, bpm_map| {
        rssp::matrix::compute_matrix_profile_reserved_for_bench(densities, bpm_map)
    });
    run_matrix_alloc_phase("optimized", iterations, &inputs, |densities, bpm_map| {
        rssp::matrix::compute_matrix_profile(densities, bpm_map)
    });
}

fn main() {
    let (mode, iterations) = parse_args();
    match mode {
        Mode::CustomCompile => {
            run_custom_pattern_alloc(iterations);
            return;
        }
        Mode::Matrix => {
            run_matrix_alloc(iterations);
            return;
        }
        Mode::CourseAnalyze => {
            run_course_analyze_alloc(iterations);
            return;
        }
        Mode::PackScan => {
            run_pack_scan_alloc(iterations);
            return;
        }
        Mode::BackgroundChanges => {
            run_background_changes_alloc(iterations);
            return;
        }
        Mode::AssetFallbacks => {
            run_asset_fallbacks_alloc(iterations);
            return;
        }
        Mode::SongAssets => {
            run_song_assets_alloc(iterations);
            return;
        }
        Mode::TranslateMarkers => {
            run_translate_markers_alloc(iterations);
            return;
        }
        Mode::MetadataAnalyze => {
            run_metadata_analyze_alloc(iterations);
            return;
        }
        Mode::JsonTiming => {
            run_timing_json_alloc(iterations);
            return;
        }
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
    let course_summary = matches!(mode, Mode::CourseJson | Mode::CourseCsv)
        .then(|| build_course_summary(&corpus, &options, 1_024));

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
