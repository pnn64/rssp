use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

#[allow(dead_code)]
#[path = "support/assets.rs"]
mod assets_bench;
#[path = "support/bpm_display.rs"]
mod bpm_display_bench;
#[path = "support/bpm_summary.rs"]
mod bpm_summary_bench;
#[path = "support/course.rs"]
mod course_bench;
#[path = "support/elapsed.rs"]
mod elapsed_bench;
#[path = "support/last_beat.rs"]
mod last_beat_bench;
#[path = "support/metadata.rs"]
mod metadata_bench;
#[path = "support/nps_stats.rs"]
mod nps_stats_bench;
#[path = "support/pack.rs"]
mod pack_bench;
#[path = "support/parse_dispatch.rs"]
mod parse_dispatch_bench;
#[path = "support/path_sort.rs"]
mod path_sort_bench;
#[path = "support/pattern_scratch.rs"]
mod pattern_scratch_bench;
#[path = "support/report_nps.rs"]
mod report_nps_bench;
#[path = "support/report_patterns.rs"]
mod report_patterns_bench;
#[path = "support/report_timing.rs"]
mod report_timing_bench;
#[path = "support/row_to_beat.rs"]
mod row_to_beat_bench;
#[path = "support/selectable.rs"]
mod selectable_bench;
#[path = "support/serialize.rs"]
mod serialize_bench;
#[path = "support/sm_timing.rs"]
mod sm_timing_bench;
#[path = "support/step_parity.rs"]
mod step_parity_bench;
#[path = "support/tech_prefix.rs"]
mod tech_prefix_bench;
#[path = "support/text_report.rs"]
mod text_report_bench;
#[path = "support/timing_borrow.rs"]
mod timing_borrow_bench;
#[path = "support/timing_merge.rs"]
mod timing_merge_bench;
#[path = "support/timing_rows.rs"]
mod timing_rows_bench;
#[path = "support/timing_segments.rs"]
mod timing_segments_bench;
#[path = "support/timing_sort.rs"]
mod timing_sort_bench;
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
    ParseDispatch,
    ParseAppend,
    ParseReserve,
    Fast,
    Full,
    AnalysisReuse,
    StreamOutputs,
    Matrix,
    Annotations,
    Hashes,
    Durations,
    LastBeat,
    TimingBuild,
    TimingSort,
    SmTiming,
    TimingMerge,
    TimingText,
    Serialize,
    Nps,
    NpsStats,
    NpsCursor,
    Minimize,
    Bpms,
    CleanMap,
    NormalizeMap,
    FusedMap,
    DisplayBpm,
    BpmDisplayTags,
    BpmStats,
    ElapsedEvents,
    Tech,
    TechPrefix,
    Snapshot,
    Csv,
    Json,
    JsonBpmText,
    JsonCustomPatterns,
    JsonFull,
    JsonHashBpms,
    JsonNps,
    JsonStreams,
    JsonTiming,
    CourseJson,
    CourseCsv,
    CourseParse,
    CourseEntryReserve,
    CourseMods,
    CourseSelectMods,
    CourseSelectParse,
    CourseAnalyze,
    CourseHashDedup,
    CourseStepType,
    CourseTitleMatch,
    CourseBanner,
    CourseResolve,
    PackIni,
    PackHintNormalize,
    PackRoot,
    PackScan,
    PathSort,
    BackgroundChanges,
    AssetFallbacks,
    SongAssets,
    TranslateMarkers,
    MetadataAnalyze,
    TextReport,
    CustomCompile,
    DefaultPatternDfa,
    ParitySingle,
    ParityDouble,
    ParitySingleHolds,
    ParityDoubleHolds,
}

struct SimInput {
    extension: &'static str,
    raw: Vec<u8>,
}

struct MinimizeInput {
    lanes: usize,
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
                    "parse-dispatch" => Mode::ParseDispatch,
                    "parse-append" => Mode::ParseAppend,
                    "parse-reserve" => Mode::ParseReserve,
                    "fast" => Mode::Fast,
                    "analysis-reuse" => Mode::AnalysisReuse,
                    "stream-outputs" => Mode::StreamOutputs,
                    "matrix" => Mode::Matrix,
                    "annotations" => Mode::Annotations,
                    "hashes" => Mode::Hashes,
                    "durations" => Mode::Durations,
                    "last-beat" => Mode::LastBeat,
                    "timing-build" => Mode::TimingBuild,
                    "timing-sort" => Mode::TimingSort,
                    "sm-timing" => Mode::SmTiming,
                    "timing-merge" => Mode::TimingMerge,
                    "timing-text" => Mode::TimingText,
                    "serialize" => Mode::Serialize,
                    "nps" => Mode::Nps,
                    "nps-stats" => Mode::NpsStats,
                    "nps-cursor" => Mode::NpsCursor,
                    "minimize" => Mode::Minimize,
                    "bpms" => Mode::Bpms,
                    "clean-map" => Mode::CleanMap,
                    "normalize-map" => Mode::NormalizeMap,
                    "fused-map" => Mode::FusedMap,
                    "display-bpm" => Mode::DisplayBpm,
                    "bpm-display-tags" => Mode::BpmDisplayTags,
                    "bpm-stats" => Mode::BpmStats,
                    "elapsed-events" => Mode::ElapsedEvents,
                    "tech" => Mode::Tech,
                    "tech-prefix" => Mode::TechPrefix,
                    "snapshot" => Mode::Snapshot,
                    "csv" => Mode::Csv,
                    "json" => Mode::Json,
                    "json-bpm-text" => Mode::JsonBpmText,
                    "json-custom-patterns" => Mode::JsonCustomPatterns,
                    "json-full" => Mode::JsonFull,
                    "json-hash-bpms" => Mode::JsonHashBpms,
                    "json-nps" => Mode::JsonNps,
                    "json-streams" => Mode::JsonStreams,
                    "json-timing" => Mode::JsonTiming,
                    "course-json" => Mode::CourseJson,
                    "course-csv" => Mode::CourseCsv,
                    "course-parse" => Mode::CourseParse,
                    "course-entry-reserve" => Mode::CourseEntryReserve,
                    "course-mods" => Mode::CourseMods,
                    "course-select-mods" => Mode::CourseSelectMods,
                    "course-select-parse" => Mode::CourseSelectParse,
                    "course-analyze" => Mode::CourseAnalyze,
                    "course-hash-dedup" => Mode::CourseHashDedup,
                    "course-stepstype" => Mode::CourseStepType,
                    "course-title" => Mode::CourseTitleMatch,
                    "course-banner" => Mode::CourseBanner,
                    "course-resolve" => Mode::CourseResolve,
                    "pack-ini" => Mode::PackIni,
                    "pack-hint" => Mode::PackHintNormalize,
                    "pack-root" => Mode::PackRoot,
                    "pack-scan" => Mode::PackScan,
                    "path-sort" => Mode::PathSort,
                    "background-changes" => Mode::BackgroundChanges,
                    "asset-fallbacks" => Mode::AssetFallbacks,
                    "song-assets" => Mode::SongAssets,
                    "translate-markers" => Mode::TranslateMarkers,
                    "metadata-analyze" => Mode::MetadataAnalyze,
                    "text-report" => Mode::TextReport,
                    "custom-compile" => Mode::CustomCompile,
                    "default-pattern-dfa" => Mode::DefaultPatternDfa,
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
        | Mode::Minimize
        | Mode::Bpms
        | Mode::Tech
        | Mode::TechPrefix
        | Mode::Snapshot
        | Mode::Csv
        | Mode::Json => rssp::AnalysisOptions {
            mono_threshold: 6,
            compute_tech_counts: false,
            compute_pattern_counts: false,
            ..rssp::AnalysisOptions::default()
        },
        Mode::LastBeat
        | Mode::BpmStats
        | Mode::CleanMap
        | Mode::NormalizeMap
        | Mode::FusedMap
        | Mode::DisplayBpm
        | Mode::BpmDisplayTags
        | Mode::ElapsedEvents
        | Mode::TimingBuild
        | Mode::TimingSort
        | Mode::SmTiming
        | Mode::TimingMerge
        | Mode::TimingText
        | Mode::Serialize
        | Mode::NpsStats
        | Mode::NpsCursor => rssp::AnalysisOptions::default(),
        Mode::Full | Mode::AnalysisReuse => rssp::AnalysisOptions {
            mono_threshold: 6,
            ..rssp::AnalysisOptions::default()
        },
        Mode::StreamOutputs => rssp::AnalysisOptions::default(),
        Mode::CourseJson | Mode::CourseCsv => rssp::AnalysisOptions {
            mono_threshold: 6,
            ..rssp::AnalysisOptions::default()
        },
        Mode::CourseParse
        | Mode::CourseEntryReserve
        | Mode::CourseMods
        | Mode::CourseSelectMods
        | Mode::CourseSelectParse
        | Mode::CourseAnalyze
        | Mode::CourseHashDedup => rssp::AnalysisOptions::default(),
        Mode::CourseStepType
        | Mode::CourseTitleMatch
        | Mode::CourseBanner
        | Mode::CourseResolve => rssp::AnalysisOptions::default(),
        Mode::PackIni | Mode::PackHintNormalize => rssp::AnalysisOptions::default(),
        Mode::PackRoot => rssp::AnalysisOptions::default(),
        Mode::PackScan => rssp::AnalysisOptions::default(),
        Mode::PathSort => rssp::AnalysisOptions::default(),
        Mode::BackgroundChanges => rssp::AnalysisOptions::default(),
        Mode::AssetFallbacks => rssp::AnalysisOptions::default(),
        Mode::SongAssets => rssp::AnalysisOptions::default(),
        Mode::TranslateMarkers => rssp::AnalysisOptions::default(),
        Mode::MetadataAnalyze | Mode::TextReport => rssp::AnalysisOptions::default(),
        Mode::CustomCompile | Mode::DefaultPatternDfa => rssp::AnalysisOptions::default(),
        Mode::JsonFull => rssp::AnalysisOptions {
            mono_threshold: 6,
            ..rssp::AnalysisOptions::default()
        },
        Mode::JsonBpmText
        | Mode::JsonCustomPatterns
        | Mode::JsonHashBpms
        | Mode::JsonNps
        | Mode::JsonStreams
        | Mode::JsonTiming => rssp::AnalysisOptions::default(),
        Mode::Annotations => rssp::AnalysisOptions {
            mono_threshold: 6,
            compute_note_annotations: true,
            ..rssp::AnalysisOptions::default()
        },
        Mode::ParitySingle
        | Mode::ParityDouble
        | Mode::ParitySingleHolds
        | Mode::ParityDoubleHolds => rssp::AnalysisOptions::default(),
        Mode::ParseDispatch | Mode::ParseAppend | Mode::ParseReserve => {
            rssp::AnalysisOptions::default()
        }
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
            Mode::BpmStats
            | Mode::CleanMap
            | Mode::NormalizeMap
            | Mode::FusedMap
            | Mode::DisplayBpm
            | Mode::BpmDisplayTags => {
                unreachable!("mode uses its dedicated allocation runner")
            }
            Mode::TimingSort => unreachable!("mode uses its dedicated allocation runner"),
            Mode::ParseDispatch | Mode::ParseAppend | Mode::ParseReserve => {
                unreachable!("mode uses its dedicated allocation runner")
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
            Mode::TechPrefix => {
                unreachable!("tech prefix mode uses its dedicated allocation runner")
            }
            Mode::Matrix
            | Mode::AnalysisReuse
            | Mode::StreamOutputs
            | Mode::Serialize
            | Mode::NpsStats
            | Mode::NpsCursor => {
                unreachable!("mode uses its dedicated allocation runner")
            }
            Mode::Minimize => {
                unreachable!("minimize mode uses its paired allocation runner")
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
            Mode::JsonNps => {
                unreachable!("NPS JSON mode uses its dedicated allocation runner")
            }
            Mode::JsonBpmText => {
                unreachable!("BPM text JSON mode uses its dedicated allocation runner")
            }
            Mode::JsonCustomPatterns => {
                unreachable!("custom pattern JSON mode uses its dedicated allocation runner")
            }
            Mode::JsonHashBpms => {
                unreachable!("hash BPM JSON mode uses its dedicated allocation runner")
            }
            Mode::JsonStreams => {
                unreachable!("stream JSON mode uses its dedicated allocation runner")
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
            Mode::CourseParse | Mode::CourseEntryReserve => {
                unreachable!("course parse mode uses its dedicated allocation runner")
            }
            Mode::CourseMods => {
                unreachable!("course modifier mode uses its dedicated allocation runner")
            }
            Mode::CourseSelectMods => {
                unreachable!("course selection modifier mode uses its dedicated allocation runner")
            }
            Mode::CourseSelectParse => {
                unreachable!("course selection parse mode uses its dedicated allocation runner")
            }
            Mode::CourseAnalyze => {
                unreachable!("course analysis mode uses its dedicated allocation runner")
            }
            Mode::CourseStepType => {
                unreachable!("course step-type mode uses its dedicated allocation runner")
            }
            Mode::ElapsedEvents => {
                unreachable!("elapsed event mode uses its dedicated allocation runner")
            }
            Mode::CourseTitleMatch => {
                unreachable!("course title mode uses its dedicated allocation runner")
            }
            Mode::CourseBanner => {
                unreachable!("course banner mode uses its dedicated allocation runner")
            }
            Mode::CourseResolve => {
                unreachable!("course resolve mode uses its dedicated allocation runner")
            }
            Mode::PackIni => {
                unreachable!("Pack.ini mode uses its dedicated allocation runner")
            }
            Mode::PackHintNormalize => {
                unreachable!("pack hint mode uses its dedicated allocation runner")
            }
            Mode::PackRoot => {
                unreachable!("pack root mode uses its dedicated allocation runner")
            }
            Mode::PackScan => {
                unreachable!("pack scan mode uses its dedicated allocation runner")
            }
            Mode::MetadataAnalyze => {
                unreachable!("metadata analysis mode uses its dedicated allocation runner")
            }
            Mode::TextReport => {
                unreachable!("text report mode uses its dedicated allocation runner")
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
        Mode::ParseDispatch => "parse-dispatch",
        Mode::ParseAppend => "parse-append",
        Mode::ParseReserve => "parse-reserve",
        Mode::Fast => "fast",
        Mode::Full => "full",
        Mode::AnalysisReuse => "analysis-reuse",
        Mode::StreamOutputs => "stream-outputs",
        Mode::Matrix => "matrix",
        Mode::Annotations => "annotations",
        Mode::Hashes => "hashes",
        Mode::Durations => "durations",
        Mode::LastBeat => "last-beat",
        Mode::TimingBuild => "timing-build",
        Mode::TimingSort => "timing-sort",
        Mode::SmTiming => "sm-timing",
        Mode::TimingMerge => "timing-merge",
        Mode::TimingText => "timing-text",
        Mode::Serialize => "serialize",
        Mode::Nps => "nps",
        Mode::NpsStats => "nps-stats",
        Mode::NpsCursor => "nps-cursor",
        Mode::Minimize => "minimize",
        Mode::Bpms => "bpms",
        Mode::CleanMap => "clean-map",
        Mode::NormalizeMap => "normalize-map",
        Mode::FusedMap => "fused-map",
        Mode::DisplayBpm => "display-bpm",
        Mode::BpmDisplayTags => "bpm-display-tags",
        Mode::BpmStats => "bpm-stats",
        Mode::ElapsedEvents => "elapsed-events",
        Mode::Tech => "tech",
        Mode::TechPrefix => "tech-prefix",
        Mode::Snapshot => "snapshot",
        Mode::Csv => "csv",
        Mode::Json => "json",
        Mode::JsonBpmText => "json-bpm-text",
        Mode::JsonCustomPatterns => "json-custom-patterns",
        Mode::JsonFull => "json-full",
        Mode::JsonHashBpms => "json-hash-bpms",
        Mode::JsonNps => "json-nps",
        Mode::JsonStreams => "json-streams",
        Mode::JsonTiming => "json-timing",
        Mode::CourseJson => "course-json",
        Mode::CourseCsv => "course-csv",
        Mode::CourseParse => "course-parse",
        Mode::CourseEntryReserve => "course-entry-reserve",
        Mode::CourseMods => "course-mods",
        Mode::CourseSelectMods => "course-select-mods",
        Mode::CourseSelectParse => "course-select-parse",
        Mode::CourseAnalyze => "course-analyze",
        Mode::CourseHashDedup => "course-hash-dedup",
        Mode::CourseStepType => "course-stepstype",
        Mode::CourseTitleMatch => "course-title",
        Mode::CourseBanner => "course-banner",
        Mode::CourseResolve => "course-resolve",
        Mode::PackIni => "pack-ini",
        Mode::PackHintNormalize => "pack-hint",
        Mode::PackRoot => "pack-root",
        Mode::PackScan => "pack-scan",
        Mode::PathSort => "path-sort",
        Mode::BackgroundChanges => "background-changes",
        Mode::AssetFallbacks => "asset-fallbacks",
        Mode::SongAssets => "song-assets",
        Mode::TranslateMarkers => "translate-markers",
        Mode::MetadataAnalyze => "metadata-analyze",
        Mode::TextReport => "text-report",
        Mode::CustomCompile => "custom-compile",
        Mode::DefaultPatternDfa => "default-pattern-dfa",
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

fn run_note_parse_phase(data: &[u8], phase: &str, iterations: usize, fused: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        let (count, hash) = black_box(rssp::step_parity::parse_notes_for_bench(
            black_box(data),
            4,
            fused,
        ));
        checksum = checksum.wrapping_add(hash ^ count as u64);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=parity-note-parse phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mrows_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        step_parity_bench::NOTE_DATA_ROW_COUNT as f64 * divisor
            / elapsed.as_secs_f64()
            / 1_000_000.0,
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_note_parse_alloc(iterations: usize) {
    let data = step_parity_bench::note_data();
    step_parity_bench::assert_note_data_behavior(&data);
    run_note_parse_phase(&data, "materialized", iterations, false);
    run_note_parse_phase(&data, "fused", iterations, true);
}

fn run_perm_build_phase(
    lanes: usize,
    phase: &str,
    iterations: usize,
    build: fn(usize) -> Option<(usize, u64)>,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    let mut entries = 0usize;
    for _ in 0..iterations {
        let built = black_box(build(black_box(lanes))).expect("supported parity layout");
        entries = built.0;
        checksum ^= black_box(built.1);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=parity-cache phase={} lanes={} iters={} entries={} checksum={} ",
            "elapsed_s={:.6} throughput_tables_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        lanes,
        iterations,
        entries,
        checksum,
        elapsed.as_secs_f64(),
        divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_perm_build_alloc(lanes: usize, iterations: usize) {
    assert!(rssp::step_parity::perm_builds_match_for_bench(lanes));
    run_perm_build_phase(
        lanes,
        "legacy",
        iterations,
        rssp::step_parity::legacy_perm_build_for_bench,
    );
    run_perm_build_phase(
        lanes,
        "packed",
        iterations,
        rssp::step_parity::packed_perm_build_for_bench,
    );
}

fn run_wide_hold_alloc<const LANES: usize>(
    mode: &str,
    rows: &[[u8; LANES]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    iterations: usize,
) -> rssp::TechCounts {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut scratch = rssp::step_parity::wide_hold_timing_rows_scratch::<LANES>()
        .expect("supported parity layout");
    let counts = black_box(rssp::step_parity::analyze_timing_rows_wide_holds_for_bench(
        black_box(rows),
        black_box(beats),
        black_box(timing),
        true,
        black_box(&mut scratch),
    ));
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode,
        "hold-heads-wide-cold",
        1,
        rows.len(),
        elapsed,
        before,
        after,
    );

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(rssp::step_parity::analyze_timing_rows_wide_holds_for_bench(
            black_box(rows),
            black_box(beats),
            black_box(timing),
            true,
            black_box(&mut scratch),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode,
        "hold-heads-wide-reused",
        iterations,
        rows.len(),
        elapsed,
        before,
        after,
    );
    counts
}

fn run_dense_hold_alloc<const LANES: usize>(
    mode: &str,
    rows: &[[u8; LANES]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    iterations: usize,
) -> rssp::TechCounts {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut scratch =
        rssp::step_parity::dense_hold_timing_scratch::<LANES>().expect("supported parity layout");
    let counts = black_box(rssp::step_parity::analyze_dense_holds_for_bench(
        black_box(rows),
        black_box(beats),
        black_box(timing),
        true,
        black_box(&mut scratch),
    ));
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode,
        "hold-heads-dense-cold",
        1,
        rows.len(),
        elapsed,
        before,
        after,
    );

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(rssp::step_parity::analyze_dense_holds_for_bench(
            black_box(rows),
            black_box(beats),
            black_box(timing),
            true,
            black_box(&mut scratch),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode,
        "hold-heads-dense-reused",
        iterations,
        rows.len(),
        elapsed,
        before,
        after,
    );
    counts
}

fn run_growing_alloc<const LANES: usize>(
    mode: &str,
    rows: &[[u8; LANES]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    has_holds: bool,
    iterations: usize,
) -> rssp::TechCounts {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut scratch =
        rssp::step_parity::growing_timing_scratch::<LANES>().expect("supported parity layout");
    let counts = black_box(rssp::step_parity::analyze_growing_for_bench(
        black_box(rows),
        black_box(beats),
        black_box(timing),
        has_holds,
        black_box(&mut scratch),
    ));
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode,
        "workspace-growing-cold",
        1,
        rows.len(),
        elapsed,
        before,
        after,
    );

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(rssp::step_parity::analyze_growing_for_bench(
            black_box(rows),
            black_box(beats),
            black_box(timing),
            has_holds,
            black_box(&mut scratch),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode,
        "workspace-growing-reused",
        iterations,
        rows.len(),
        elapsed,
        before,
        after,
    );
    counts
}

fn run_arena_transition<const LANES: usize>(
    mode: &str,
    phase: &str,
    rows: &[[u8; LANES]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    has_holds: bool,
    legacy_growth: bool,
) -> rssp::TechCounts {
    let mut scratch =
        rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
    let warm_len = (rows.len() / 8).max(1);
    black_box(rssp::step_parity::analyze_arena_for_bench(
        black_box(&rows[..warm_len]),
        black_box(&beats[..warm_len]),
        black_box(timing),
        has_holds,
        legacy_growth,
        black_box(&mut scratch),
    ));

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let counts = black_box(rssp::step_parity::analyze_arena_for_bench(
        black_box(rows),
        black_box(beats),
        black_box(timing),
        has_holds,
        legacy_growth,
        black_box(&mut scratch),
    ));
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(mode, phase, 1, rows.len(), elapsed, before, after);
    counts
}

fn run_double_decode_alloc<const LANES: usize>(
    mode: &str,
    phase: &str,
    rows: &[[u8; LANES]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    has_holds: bool,
    legacy_decode: bool,
    iterations: usize,
) -> rssp::TechCounts {
    let mut scratch =
        rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
    let counts = black_box(rssp::step_parity::analyze_double_decode_for_bench(
        black_box(rows),
        black_box(beats),
        black_box(timing),
        has_holds,
        legacy_decode,
        black_box(&mut scratch),
    ));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(rssp::step_parity::analyze_double_decode_for_bench(
            black_box(rows),
            black_box(beats),
            black_box(timing),
            has_holds,
            legacy_decode,
            black_box(&mut scratch),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(mode, phase, iterations, rows.len(), elapsed, before, after);
    counts
}

fn run_double_result_alloc<const LANES: usize>(
    mode: &str,
    phase: &str,
    rows: &[[u8; LANES]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    has_holds: bool,
    legacy_result: bool,
    iterations: usize,
) -> rssp::TechCounts {
    let mut scratch =
        rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
    let counts = black_box(rssp::step_parity::analyze_double_result_for_bench(
        black_box(rows),
        black_box(beats),
        black_box(timing),
        has_holds,
        legacy_result,
        black_box(&mut scratch),
    ));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(rssp::step_parity::analyze_double_result_for_bench(
            black_box(rows),
            black_box(beats),
            black_box(timing),
            has_holds,
            legacy_result,
            black_box(&mut scratch),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(mode, phase, iterations, rows.len(), elapsed, before, after);
    counts
}

fn run_double_tap_key_alloc<const LANES: usize>(
    mode: &str,
    phase: &str,
    rows: &[[u8; LANES]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    has_holds: bool,
    legacy_key: bool,
    iterations: usize,
) -> rssp::TechCounts {
    let mut scratch =
        rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
    let counts = black_box(rssp::step_parity::analyze_double_tap_key_for_bench(
        black_box(rows),
        black_box(beats),
        black_box(timing),
        has_holds,
        legacy_key,
        black_box(&mut scratch),
    ));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(rssp::step_parity::analyze_double_tap_key_for_bench(
            black_box(rows),
            black_box(beats),
            black_box(timing),
            has_holds,
            legacy_key,
            black_box(&mut scratch),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(mode, phase, iterations, rows.len(), elapsed, before, after);
    counts
}

fn run_double_tap_cost_alloc<const LANES: usize>(
    mode: &str,
    phase: &str,
    rows: &[[u8; LANES]],
    beats: &[f32],
    timing: &rssp::timing::TimingData,
    has_holds: bool,
    legacy_cost: bool,
    iterations: usize,
) -> rssp::TechCounts {
    let mut scratch =
        rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
    let counts = black_box(rssp::step_parity::analyze_double_tap_cost_for_bench(
        black_box(rows),
        black_box(beats),
        black_box(timing),
        has_holds,
        legacy_cost,
        black_box(&mut scratch),
    ));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(rssp::step_parity::analyze_double_tap_cost_for_bench(
            black_box(rows),
            black_box(beats),
            black_box(timing),
            has_holds,
            legacy_cost,
            black_box(&mut scratch),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(mode, phase, iterations, rows.len(), elapsed, before, after);
    counts
}

fn run_parity_alloc<const LANES: usize>(
    mode: &str,
    row_count: usize,
    masks: &[u8],
    has_holds: bool,
    iterations: usize,
) {
    run_perm_build_alloc(LANES, iterations);
    let rows = if has_holds {
        step_parity_bench::hold_rows::<LANES>(row_count, masks)
    } else {
        step_parity_bench::rows::<LANES>(row_count, masks)
    };
    let beats = step_parity_bench::beats(row_count);
    let timing = step_parity_bench::timing();

    // Initialize the immutable layout/permutation cache outside all samples.
    drop(rssp::step_parity::timing_rows_scratch::<LANES>());

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut legacy =
        rssp::step_parity::legacy_timing_rows_scratch::<LANES>().expect("supported parity layout");
    black_box(rssp::step_parity::analyze_timing_rows_legacy_for_bench(
        black_box(&rows),
        black_box(&beats),
        black_box(&timing),
        has_holds,
        black_box(&mut legacy),
    ));
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(mode, "legacy-cold", 1, row_count, elapsed, before, after);

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(rssp::step_parity::analyze_timing_rows_legacy_for_bench(
            black_box(&rows),
            black_box(&beats),
            black_box(&timing),
            has_holds,
            black_box(&mut legacy),
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode,
        "legacy-reused",
        iterations,
        row_count,
        elapsed,
        before,
        after,
    );

    let wide_counts =
        has_holds.then(|| run_wide_hold_alloc(mode, &rows, &beats, &timing, iterations));
    let dense_counts =
        has_holds.then(|| run_dense_hold_alloc(mode, &rows, &beats, &timing, iterations));
    let growing_counts = (LANES == 4)
        .then(|| run_growing_alloc(mode, &rows, &beats, &timing, has_holds, iterations));
    let arena_counts = (LANES == 4).then(|| {
        let sampled = run_arena_transition(
            mode,
            "arena-sampled-growth",
            &rows,
            &beats,
            &timing,
            has_holds,
            true,
        );
        let learned = run_arena_transition(
            mode,
            "arena-learned-rebuild",
            &rows,
            &beats,
            &timing,
            has_holds,
            false,
        );
        assert_eq!(learned, sampled);
        learned
    });
    let double_decode_counts = (LANES == 8).then(|| {
        let scalar = run_double_decode_alloc(
            mode,
            "double-decode-scalar",
            &rows,
            &beats,
            &timing,
            has_holds,
            true,
            iterations,
        );
        let chunked = run_double_decode_alloc(
            mode,
            "double-decode-chunked",
            &rows,
            &beats,
            &timing,
            has_holds,
            false,
            iterations,
        );
        assert_eq!(chunked, scalar);
        chunked
    });
    let double_result_counts = (LANES == 8).then(|| {
        let materialized = run_double_result_alloc(
            mode,
            "double-result-materialized",
            &rows,
            &beats,
            &timing,
            has_holds,
            true,
            iterations,
        );
        let packed = run_double_result_alloc(
            mode,
            "double-result-packed",
            &rows,
            &beats,
            &timing,
            has_holds,
            false,
            iterations,
        );
        assert_eq!(packed, materialized);
        packed
    });
    let double_tap_key_counts = (LANES == 8).then(|| {
        let general = run_double_tap_key_alloc(
            mode,
            "double-tap-key-general",
            &rows,
            &beats,
            &timing,
            has_holds,
            true,
            iterations,
        );
        let direct = run_double_tap_key_alloc(
            mode,
            "double-tap-key-direct",
            &rows,
            &beats,
            &timing,
            has_holds,
            false,
            iterations,
        );
        assert_eq!(direct, general);
        direct
    });
    let double_tap_cost_counts = (LANES == 8).then(|| {
        let general = run_double_tap_cost_alloc(
            mode,
            "double-tap-cost-general",
            &rows,
            &beats,
            &timing,
            has_holds,
            true,
            iterations,
        );
        let direct = run_double_tap_cost_alloc(
            mode,
            "double-tap-cost-direct",
            &rows,
            &beats,
            &timing,
            has_holds,
            false,
            iterations,
        );
        assert_eq!(direct, general);
        direct
    });

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut scratch =
        rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
    let compact_counts = black_box(rssp::step_parity::analyze_timing_rows_known_holds(
        black_box(&rows),
        black_box(&beats),
        black_box(&timing),
        has_holds,
        black_box(&mut scratch),
    ));
    if let Some(wide_counts) = wide_counts {
        assert_eq!(compact_counts, wide_counts);
    }
    if let Some(dense_counts) = dense_counts {
        assert_eq!(compact_counts, dense_counts);
    }
    if let Some(growing_counts) = growing_counts {
        assert_eq!(compact_counts, growing_counts);
    }
    if let Some(arena_counts) = arena_counts {
        assert_eq!(compact_counts, arena_counts);
    }
    if let Some(double_decode_counts) = double_decode_counts {
        assert_eq!(compact_counts, double_decode_counts);
    }
    if let Some(double_result_counts) = double_result_counts {
        assert_eq!(compact_counts, double_result_counts);
    }
    if let Some(double_tap_key_counts) = double_tap_key_counts {
        assert_eq!(compact_counts, double_tap_key_counts);
    }
    if let Some(double_tap_cost_counts) = double_tap_cost_counts {
        assert_eq!(compact_counts, double_tap_cost_counts);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(mode, "compact-cold", 1, row_count, elapsed, before, after);

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
        mode,
        "compact-reused",
        iterations,
        row_count,
        elapsed,
        before,
        after,
    );

    if LANES == 4 && !has_holds {
        for (phase, legacy_tap_path) in [
            ("tap-path-legacy-reused", true),
            ("tap-path-specialized-reused", false),
        ] {
            let mut tap_scratch =
                rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
            black_box(rssp::step_parity::analyze_timing_rows_tap_path_for_bench(
                &rows,
                &beats,
                &timing,
                has_holds,
                legacy_tap_path,
                &mut tap_scratch,
            ));
            reset_counters();
            let before = Counters::read();
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(rssp::step_parity::analyze_timing_rows_tap_path_for_bench(
                    black_box(&rows),
                    black_box(&beats),
                    black_box(&timing),
                    has_holds,
                    legacy_tap_path,
                    black_box(&mut tap_scratch),
                ));
            }
            let elapsed = start.elapsed();
            let after = Counters::read();
            print_parity_alloc(mode, phase, iterations, row_count, elapsed, before, after);
        }
    }

    if LANES == 4 {
        for (phase, legacy_hash) in [
            ("row-hash-legacy-reused", true),
            ("row-hash-folded-reused", false),
        ] {
            let mut hash_scratch =
                rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
            black_box(rssp::step_parity::analyze_timing_rows_hash_for_bench(
                &rows,
                &beats,
                &timing,
                has_holds,
                legacy_hash,
                &mut hash_scratch,
            ));
            reset_counters();
            let before = Counters::read();
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(rssp::step_parity::analyze_timing_rows_hash_for_bench(
                    black_box(&rows),
                    black_box(&beats),
                    black_box(&timing),
                    has_holds,
                    legacy_hash,
                    black_box(&mut hash_scratch),
                ));
            }
            let elapsed = start.elapsed();
            let after = Counters::read();
            print_parity_alloc(mode, phase, iterations, row_count, elapsed, before, after);
        }
    }

    let mut annotation_scratch =
        rssp::step_parity::timing_rows_scratch::<LANES>().expect("supported parity layout");
    drop(
        rssp::step_parity::analyze_and_annotate_timing_rows_known_holds(
            &rows,
            &beats,
            &timing,
            has_holds,
            &mut annotation_scratch,
        ),
    );
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(
            rssp::step_parity::analyze_and_annotate_timing_rows_known_holds(
                black_box(&rows),
                black_box(&beats),
                black_box(&timing),
                has_holds,
                black_box(&mut annotation_scratch),
            ),
        );
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode,
        "annotations-owned",
        iterations,
        row_count,
        elapsed,
        before,
        after,
    );

    let mut annotations = Vec::new();
    rssp::step_parity::analyze_and_annotate_timing_rows_known_holds_in(
        &rows,
        &beats,
        &timing,
        has_holds,
        &mut annotation_scratch,
        &mut annotations,
    );
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(
            rssp::step_parity::analyze_and_annotate_timing_rows_known_holds_in(
                black_box(&rows),
                black_box(&beats),
                black_box(&timing),
                has_holds,
                black_box(&mut annotation_scratch),
                black_box(&mut annotations),
            ),
        );
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    print_parity_alloc(
        mode,
        "annotations-reused",
        iterations,
        row_count,
        elapsed,
        before,
        after,
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

fn run_custom_pattern_alloc_phase(
    phase: &str,
    iterations: usize,
    patterns: &[String],
    compile: impl Fn(&[String]) -> rssp::patterns::CompiledCustomPatterns,
) {
    black_box(compile(patterns));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let compiled = compile(black_box(patterns));
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
            "mode=custom-compile phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_patterns_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
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

fn run_pattern_count_phase(
    phase: &str,
    iterations: usize,
    rows: &[[u8; 4]],
    mut analyze: impl FnMut(&[[u8; 4]]) -> rssp::patterns::PatternAnalysis,
) {
    black_box(analyze(rows));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u32;
    for _ in 0..iterations {
        let analysis = analyze(black_box(rows));
        checksum = checksum.wrapping_add(
            analysis
                .custom_patterns
                .iter()
                .map(|entry| entry.count)
                .sum::<u32>()
                + analysis.detected_patterns.iter().sum::<u32>(),
        );
        black_box(analysis);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=custom-compile stage=count phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_rows_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        rows.len() as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_default_dfa_phase(
    phase: &str,
    iterations: usize,
    bitmasks: &[u8],
    detect: impl Fn(&[u8]) -> rssp::patterns::PatternCounts,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u32;
    for _ in 0..iterations {
        let counts = detect(black_box(bitmasks));
        checksum = checksum.wrapping_add(counts.iter().sum::<u32>());
        black_box(counts);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=default-pattern-dfa phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_builds_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_default_dfa_alloc(iterations: usize) {
    let rows = pattern_scratch_bench::rows();
    let bitmasks: Vec<_> = rows
        .iter()
        .map(|row| {
            u8::from(row[0] != b'0')
                | (u8::from(row[1] != b'0') << 1)
                | (u8::from(row[2] != b'0') << 2)
                | (u8::from(row[3] != b'0') << 3)
        })
        .collect();
    let input = &bitmasks[..256.min(bitmasks.len())];
    let expected = rssp::patterns::detect_default_patterns_runtime_build_for_bench(&bitmasks);
    assert_eq!(rssp::patterns::detect_default_patterns(&bitmasks), expected);
    assert_eq!(
        rssp::patterns::detect_default_patterns_heap_for_bench(&bitmasks),
        expected
    );
    let (heap_bytes, static_bytes) = rssp::patterns::default_pattern_dfa_sizes_for_bench();
    println!(
        "mode=default-pattern-dfa heap_payload_bytes={} static_bytes={}",
        heap_bytes, static_bytes
    );
    run_default_dfa_phase("runtime-build", iterations, input, |input| {
        rssp::patterns::detect_default_patterns_runtime_build_for_bench(input)
    });
    run_default_dfa_phase("static", iterations, input, |input| {
        rssp::patterns::detect_default_patterns(input)
    });
}

fn run_custom_pattern_alloc(iterations: usize) {
    const UNIQUE_PATTERNS: usize = 256;
    let patterns = custom_pattern_input(UNIQUE_PATTERNS);
    run_custom_pattern_alloc_phase("legacy", iterations, &patterns, |patterns| {
        rssp::patterns::compile_custom_patterns_legacy_for_bench(patterns)
    });
    run_custom_pattern_alloc_phase("growing-dfa", iterations, &patterns, |patterns| {
        rssp::patterns::compile_custom_patterns_growing_dfa_for_bench(patterns)
    });
    run_custom_pattern_alloc_phase("presized-dfa", iterations, &patterns, |patterns| {
        rssp::patterns::compile_custom_patterns(patterns)
    });
    let compiled = rssp::patterns::compile_custom_patterns(&patterns);
    let rows = pattern_scratch_bench::rows();
    pattern_scratch_bench::assert_behavior(&rows, 6, &compiled);
    run_pattern_count_phase("allocating", iterations, &rows, |rows| {
        rssp::patterns::analyze_patterns_from_rows(rows, 6, &compiled)
    });
    let mut counts = Vec::new();
    run_pattern_count_phase("reused", iterations, &rows, |rows| {
        rssp::patterns::analyze_patterns_from_rows_with_scratch(rows, 6, &compiled, &mut counts)
    });
    run_prepared_analysis_alloc(iterations, patterns);
}

fn run_prepared_phase(
    phase: &str,
    iterations: usize,
    mut analyze: impl FnMut() -> Result<rssp::SimfileSummary, String>,
) {
    black_box(analyze().expect("batch fixture should analyze"));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let summary = analyze().expect("batch fixture should analyze");
        checksum = checksum.wrapping_add(
            summary
                .charts
                .iter()
                .map(|chart| chart.custom_patterns.len())
                .sum::<usize>(),
        );
        black_box(summary);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=prepared-analysis phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_files_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_prepared_analysis_alloc(iterations: usize, patterns: Vec<String>) {
    const FIXTURE: &[u8] = b"#VERSION:0.83;#TITLE:Batch;#BPMS:0=120;\
#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Challenge;#METER:10;\
#NOTES:\n1000\n0100\n0010\n0001\n;";
    let options = rssp::AnalysisOptions {
        custom_patterns: patterns,
        compute_tech_counts: false,
        ..rssp::AnalysisOptions::default()
    };
    let prepared = rssp::PreparedAnalysis::new(options.clone());
    let mut prepared_scratch = rssp::AnalysisScratch::default();
    let expected = rssp::analyze(FIXTURE, "ssc", &options.clone())
        .expect("fresh batch analysis should succeed");
    let actual = rssp::analyze_prepared_in(FIXTURE, "ssc", &prepared, &mut prepared_scratch)
        .expect("prepared batch analysis should succeed");
    let (mut expected_json, mut actual_json) = (Vec::new(), Vec::new());
    rssp::report::write_reports(
        &expected,
        rssp::report::OutputMode::JSON,
        &mut expected_json,
    )
    .expect("fresh batch summary should serialize");
    rssp::report::write_reports(&actual, rssp::report::OutputMode::JSON, &mut actual_json)
        .expect("prepared batch summary should serialize");
    assert_eq!(actual_json, expected_json);
    run_prepared_phase("fresh-each-file", iterations, || {
        rssp::analyze(FIXTURE, "ssc", &options.clone())
    });
    run_prepared_phase("prepared-reused", iterations, || {
        rssp::analyze_prepared_in(FIXTURE, "ssc", &prepared, &mut prepared_scratch)
    });

    let chart: Vec<_> = options
        .custom_patterns
        .iter()
        .map(|pattern| rssp::patterns::CustomPatternSummary {
            pattern: pattern.clone(),
            count: 1,
        })
        .collect();
    run_course_pattern_phase(
        "linear-find-sort",
        iterations,
        &chart,
        rssp::profile::merge_course_patterns_legacy,
    );
    run_course_pattern_phase(
        "binary-insert",
        iterations,
        &chart,
        rssp::profile::merge_course_patterns,
    );
}

fn run_course_pattern_phase(
    phase: &str,
    iterations: usize,
    chart: &[rssp::patterns::CustomPatternSummary],
    merge: fn(
        &mut Vec<rssp::patterns::CustomPatternSummary>,
        &[rssp::patterns::CustomPatternSummary],
    ),
) {
    const CHARTS: usize = 64;
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let mut total = Vec::new();
        for _ in 0..CHARTS {
            merge(black_box(&mut total), black_box(chart));
        }
        checksum = checksum.wrapping_add(total.iter().map(|pattern| pattern.count as usize).sum());
        black_box(total);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-patterns phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_patterns_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        chart.len() as f64 * CHARTS as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_stream_outputs_alloc_phase(
    phase: &str,
    iterations: usize,
    measures: &[usize],
    base_live_bytes: usize,
    mut compute: impl FnMut(
        &[usize],
    ) -> (
        rssp::stats::StreamCounts,
        (String, String, String),
        (String, String, String),
    ),
) {
    black_box(compute(measures));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let output = compute(black_box(measures));
        checksum = checksum.wrapping_add(output.0.run16_streams as usize);
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=stream-outputs phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_measures_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} retained_bytes={} peak_working_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        measures.len() as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        before.live_bytes.saturating_sub(base_live_bytes),
        after.peak_live_bytes.saturating_sub(base_live_bytes),
    );
}

fn run_stream_outputs_alloc(iterations: usize) {
    let measures: Vec<_> = (0..16_384)
        .map(|index| match index % 23 {
            0..=7 => 16,
            8..=11 => 20,
            12..=14 => 24,
            15..=16 => 32,
            _ => 0,
        })
        .collect();
    let base_live_bytes = LIVE_BYTES.load(Ordering::Relaxed);
    run_stream_outputs_alloc_phase(
        "allocating",
        iterations,
        &measures,
        base_live_bytes,
        |measures| rssp::stats::compute_stream_outputs(measures),
    );
    let mut tokens = Vec::new();
    run_stream_outputs_alloc_phase(
        "reused",
        iterations,
        &measures,
        base_live_bytes,
        |measures| rssp::stats::compute_stream_outputs_with_scratch(measures, &mut tokens),
    );
}

fn run_analysis_alloc_phase(
    phase: &str,
    iterations: usize,
    corpus: &[SimInput],
    options: &rssp::AnalysisOptions,
    base_live_bytes: usize,
    mut analyze: impl FnMut(&SimInput) -> rssp::report::SimfileSummary,
) {
    for sim in corpus {
        black_box(analyze(sim));
    }
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for sim in corpus {
            let summary = analyze(black_box(sim));
            checksum = checksum.wrapping_add(summary.charts.len());
            black_box(summary);
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let bytes = corpus.iter().map(|sim| sim.raw.len()).sum::<usize>() as f64 * divisor;
    println!(
        concat!(
            "mode=analysis-reuse phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} retained_bytes={} peak_working_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        bytes / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        before.live_bytes.saturating_sub(base_live_bytes),
        after.peak_live_bytes.saturating_sub(base_live_bytes),
    );
    black_box(options);
}

fn run_analysis_reuse_alloc(
    iterations: usize,
    corpus: &[SimInput],
    options: &rssp::AnalysisOptions,
) {
    for sim in corpus {
        black_box(
            rssp::analyze(sim.raw.as_slice(), sim.extension, options)
                .expect("fixture should analyze"),
        );
    }
    if let Some(sim) = corpus.first() {
        let (mut expected_scratch, mut actual_scratch) = (
            rssp::AnalysisScratch::default(),
            rssp::AnalysisScratch::default(),
        );
        let expected = rssp::profile::analyze_owned_timing(
            &sim.raw,
            sim.extension,
            options,
            &mut expected_scratch,
        )
        .expect("owned timing analysis should succeed");
        let actual =
            rssp::analyze_with_scratch(&sim.raw, sim.extension, options, &mut actual_scratch)
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
    let base_live_bytes = LIVE_BYTES.load(Ordering::Relaxed);
    run_analysis_alloc_phase(
        "fresh",
        iterations,
        corpus,
        options,
        base_live_bytes,
        |sim| {
            rssp::analyze(sim.raw.as_slice(), sim.extension, options)
                .expect("fixture should analyze")
        },
    );
    let mut allocating_scratch = rssp::AnalysisScratch::default();
    run_analysis_alloc_phase(
        "reused-bpm-allocating",
        iterations,
        corpus,
        options,
        base_live_bytes,
        |sim| {
            rssp::profile::analyze_with_allocating_bpms(
                sim.raw.as_slice(),
                sim.extension,
                options,
                &mut allocating_scratch,
            )
            .expect("fixture should analyze")
        },
    );
    drop(allocating_scratch);
    let mut owned_timing_scratch = rssp::AnalysisScratch::default();
    run_analysis_alloc_phase(
        "reused-bpm-owned-timing",
        iterations,
        corpus,
        options,
        base_live_bytes,
        |sim| {
            rssp::profile::analyze_owned_timing(
                sim.raw.as_slice(),
                sim.extension,
                options,
                &mut owned_timing_scratch,
            )
            .expect("fixture should analyze")
        },
    );
    drop(owned_timing_scratch);
    let mut scratch = rssp::AnalysisScratch::default();
    run_analysis_alloc_phase(
        "reused-bpm-buffers",
        iterations,
        corpus,
        options,
        base_live_bytes,
        |sim| {
            rssp::analyze_with_scratch(sim.raw.as_slice(), sim.extension, options, &mut scratch)
                .expect("fixture should analyze")
        },
    );
}

fn run_bpm_stats_alloc_phase(
    phase: &str,
    iterations: usize,
    map: &[(f64, f64)],
    mut compute: impl FnMut(&[(f64, f64)]) -> (i32, i32, f64, f64),
) {
    black_box(compute(map));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        let result = compute(black_box(map));
        checksum = checksum
            .wrapping_add(result.0 as u64)
            .wrapping_add(result.1 as u64)
            .wrapping_add(result.2.to_bits())
            .wrapping_add(result.3.to_bits());
        black_box(result);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=bpm-stats phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        map.len() as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_bpm_stats_alloc(iterations: usize) {
    let map = bpm_summary_bench::fixture();
    bpm_summary_bench::assert_behavior(&map);
    let mut legacy_values = Vec::with_capacity(map.len());
    run_bpm_stats_alloc_phase("sum-after-fill", iterations, &map, |map| {
        bpm_summary_bench::compute(map, &mut legacy_values, true)
    });
    let mut fused_values = Vec::with_capacity(map.len());
    run_bpm_stats_alloc_phase("sum-while-fill", iterations, &map, |map| {
        bpm_summary_bench::compute(map, &mut fused_values, false)
    });
}

fn run_display_bpm_alloc(iterations: usize) {
    const CASES: [(Option<&str>, f64, f64, f64); 4] = [
        (None, 120.0, 180.0, 1.0),
        (Some("150"), 120.0, 180.0, 1.0),
        (Some("120:180"), 120.0, 180.0, 1.25),
        (Some("*"), 90.0, 240.0, 1.1),
    ];
    for (tag, min, max, rate) in CASES {
        black_box(rssp::bpm::resolve_display_bpm(tag, min, max, rate));
    }

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for (tag, min, max, rate) in CASES {
            let result = rssp::bpm::resolve_display_bpm(
                black_box(tag),
                black_box(min),
                black_box(max),
                black_box(rate),
            );
            checksum = checksum.wrapping_add(result.2.len());
            black_box(result);
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=display-bpm iters={} checksum={} elapsed_s={:.6} ",
            "throughput_values_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        CASES.len() as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_bpm_display_tags_phase(data: &[u8], phase: &str, iterations: usize, legacy: bool) {
    black_box(bpm_display_bench::compute(data, legacy));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let snapshots = bpm_display_bench::compute(black_box(data), legacy);
        checksum = checksum
            .wrapping_add(snapshots.len())
            .wrapping_add(snapshots[0].display_bpm.len())
            .wrapping_add(snapshots[snapshots.len() - 1].display_bpm.len());
        black_box(snapshots);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=bpm-display-tags phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_charts_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        bpm_display_bench::CHART_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_bpm_display_tags_alloc(iterations: usize) {
    let fixture = bpm_display_bench::fixture();
    bpm_display_bench::assert_behavior(&fixture);
    run_bpm_display_tags_phase(&fixture, "owned-temporary", iterations, true);
    run_bpm_display_tags_phase(&fixture, "borrowed-tag", iterations, false);
}

fn run_parse_dispatch_phase(data: &[u8], phase: &str, iterations: usize, legacy: bool) {
    black_box(parse_dispatch_bench::parse(data, "ssc", legacy));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let parsed = parse_dispatch_bench::parse(black_box(data), "ssc", legacy);
        checksum = checksum
            .wrapping_add(parsed.notes_list.len())
            .wrapping_add(parsed.attacks.as_deref().map_or(0, <[u8]>::len));
        black_box(parsed);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=parse-dispatch phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        data.len() as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_parse_dispatch_alloc(iterations: usize) {
    let fixture = parse_dispatch_bench::fixture();
    parse_dispatch_bench::assert_behavior(&fixture);
    run_parse_dispatch_phase(&fixture, "sequential-tags", iterations, true);
    run_parse_dispatch_phase(&fixture, "indexed-tags", iterations, false);
}

fn run_parse_append_phase(data: &[u8], phase: &str, iterations: usize, legacy: bool) {
    black_box(parse_dispatch_bench::parse_append(data, "ssc", legacy));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let parsed = parse_dispatch_bench::parse_append(black_box(data), "ssc", legacy);
        checksum = checksum
            .wrapping_add(parsed.notes_list.len())
            .wrapping_add(parsed.attacks.as_deref().map_or(0, <[u8]>::len));
        black_box(parsed);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=parse-append phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        data.len() as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_parse_append_alloc(iterations: usize) {
    let fixture = parse_dispatch_bench::fixture();
    parse_dispatch_bench::assert_append_behavior(&fixture, "ssc");
    run_parse_append_phase(&fixture, "allocate-then-grow", iterations, true);
    run_parse_append_phase(&fixture, "presized-copy", iterations, false);
}

fn run_parse_reserve_phase(
    data: &[u8],
    ext: &str,
    chart_count: usize,
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    black_box(parse_dispatch_bench::parse_reserved(data, ext, legacy));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let parsed = parse_dispatch_bench::parse_reserved(black_box(data), ext, legacy);
        checksum = checksum.wrapping_add(parsed.notes_list.len());
        black_box(parsed);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=parse-reserve ext={} charts={} phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        ext,
        chart_count,
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        data.len() as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_parse_reserve_alloc(iterations: usize) {
    parse_dispatch_bench::assert_reserve_behavior();
    let typical =
        parse_dispatch_bench::fixture_with_charts(parse_dispatch_bench::TYPICAL_CHART_COUNT);
    let large = parse_dispatch_bench::fixture();
    let sm = parse_dispatch_bench::sm_fixture(parse_dispatch_bench::TYPICAL_CHART_COUNT);
    for (data, ext, chart_count) in [
        (
            typical.as_slice(),
            "ssc",
            parse_dispatch_bench::TYPICAL_CHART_COUNT,
        ),
        (large.as_slice(), "ssc", parse_dispatch_bench::CHART_COUNT),
        (
            sm.as_slice(),
            "sm",
            parse_dispatch_bench::TYPICAL_CHART_COUNT,
        ),
    ] {
        run_parse_reserve_phase(data, ext, chart_count, "growing-vec", iterations, true);
        run_parse_reserve_phase(data, ext, chart_count, "presized-vec", iterations, false);
    }
}

fn run_tech_prefix_startup(phase: &str, legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let notation = tech_prefix_bench::parse(black_box("unknown"), black_box(""), legacy);
    black_box(notation);
    let after = Counters::read();
    println!(
        concat!(
            "mode=tech-prefix stage=startup phase={} alloc_calls={} dealloc_calls={} ",
            "realloc_calls={} alloc_bytes={} realloc_bytes={} live_growth_bytes={} ",
            "peak_live_growth_bytes={}"
        ),
        phase,
        after.alloc_calls - before.alloc_calls,
        after.dealloc_calls - before.dealloc_calls,
        after.realloc_calls - before.realloc_calls,
        after.alloc_bytes - before.alloc_bytes,
        after.realloc_bytes - before.realloc_bytes,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_tech_prefix_phase(
    credit: &str,
    description: &str,
    phase: &str,
    iterations: usize,
    mode: u8,
) {
    let parse = |credit, description| match mode {
        0 => tech_prefix_bench::parse(credit, description, true),
        1 => tech_prefix_bench::parse_unicode(credit, description),
        _ => tech_prefix_bench::parse(credit, description, false),
    };
    black_box(parse(credit, description));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let notation = parse(black_box(credit), black_box(description));
        checksum = checksum.wrapping_add(notation.len());
        black_box(notation);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let bytes = credit.len() + description.len();
    println!(
        concat!(
            "mode=tech-prefix stage=steady phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        bytes as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_tech_prefix_alloc(iterations: usize) {
    run_tech_prefix_startup("const-index", false);
    run_tech_prefix_startup("runtime-index", true);
    tech_prefix_bench::assert_behavior();
    let (credit, description) = tech_prefix_bench::valid_input();
    run_tech_prefix_phase(&credit, &description, "runtime-index", iterations, 0);
    run_tech_prefix_phase(&credit, &description, "const-index-unicode", iterations, 1);
    run_tech_prefix_phase(&credit, &description, "const-index-ascii", iterations, 2);
}

fn run_clean_map_alloc(iterations: usize) {
    use std::fmt::Write;

    const ENTRIES: usize = 512;
    let mut raw = String::with_capacity(ENTRIES * 20);
    let mut expected = String::with_capacity(ENTRIES * 16);
    for idx in 0..ENTRIES {
        if idx != 0 {
            raw.push(',');
            expected.push(',');
        }
        write!(&mut raw, "\u{000b}{}={}\u{000b}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
        write!(&mut expected, "{}={}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
    }
    assert_eq!(rssp::bpm::clean_timing_map(&raw), expected);

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let cleaned = rssp::bpm::clean_timing_map(black_box(&raw));
        checksum = checksum.wrapping_add(cleaned.len());
        black_box(cleaned);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=clean-map iters={} checksum={} elapsed_s={:.6} throughput_mib_s={:.3} ",
            "alloc_calls_per_iter={:.1} dealloc_calls_per_iter={:.1} ",
            "realloc_calls_per_iter={:.1} alloc_bytes_per_iter={:.1} ",
            "realloc_bytes_per_iter={:.1} live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        raw.len() as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );

    let maps = timing_borrow_bench::TimingMaps::new();
    timing_borrow_bench::assert_behavior(&maps);
    run_timing_borrow_phase("owned-cleaned-maps", iterations, &maps, |maps| maps.owned());
    run_timing_borrow_phase("borrowed-clean-maps", iterations, &maps, |maps| {
        maps.borrowed()
    });
}

fn run_timing_borrow_phase(
    phase: &str,
    iterations: usize,
    maps: &timing_borrow_bench::TimingMaps,
    process: impl Fn(&timing_borrow_bench::TimingMaps) -> usize,
) {
    black_box(process(maps));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(process(black_box(maps)));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=timing-borrow phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        maps.bytes() as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_normalize_map_alloc(iterations: usize) {
    use std::fmt::Write;

    const ENTRIES: usize = 512;
    let mut raw = String::with_capacity(ENTRIES * 20);
    let mut expected = String::with_capacity(ENTRIES * 24);
    for idx in 0..ENTRIES {
        if idx != 0 {
            raw.push(',');
            expected.push(',');
        }
        write!(&mut raw, "+\u{000b}{}=+\u{000b}{}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
        write!(&mut expected, "{}.000={}.000", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
    }
    assert_eq!(rssp::bpm::normalize_float_digits(&raw), expected);

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let normalized = rssp::bpm::normalize_float_digits(black_box(&raw));
        checksum = checksum.wrapping_add(normalized.len());
        black_box(normalized);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=normalize-map iters={} checksum={} elapsed_s={:.6} throughput_mib_s={:.3} ",
            "alloc_calls_per_iter={:.1} dealloc_calls_per_iter={:.1} ",
            "realloc_calls_per_iter={:.1} alloc_bytes_per_iter={:.1} ",
            "realloc_bytes_per_iter={:.1} live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        raw.len() as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_fused_map_alloc(iterations: usize) {
    use std::fmt::Write;

    const ENTRIES: usize = 512;
    let mut raw = String::with_capacity(ENTRIES * 20);
    let mut cleaned = String::with_capacity(ENTRIES * 16);
    let mut normalized = String::with_capacity(ENTRIES * 24);
    for idx in 0..ENTRIES {
        if idx != 0 {
            raw.push(',');
            cleaned.push(',');
            normalized.push(',');
        }
        write!(&mut raw, "+\u{000b}{}=+\u{000b}{}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
        write!(&mut cleaned, "+{}=+{}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
        write!(&mut normalized, "{}.000={}.000", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
    }
    assert_eq!(
        rssp::bpm::clean_and_normalize_float_digits(&raw),
        (cleaned, normalized)
    );

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let (cleaned, normalized) = rssp::bpm::clean_and_normalize_float_digits(black_box(&raw));
        checksum = checksum.wrapping_add(cleaned.len() ^ normalized.len());
        black_box((cleaned, normalized));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=fused-map iters={} checksum={} elapsed_s={:.6} throughput_mib_s={:.3} ",
            "alloc_calls_per_iter={:.1} dealloc_calls_per_iter={:.1} ",
            "realloc_calls_per_iter={:.1} alloc_bytes_per_iter={:.1} ",
            "realloc_bytes_per_iter={:.1} live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        raw.len() as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_chart_alloc_phase<T>(
    mode: &str,
    phase: &str,
    iterations: usize,
    corpus: &[SimInput],
    compute: impl Fn(&SimInput) -> Vec<T>,
) {
    for sim in corpus {
        black_box(compute(sim));
    }
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for sim in corpus {
            let charts = compute(black_box(sim));
            checksum = checksum.wrapping_add(charts.len());
            black_box(charts);
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let bytes = corpus.iter().map(|sim| sim.raw.len()).sum::<usize>() as f64 * divisor;
    println!(
        concat!(
            "mode={} phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        mode,
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        bytes / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_nps_alloc(iterations: usize, corpus: &[SimInput]) {
    run_chart_alloc_phase("nps", "materialized", iterations, corpus, |sim| {
        rssp::nps::compute_chart_peak_nps_legacy_for_bench(&sim.raw, sim.extension)
            .expect("fixture NPS should compute")
    });
    run_chart_alloc_phase("nps", "reused-owned-timing", iterations, corpus, |sim| {
        rssp::nps::chart_peak_nps_owned(&sim.raw, sim.extension)
            .expect("fixture NPS should compute")
    });
    run_chart_alloc_phase("nps", "reused-borrowed-timing", iterations, corpus, |sim| {
        rssp::compute_chart_peak_nps(&sim.raw, sim.extension).expect("fixture NPS should compute")
    });
}

fn run_duration_alloc(iterations: usize, corpus: &[SimInput]) {
    run_chart_alloc_phase("durations", "owned-timing", iterations, corpus, |sim| {
        rssp::duration::chart_durations_owned(
            &sim.raw,
            sim.extension,
            rssp::TimingOffsets::default(),
        )
        .expect("fixture durations should compute")
    });
    run_chart_alloc_phase("durations", "borrowed-timing", iterations, corpus, |sim| {
        rssp::compute_chart_durations(&sim.raw, sim.extension, rssp::TimingOffsets::default())
            .expect("fixture durations should compute")
    });
}

fn minimize_inputs(corpus: &[SimInput]) -> Vec<MinimizeInput> {
    let mut inputs = Vec::new();
    for sim in corpus {
        let parsed = rssp::parse::extract_sections(&sim.raw, sim.extension)
            .expect("fixture should parse into chart inputs");
        inputs.extend(parsed.notes_list.into_iter().filter_map(|entry| {
            Some(MinimizeInput {
                lanes: rssp::supported_stepstype_lanes_bytes(entry.fields[0])?,
                raw: entry.note_data.to_vec(),
            })
        }));
    }
    inputs
}

fn run_minimize_phase(
    phase: &str,
    iterations: usize,
    inputs: &[MinimizeInput],
    mut compute: impl FnMut(
        &[u8],
        usize,
    ) -> (Vec<u8>, rssp::stats::ArrowStats, Vec<usize>, Vec<f32>, f64),
) {
    for input in inputs {
        black_box(compute(&input.raw, input.lanes));
    }
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for input in inputs {
            let (chart, stats, densities, beats, last) =
                compute(black_box(&input.raw), black_box(input.lanes));
            checksum = checksum
                .wrapping_add(chart.len())
                .wrapping_add(stats.total_arrows as usize)
                .wrapping_add(densities.len())
                .wrapping_add(beats.len())
                .wrapping_add(last.to_bits() as usize);
            black_box((chart, stats, densities, beats, last));
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let bytes = inputs.iter().map(|input| input.raw.len()).sum::<usize>() as f64 * divisor;
    println!(
        concat!(
            "mode=minimize phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        bytes / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_minimize_alloc(iterations: usize, corpus: &[SimInput]) {
    let inputs = minimize_inputs(corpus);
    run_minimize_phase("materialized", iterations, &inputs, |data, lanes| {
        rssp::stats::minimize_chart_count_rows_legacy_for_bench(data, lanes)
    });
    run_minimize_phase("output_backed", iterations, &inputs, |data, lanes| {
        rssp::stats::minimize_chart_count_rows(data, lanes)
    });
    run_typed_rows_alloc(iterations, &inputs);
    run_spacing_count_alloc(iterations);
    run_invalid_notes_alloc(iterations);
    run_phantom_hold_ends_alloc(iterations);
}

fn invalid_notes_checksum(
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

fn spacing_count_rows() -> Vec<u8> {
    let mut raw = Vec::with_capacity(16_384 * 27);
    for measure in 0usize..16_384 {
        raw.extend_from_slice(b"1000\n0100\n0010\n0001\n");
        raw.extend_from_slice(if measure + 1 == 16_384 { b";" } else { b",\n" });
    }
    raw
}

fn spacing_count_checksum(data: &[u8], legacy_count: bool) -> usize {
    let values = rssp::nps::measure_equally_spaced_for_bench(data, 4, legacy_count);
    let checksum = values.len() + values.iter().filter(|value| **value).count();
    black_box(values);
    checksum
}

fn run_spacing_count_alloc(iterations: usize) {
    let input = MinimizeInput {
        lanes: 4,
        raw: spacing_count_rows(),
    };
    assert_eq!(
        spacing_count_checksum(&input.raw, true),
        spacing_count_checksum(&input.raw, false),
    );
    let base_live_bytes = LIVE_BYTES.load(Ordering::Relaxed);
    for (phase, legacy_count) in [
        ("equally-spaced-scalar-count", true),
        ("equally-spaced-chunked-count", false),
    ] {
        run_typed_rows_phase(phase, iterations, &input, base_live_bytes, |data| {
            spacing_count_checksum(data, legacy_count)
        });
    }
}

fn run_invalid_notes_alloc(iterations: usize) {
    let input = MinimizeInput {
        lanes: 4,
        raw: phantom_hold_rows(),
    };
    let base_live_bytes = LIVE_BYTES.load(Ordering::Relaxed);

    for (phase, legacy) in [
        ("invalid-notes-index-cold", true),
        ("invalid-notes-mark-cold", false),
    ] {
        run_typed_rows_phase(phase, iterations, &input, base_live_bytes, |data| {
            let mut scratch = rssp::stats::ChartNotesScratch::default();
            invalid_notes_checksum(data, legacy, &mut scratch)
        });
    }

    let mut legacy = rssp::stats::ChartNotesScratch::default();
    run_typed_rows_phase(
        "invalid-notes-index-reused",
        iterations,
        &input,
        base_live_bytes,
        |data| invalid_notes_checksum(data, true, &mut legacy),
    );
    drop(legacy);
    let mut marked = rssp::stats::ChartNotesScratch::default();
    run_typed_rows_phase(
        "invalid-notes-mark-reused",
        iterations,
        &input,
        base_live_bytes,
        |data| invalid_notes_checksum(data, false, &mut marked),
    );
}

fn phantom_hold_rows() -> Vec<u8> {
    let mut raw = Vec::with_capacity(4096 * 5 + 1);
    for row in 0usize..4096 {
        raw.extend_from_slice(if row.is_multiple_of(2) {
            b"2000\n"
        } else {
            b"1000\n"
        });
    }
    raw.push(b';');
    raw
}

fn phantom_hold_ends_checksum(data: &[u8], legacy_options: bool) -> usize {
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

fn run_phantom_hold_ends_alloc(iterations: usize) {
    let input = MinimizeInput {
        lanes: 4,
        raw: phantom_hold_rows(),
    };
    assert_eq!(
        phantom_hold_ends_checksum(&input.raw, true),
        phantom_hold_ends_checksum(&input.raw, false),
    );
    let base_live_bytes = LIVE_BYTES.load(Ordering::Relaxed);
    for (phase, legacy_options) in [
        ("phantom-hold-option-table", true),
        ("phantom-hold-sentinel-table", false),
    ] {
        run_typed_rows_phase(phase, iterations, &input, base_live_bytes, |data| {
            phantom_hold_ends_checksum(data, legacy_options)
        });
    }
}

fn run_typed_rows_phase(
    phase: &str,
    iterations: usize,
    input: &MinimizeInput,
    base_live_bytes: usize,
    mut minimize: impl FnMut(&[u8]) -> usize,
) {
    black_box(minimize(&input.raw));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(minimize(black_box(&input.raw)));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let bytes = input.raw.len() as f64 * divisor;
    println!(
        concat!(
            "mode=minimize phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} retained_bytes={} peak_working_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        bytes / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        before.live_bytes.saturating_sub(base_live_bytes),
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_typed_rows_alloc(iterations: usize, inputs: &[MinimizeInput]) {
    let input = inputs
        .iter()
        .filter(|input| input.lanes == 4)
        .max_by_key(|input| input.raw.len())
        .expect("minimize corpus should contain a 4-lane chart");
    let base_live_bytes = LIVE_BYTES.load(Ordering::Relaxed);
    run_typed_rows_phase(
        "typed-rows-owned",
        iterations,
        input,
        base_live_bytes,
        |data| {
            let (chart, stats, densities, rows, beats, last) =
                rssp::stats::minimize_rows_typed::<4>(data);
            let checksum = chart
                .len()
                .wrapping_add(stats.total_arrows as usize)
                .wrapping_add(densities.len())
                .wrapping_add(rows.len())
                .wrapping_add(beats.len())
                .wrapping_add(last.to_bits() as usize);
            black_box((chart, stats, densities, rows, beats, last));
            checksum
        },
    );

    let mut rows = rssp::stats::TypedRowsScratch::<4>::default();
    run_typed_rows_phase(
        "typed-rows-reused",
        iterations,
        input,
        base_live_bytes,
        |data| {
            let (chart, stats, densities, beats, last) =
                rssp::stats::minimize_rows_typed_in::<4>(data, &mut rows);
            let checksum = chart
                .len()
                .wrapping_add(stats.total_arrows as usize)
                .wrapping_add(densities.len())
                .wrapping_add(rows.rows().len())
                .wrapping_add(beats.len())
                .wrapping_add(last.to_bits() as usize);
            black_box((chart, stats, densities, beats, last));
            black_box(rows.rows());
            checksum
        },
    );
}

fn run_course_analyze_phase(
    phase: &str,
    iterations: usize,
    fixture: &course_bench::CourseFixture,
    options: &rssp::AnalysisOptions,
    analyze: impl Fn(&course_bench::CourseFixture, rssp::AnalysisOptions) -> rssp::CourseSummary,
) {
    black_box(analyze(fixture, options.clone()));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let summary = analyze(black_box(fixture), black_box(options.clone()));
        checksum = checksum.wrapping_add(summary.entries.len());
        black_box(summary);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-analyze phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
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

fn run_course_parse_phase(input: &[u8], phase: &str, iterations: usize, mode: u8) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let parsed = match mode {
            0 => rssp::course::profile_parse_crs(black_box(input), true),
            1 => rssp::course::profile_parse_crs(black_box(input), false),
            2 => rssp::course::profile_parse_crs_dispatch(black_box(input), true),
            _ => rssp::course::profile_parse_crs_dispatch(black_box(input), false),
        }
        .expect("course should parse");
        checksum = checksum.wrapping_add(parsed.entries.len());
        black_box(parsed);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-parse phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
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

fn run_course_parse_alloc(iterations: usize) {
    let fixture = course_bench::CourseFixture::new();
    let input = std::fs::read(fixture.course_path()).expect("benchmark course should be readable");
    let current =
        rssp::course::profile_parse_crs(&input, false).expect("benchmark course should parse");
    let legacy = rssp::course::profile_parse_crs(&input, true)
        .expect("legacy benchmark course should parse");
    let sequential = rssp::course::profile_parse_crs_dispatch(&input, true)
        .expect("sequential dispatch course should parse");
    assert_eq!(current.entries, legacy.entries);
    assert_eq!(current.repeat, legacy.repeat);
    assert_eq!(current.meters, legacy.meters);
    course_bench::assert_same_course(&current, &sequential);
    assert_eq!(current.entries.len(), course_bench::SONG_COUNT);
    assert!(current.repeat);
    assert_eq!(
        current.meters,
        [Some(3), Some(6), Some(9), Some(12), Some(15), Some(18)]
    );

    run_course_parse_phase(&input, "legacy-control-allocs", iterations, 0);
    run_course_parse_phase(&input, "stream-control-fields", iterations, 1);
    run_course_parse_phase(&input, "sequential-tag-dispatch", iterations, 2);
    run_course_parse_phase(&input, "indexed-tag-dispatch", iterations, 3);
}

fn run_course_entry_reserve_phase(
    input: &[u8],
    entry_count: usize,
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    black_box(course_bench::parse_reserved(input, legacy));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let parsed = course_bench::parse_reserved(black_box(input), legacy);
        checksum = checksum.wrapping_add(parsed.entries.len());
        black_box(parsed);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-entry-reserve entries={} phase={} iters={} checksum={} ",
            "elapsed_s={:.6} throughput_entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        entry_count,
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        entry_count as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_course_entry_reserve_alloc(iterations: usize) {
    course_bench::assert_parse_reserve_behavior();
    let typical = course_bench::parse_input(course_bench::PARSE_TYPICAL_COUNT);
    let large = course_bench::parse_input(course_bench::PARSE_LARGE_COUNT);
    for (input, entry_count) in [
        (typical.as_slice(), course_bench::PARSE_TYPICAL_COUNT),
        (large.as_slice(), course_bench::PARSE_LARGE_COUNT),
    ] {
        run_course_entry_reserve_phase(input, entry_count, "growing-vec", iterations, true);
        run_course_entry_reserve_phase(input, entry_count, "presized-vec", iterations, false);
    }
}

fn run_course_mods_alloc(iterations: usize) {
    assert_eq!(
        rssp::course::profile_song_mods(true, course_bench::MODS),
        (
            false,
            true,
            2,
            "1.5x,reverse,mirror,noholds,nomines,sudden".to_string(),
        )
    );

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let (secret, no_difficult, gain_lives, modifiers) =
            rssp::course::profile_song_mods(black_box(true), black_box(course_bench::MODS));
        checksum = checksum
            .wrapping_add(usize::from(secret))
            .wrapping_add(usize::from(no_difficult))
            .wrapping_add(gain_lives as usize)
            .wrapping_add(modifiers.len());
        black_box(modifiers);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-mods iters={} checksum={} elapsed_s={:.6} ",
            "throughput_modifiers_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        course_bench::MOD_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_select_mods_alloc(iterations: usize) {
    assert_eq!(
        rssp::course::profile_select_mods(course_bench::SELECT_MODS),
        (
            true,
            true,
            "1.5x,reverse,mirror,noholds,nomines,sudden".to_string(),
        )
    );

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let (secret, no_difficult, modifiers) =
            rssp::course::profile_select_mods(black_box(course_bench::SELECT_MODS));
        checksum = checksum
            .wrapping_add(usize::from(secret))
            .wrapping_add(usize::from(no_difficult))
            .wrapping_add(modifiers.len());
        black_box(modifiers);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-select-mods iters={} checksum={} elapsed_s={:.6} ",
            "throughput_modifiers_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        course_bench::SELECT_MOD_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_select_parse_alloc(iterations: usize) {
    let input = course_bench::select_input();
    let parsed = rssp::course::parse_crs(&input).expect("selection benchmark should parse");
    assert_eq!(parsed.entries.len(), course_bench::SELECT_COUNT);

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let parsed =
            rssp::course::parse_crs(black_box(&input)).expect("selection benchmark should parse");
        checksum = checksum.wrapping_add(parsed.entries.len());
        black_box(parsed);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-select-parse iters={} checksum={} elapsed_s={:.6} ",
            "throughput_params_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        course_bench::SELECT_COUNT as f64 * course_bench::SELECT_PARAMS as f64 * divisor
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

fn run_course_analyze_alloc(iterations: usize) {
    let fixture = course_bench::CourseFixture::new();
    let repeated = course_bench::CourseFixture::repeated();
    let options = course_bench::clone_heavy_options();
    fixture.assert_group_cache();
    fixture.assert_group_catalog();
    fixture.assert_catalog_dirs();
    repeated.assert_song_cache();
    run_course_analyze_phase(
        "cache-all",
        iterations,
        &fixture,
        &options,
        |fixture, options| {
            rssp::course::analyze_crs_path_cache_all_for_bench(
                fixture.course_path(),
                Some(fixture.songs_dir()),
                "dance-single",
                "Medium",
                options,
            )
            .expect("benchmark course should analyze")
        },
    );
    run_course_analyze_phase(
        "cache-repeated",
        iterations,
        &fixture,
        &options,
        |fixture, options| {
            rssp::course::analyze_crs_path(
                fixture.course_path(),
                Some(fixture.songs_dir()),
                "dance-single",
                "Medium",
                options,
            )
            .expect("benchmark course should analyze")
        },
    );
    for (phase, trust_catalog) in [
        ("catalog-dir-recheck", false),
        ("catalog-dir-trusted", true),
    ] {
        run_course_analyze_phase(phase, iterations, &fixture, &options, |fixture, options| {
            rssp::course::profile_catalog_dirs(
                fixture.course_path(),
                Some(fixture.songs_dir()),
                "dance-single",
                "Medium",
                options,
                trust_catalog,
            )
            .expect("catalog directory benchmark course should analyze")
        });
    }
    for (phase, group_cache) in [("group-cache-off", false), ("group-cache-on", true)] {
        run_course_analyze_phase(phase, iterations, &fixture, &options, |fixture, options| {
            rssp::course::profile_analyze_groups(
                fixture.course_path(),
                Some(fixture.songs_dir()),
                "dance-single",
                "Medium",
                options,
                group_cache,
            )
            .expect("group cache benchmark course should analyze")
        });
    }
    for (phase, group_catalog) in [("group-catalog-off", false), ("group-catalog-on", true)] {
        run_course_analyze_phase(phase, iterations, &fixture, &options, |fixture, options| {
            rssp::course::profile_analyze_catalog(
                fixture.course_path(),
                Some(fixture.songs_dir()),
                "dance-single",
                "Medium",
                options,
                group_catalog,
            )
            .expect("group catalog benchmark course should analyze")
        });
    }
    for (phase, song_key_cache) in [("repeat-path-key", false), ("repeat-song-key", true)] {
        run_course_analyze_phase(
            phase,
            iterations,
            &repeated,
            &options,
            |fixture, options| {
                rssp::course::profile_analyze_crs(
                    fixture.course_path(),
                    Some(fixture.songs_dir()),
                    "dance-single",
                    "Medium",
                    options,
                    song_key_cache,
                )
                .expect("repeated benchmark course should analyze")
            },
        );
    }
}

fn run_course_hash_dedup_phase(values: &[String], phase: &str, iterations: usize, std_hash: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let output = rssp::course::profile_dedup_hashes(black_box(values), std_hash);
        checksum = checksum
            .wrapping_add(output.len())
            .wrapping_add(output.last().map_or(0, String::len));
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-hash-dedup phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_hashes_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        values.len() as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_course_hash_dedup_alloc(iterations: usize) {
    let values = course_bench::hash_values();
    course_bench::assert_hash_dedup_behavior(&values);
    run_course_hash_dedup_phase(&values, "std-sip-hash-4096", iterations, true);
    run_course_hash_dedup_phase(&values, "fold-hash-4096", iterations, false);
    run_course_hash_reserve_phase(
        &values,
        "fold-hash-growing-4096",
        iterations,
        HashDedupMode::Growing,
    );
    run_course_hash_reserve_phase(
        &values,
        "fold-hash-bounded-8-4096",
        iterations,
        HashDedupMode::Bounded,
    );

    let values = course_bench::course_hash_values();
    course_bench::assert_hash_dedup_behavior(&values);
    run_course_hash_dedup_phase(&values, "std-sip-hash-64", iterations, true);
    run_course_hash_dedup_phase(&values, "fold-hash-64", iterations, false);
    run_course_hash_reserve_phase(
        &values,
        "fold-hash-growing-64",
        iterations,
        HashDedupMode::Growing,
    );
    run_course_hash_reserve_phase(
        &values,
        "fold-hash-bounded-8-64",
        iterations,
        HashDedupMode::Bounded,
    );

    let values = course_bench::typical_hash_values();
    course_bench::assert_hash_dedup_behavior(&values);
    run_course_hash_reserve_phase(
        &values,
        "fold-hash-bounded-8-typical-64",
        iterations,
        HashDedupMode::Bounded,
    );
    run_course_hash_reserve_phase(
        &values,
        "adaptive-linear-typical-64",
        iterations,
        HashDedupMode::Adaptive,
    );
}

#[derive(Clone, Copy)]
enum HashDedupMode {
    Growing,
    Bounded,
    Adaptive,
}

fn run_course_hash_reserve_phase(
    values: &[String],
    phase: &str,
    iterations: usize,
    mode: HashDedupMode,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let values = black_box(values);
        let output = match mode {
            HashDedupMode::Growing => rssp::course::profile_dedup_hashes(values, false),
            HashDedupMode::Bounded => rssp::course::profile_dedup_hashes_reserved(values),
            HashDedupMode::Adaptive => rssp::course::profile_dedup_hashes_adaptive(values),
        };
        checksum = checksum
            .wrapping_add(output.len())
            .wrapping_add(output.last().map_or(0, String::len));
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-hash-dedup phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_hashes_s={:.3} alloc_calls_per_iter={:.1} dealloc_calls_per_iter={:.1} ",
            "realloc_calls_per_iter={:.1} alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        values.len() as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn timing_build_fixture() -> (String, String, String) {
    let mut bpms = String::with_capacity(512 * 16);
    let mut stops = String::with_capacity(256 * 16);
    let mut speeds = String::with_capacity(512 * 16);
    for index in 0..512 {
        if index != 0 {
            bpms.push(',');
        }
        write!(&mut bpms, "{}={}", index * 4, 120 + index % 180)
            .expect("writing to a String should succeed");
    }
    for index in 0..256 {
        if index != 0 {
            stops.push(',');
        }
        write!(&mut stops, "{}=0.125", index * 8 + 2).expect("writing to a String should succeed");
    }
    for index in 0..512 {
        if index != 0 {
            speeds.push(',');
        }
        write!(
            &mut speeds,
            "{}={}={}={}",
            index * 4,
            1 + index % 7,
            1 + index % 4,
            index & 1
        )
        .expect("writing to a String should succeed");
    }
    (bpms, stops, speeds)
}

fn timing_build_checksum(timing: &rssp::timing::TimingData) -> u64 {
    let mut checksum = rssp::timing::get_time_for_beat(timing, 2_044.0).to_bits();
    for index in 0..64 {
        let beat = index as f64 * 32.0 + 1.5;
        let time = rssp::timing::get_time_for_beat(timing, beat);
        checksum = checksum.rotate_left(7)
            ^ rssp::timing::get_speed_multiplier(timing, beat, time).to_bits();
    }
    checksum
}

fn run_segment_parse_phase(map: &str, phase: &str, iterations: usize, legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        let output = timing_segments_bench::parse(black_box(map), legacy);
        checksum = checksum
            .wrapping_add(output.len() as u64)
            .wrapping_add(output.last().map_or(0, |segment| segment.beat.to_bits()));
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=timing-segments phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} throughput_segments_s={:.3} ",
            "alloc_calls_per_iter={:.1} dealloc_calls_per_iter={:.1} ",
            "realloc_calls_per_iter={:.1} alloc_bytes_per_iter={:.1} ",
            "realloc_bytes_per_iter={:.1} live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        map.len() as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        timing_segments_bench::ENTRY_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_row_to_beat_phase(data: &[u8], phase: &str, iterations: usize, legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        let output = row_to_beat_bench::compute(black_box(data), legacy);
        checksum = checksum
            .wrapping_add(output.len() as u64)
            .wrapping_add(output.last().map_or(0, |beat| u64::from(beat.to_bits())));
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=row-to-beat phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_rows_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        row_to_beat_bench::ROW_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_timing_rows_phase(
    fixture: &timing_rows_bench::TimingRowsFixture,
    phase: &str,
    iterations: usize,
    packed: bool,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let output = rssp::timing::build_segment_rows_for_bench(
            black_box(&fixture.stops),
            black_box(&fixture.delays),
            black_box(&fixture.warps),
            black_box(&fixture.fakes),
            packed,
        );
        checksum = checksum.wrapping_add(timing_rows_bench::row_count(&output));
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=timing-rows phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_segments_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        timing_rows_bench::INPUT_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_timing_build_alloc(iterations: usize) {
    let row_data = row_to_beat_bench::fixture();
    row_to_beat_bench::assert_behavior(&row_data);
    run_row_to_beat_phase(&row_data, "growing", iterations, true);
    run_row_to_beat_phase(&row_data, "preallocated", iterations, false);

    let segment_map = timing_segments_bench::fixture();
    timing_segments_bench::assert_behavior(&segment_map);
    run_segment_parse_phase(&segment_map, "scalar-capacity-scan", iterations, true);
    run_segment_parse_phase(&segment_map, "chunked-capacity-scan", iterations, false);

    let timing_rows = timing_rows_bench::TimingRowsFixture::new();
    timing_rows_bench::assert_behavior(&timing_rows);
    run_timing_rows_phase(&timing_rows, "split", iterations, false);
    run_timing_rows_phase(&timing_rows, "packed", iterations, true);

    let (bpms, stops, speeds) = timing_build_fixture();
    let build = || {
        rssp::timing::timing_data_from_chart_data(
            0.0,
            0.0,
            None,
            black_box(&bpms),
            None,
            black_box(&stops),
            None,
            "",
            None,
            "",
            None,
            black_box(&speeds),
            None,
            "",
            None,
            "",
            rssp::timing::TimingFormat::Ssc,
            true,
        )
    };
    let expected = timing_build_checksum(&build());

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        let timing = build();
        let actual = timing_build_checksum(black_box(&timing));
        assert_eq!(actual, expected);
        checksum = checksum.wrapping_add(actual);
        black_box(timing);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=timing-build iters={} checksum={} elapsed_s={:.6} ",
            "throughput_segments_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        1_280.0 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_timing_sort_phase(
    fixture: &[rssp::timing::Segment],
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    black_box(timing_sort_bench::tidy(fixture.to_vec(), legacy));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        let output = timing_sort_bench::tidy(fixture.to_vec(), legacy);
        checksum = checksum
            .wrapping_add(output.len() as u64)
            .wrapping_add(output.first().map_or(0, |segment| segment.value.to_bits()))
            .wrapping_add(output.last().map_or(0, |segment| segment.value.to_bits()));
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=timing-sort phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_segments_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        timing_sort_bench::ENTRY_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_timing_sort_alloc(iterations: usize) {
    let fixture = timing_sort_bench::fixture();
    timing_sort_bench::assert_behavior(&fixture);
    run_timing_sort_phase(&fixture, "packed-records", iterations, true);
    run_timing_sort_phase(&fixture, "key-indices", iterations, false);
}

fn run_sm_timing_phase(
    fixture: &sm_timing_bench::SmTimingFixture,
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let output = rssp::timing::process_sm_timing_for_bench(
            black_box(&fixture.bpms),
            black_box(&fixture.stops),
            legacy,
        );
        checksum = checksum
            .wrapping_add(output.0.len())
            .wrapping_add(output.1.len())
            .wrapping_add(output.2.len())
            .wrapping_add(output.3.to_bits() as usize);
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=sm-timing phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_segments_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        sm_timing_bench::INPUT_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_sm_timing_alloc(iterations: usize) {
    sm_timing_bench::assert_behavior();
    let fixture = sm_timing_bench::SmTimingFixture::new();
    run_sm_timing_phase(&fixture, "legacy-f32-then-f64", iterations, true);
    run_sm_timing_phase(&fixture, "direct-f64", iterations, false);
    let extra_warps = sm_timing_bench::extra_warps();
    run_warp_merge_phase(&extra_warps, "copy-into-empty", iterations, false);
    run_warp_merge_phase(&extra_warps, "reuse-generated", iterations, true);
    let (bpms, stops) = sm_timing_bench::warp_inputs();
    run_warp_pipeline_phase(&bpms, &stops, "copy-into-empty", iterations, false);
    run_warp_pipeline_phase(&bpms, &stops, "reuse-generated", iterations, true);
}

fn run_warp_merge_phase(
    extra_warps: &[rssp::timing::Segment],
    phase: &str,
    iterations: usize,
    reuse_empty: bool,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let output = rssp::timing::merge_extra_warps_for_bench(
            Vec::new(),
            black_box(extra_warps.to_vec()),
            reuse_empty,
        );
        checksum = checksum
            .wrapping_add(output.len())
            .wrapping_add(output.last().map_or(0, |warp| warp.beat.to_bits() as usize));
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=sm-warp-merge phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_warps_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        sm_timing_bench::WARP_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_warp_pipeline_phase(
    bpms: &[(f64, f64)],
    stops: &[rssp::timing::Segment],
    phase: &str,
    iterations: usize,
    reuse_empty: bool,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let output = rssp::timing::process_sm_warp_merge_for_bench(
            black_box(bpms),
            black_box(stops),
            reuse_empty,
        );
        checksum = checksum
            .wrapping_add(output.0.len())
            .wrapping_add(output.1.len())
            .wrapping_add(output.2.len())
            .wrapping_add(output.3.to_bits() as usize);
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=sm-warp-pipeline phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_warps_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        sm_timing_bench::WARP_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

type TimingMergeFn = for<'a> fn(
    &[(f32, f32)],
    &'a [(f32, f32)],
    &[(f32, f32)],
    &[(f32, f32)],
) -> std::borrow::Cow<'a, [(f32, f32)]>;

fn run_timing_merge_phase(
    fixture: &timing_merge_bench::TimingMergeFixture,
    phase: &str,
    iterations: usize,
    merge: TimingMergeFn,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let output = merge(
            black_box(&fixture.bpms),
            black_box(&fixture.stops),
            black_box(&fixture.delays),
            black_box(&fixture.warps),
        );
        checksum = checksum
            .wrapping_add(output.len())
            .wrapping_add(output.last().map_or(0, |pair| pair.1.to_bits() as usize));
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=timing-merge phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_segments_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        timing_merge_bench::MERGE_INPUT_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_timing_merge_alloc(iterations: usize) {
    timing_merge_bench::assert_behavior();
    let fixture = timing_merge_bench::TimingMergeFixture::new();
    run_timing_merge_phase(
        &fixture,
        "materialize-warps",
        iterations,
        timing_merge_bench::legacy_convert,
    );
    run_timing_merge_phase(
        &fixture,
        "fused-warps",
        iterations,
        rssp::timing::convert_warps_and_delays_to_sm_stops,
    );
}

fn run_timing_text_phase(
    fixture: &report_timing_bench::TimingTextFixture,
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    let parse = || {
        rssp::profile::timing_text(
            &fixture.time_signatures,
            &fixture.labels,
            &fixture.tickcounts,
            &fixture.combos,
            legacy,
        )
    };
    black_box(parse());

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let (time_signatures, labels, tickcounts, combos) = parse();
        checksum = checksum
            .wrapping_add(time_signatures.len())
            .wrapping_add(labels.len())
            .wrapping_add(tickcounts.len())
            .wrapping_add(combos.len());
        black_box((time_signatures, labels, tickcounts, combos));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=timing-text phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_segments_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        (report_timing_bench::SEGMENT_COUNT * 4) as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_timing_text_alloc(iterations: usize) {
    let fixture = report_timing_bench::timing_text();
    let legacy = rssp::profile::timing_text(
        &fixture.time_signatures,
        &fixture.labels,
        &fixture.tickcounts,
        &fixture.combos,
        true,
    );
    let current = rssp::profile::timing_text(
        &fixture.time_signatures,
        &fixture.labels,
        &fixture.tickcounts,
        &fixture.combos,
        false,
    );
    assert_eq!(current, legacy, "timing text behavior must not change");
    let [time_signatures, labels, tickcounts, combos] = report_timing_bench::TIMING_TEXT_EDGE;
    assert_eq!(
        rssp::profile::timing_text(time_signatures, labels, tickcounts, combos, false),
        rssp::profile::timing_text(time_signatures, labels, tickcounts, combos, true),
        "timing text edge behavior must not change"
    );
    run_timing_text_phase(&fixture, "legacy-staged", iterations, true);
    run_timing_text_phase(&fixture, "streamed-presized", iterations, false);
}

fn run_serialize_phase(
    mode: &str,
    summary: &rssp::SimfileSummary,
    output_len: usize,
    phase: &str,
    iterations: usize,
    legacy: bool,
    mut write: impl FnMut(&rssp::SimfileSummary, &mut Vec<u8>, bool) -> usize,
) {
    let mut output = Vec::with_capacity(output_len);
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        output.clear();
        checksum = checksum.wrapping_add(write(
            black_box(summary),
            black_box(&mut output),
            black_box(legacy),
        ));
        black_box(&output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode={} phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        mode,
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        output_len as f64 * divisor / elapsed.as_secs_f64() / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_serialize_alloc(iterations: usize) {
    let fixture = serialize_bench::SerializeFixture::new();
    serialize_bench::assert_behavior(&fixture);
    run_serialize_phase(
        "serialize",
        &fixture.summary,
        fixture.output_len,
        "temporary-strings",
        iterations,
        true,
        serialize_bench::write,
    );
    run_serialize_phase(
        "serialize",
        &fixture.summary,
        fixture.output_len,
        "direct-writer",
        iterations,
        false,
        serialize_bench::write,
    );

    let buffer = serialize_bench::BufferFixture::new();
    serialize_bench::assert_buffer_behavior(&buffer);
    run_serialize_phase(
        "serialize-buffer",
        &buffer.summary,
        buffer.output_len,
        "unbuffered",
        iterations,
        true,
        serialize_bench::write_buffered,
    );
    run_serialize_phase(
        "serialize-buffer",
        &buffer.summary,
        buffer.output_len,
        "stack-buffered",
        iterations,
        false,
        serialize_bench::write_buffered,
    );

    let escape = serialize_bench::EscapeFixture::new();
    serialize_bench::assert_escape_behavior(&escape);
    run_serialize_phase(
        "serialize-escape",
        &escape.summary,
        escape.output_len,
        "byte-at-a-time",
        iterations,
        true,
        serialize_bench::write_escape,
    );
    run_serialize_phase(
        "serialize-escape",
        &escape.summary,
        escape.output_len,
        "batched-spans",
        iterations,
        false,
        serialize_bench::write_escape,
    );
}

fn run_nps_vec_alloc(
    phase: &str,
    measures: usize,
    iterations: usize,
    mut compute: impl FnMut() -> Vec<f64>,
) {
    let checksum = |values: &[f64]| {
        values.iter().fold(0u64, |sum, value| {
            sum.rotate_left(7) ^ value.to_bits().wrapping_mul(0x9e37_79b9_7f4a_7c15)
        })
    };
    let expected = checksum(&compute());
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut combined = 0u64;
    for _ in 0..iterations {
        let values = compute();
        let actual = checksum(&values);
        assert_eq!(actual, expected);
        combined = combined.wrapping_add(actual);
        black_box(values);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=nps-cursor phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_measures_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(combined),
        elapsed.as_secs_f64(),
        measures as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_nps_stats_phase(phase: &str, iterations: usize, mut compute: impl FnMut() -> (f64, f64)) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        let stats = compute();
        checksum = checksum.wrapping_add(stats.0.to_bits() ^ stats.1.to_bits());
        black_box(stats);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=nps-stats phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_values_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        nps_stats_bench::VALUE_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_nps_stats_alloc(iterations: usize) {
    nps_stats_bench::assert_behavior();
    let values = nps_stats_bench::values();
    run_nps_stats_phase("copy-to-scratch", iterations, || {
        rssp::bpm::get_nps_stats(black_box(&values))
    });

    let mut owned = values.clone();
    run_nps_stats_phase("select-in-place", iterations, || {
        owned.copy_from_slice(&values);
        rssp::bpm::get_nps_stats_in_place(black_box(&mut owned))
    });
}

fn run_nps_cursor_alloc(iterations: usize) {
    let (bpms, stops, _) = timing_build_fixture();
    let timing = rssp::timing::timing_data_from_chart_data(
        0.0,
        0.0,
        None,
        &bpms,
        None,
        &stops,
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
    let timing_densities: Vec<_> = (0..512)
        .map(|idx| [0, 16, 20, 24, 32][(idx * 7) % 5])
        .collect();
    run_nps_vec_alloc("timing-events", timing_densities.len(), iterations, || {
        rssp::bpm::compute_measure_nps_vec_with_timing(
            black_box(&timing_densities),
            black_box(&timing),
        )
    });

    let bpm_densities: Vec<_> = (0..4_096)
        .map(|idx| [0, 16, 20, 24, 32][(idx * 7) % 5])
        .collect();
    let bpm_map: Vec<_> = (0..4_096)
        .map(|idx| (idx as f64 * 4.0, 60.0 + ((idx * 37) % 300) as f64))
        .collect();
    run_nps_vec_alloc("bpm-map", bpm_densities.len(), iterations, || {
        rssp::bpm::compute_measure_nps_vec(black_box(&bpm_densities), black_box(&bpm_map))
    });
}

fn run_stepstype_phase(phase: &str, iterations: usize, compare: impl Fn(&str, &str) -> bool) {
    const CASES: [(&str, &str); 8] = [
        ("dance-single", "dance-single"),
        (" DANCE_SINGLE ", "dance-single"),
        ("dance-double", "dance-single"),
        ("DANCE-SOLO", "dance-single"),
        ("pump_single", "pump-single"),
        ("lights-cabinet", "lights-cabinet"),
        ("kb7-single", "dance-single"),
        ("非ASCII-single", "dance-single"),
    ];
    const REPEATS: usize = 512;

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for _ in 0..REPEATS {
            for (raw, normalized) in CASES {
                checksum = checksum
                    .wrapping_add(usize::from(compare(black_box(raw), black_box(normalized))));
            }
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let comparisons = (CASES.len() * REPEATS) as f64 * divisor;
    println!(
        concat!(
            "mode=course-stepstype phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_comparisons_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        comparisons / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_stepstype_alloc(iterations: usize) {
    course_bench::assert_step_norm_behavior();
    run_step_norm_phase("normalize-two-owned-passes", iterations, true);
    run_step_norm_phase("normalize-borrow-or-one-pass", iterations, false);
    run_stepstype_phase("allocating", iterations, |raw, normalized| {
        rssp::course::profile_stepstype_eq_legacy(raw, normalized)
    });
    run_stepstype_phase("bytes", iterations, |raw, normalized| {
        rssp::course::profile_stepstype_eq(raw, normalized)
    });
}

fn run_step_norm_phase(phase: &str, iterations: usize, legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for _ in 0..course_bench::STEP_NORM_BATCH {
            for raw in course_bench::STEP_NORM_CASES {
                let normalized = rssp::course::profile_normalize_stepstype(black_box(raw), legacy);
                checksum = checksum.wrapping_add(normalized.len());
                black_box(normalized);
            }
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let normalizations =
        (course_bench::STEP_NORM_CASES.len() * course_bench::STEP_NORM_BATCH) as f64 * divisor;
    println!(
        concat!(
            "mode=course-stepstype phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_normalizations_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        normalizations / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_title_match_phase(phase: &str, iterations: usize, legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for _ in 0..course_bench::TITLE_MATCH_BATCH {
            let matched = rssp::course::profile_simfile_title_eq(
                black_box(course_bench::TITLE_MATCH_INPUT),
                black_box("ssc"),
                black_box(course_bench::TITLE_MATCH_EXPECTED),
                legacy,
            );
            checksum = checksum.wrapping_add(usize::from(matched == Some(true)));
            black_box(matched);
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let matches = course_bench::TITLE_MATCH_BATCH as f64 * divisor;
    println!(
        concat!(
            "mode=course-title phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_matches_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        matches / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_title_match_alloc(iterations: usize) {
    course_bench::assert_title_match_behavior();
    run_title_match_phase("owned-full-title", iterations, true);
    run_title_match_phase("borrowed-parts", iterations, false);
}

fn run_course_banner_phase<F>(
    fixture: &course_bench::BannerFixture,
    phase: &str,
    iterations: usize,
    resolve: F,
) where
    F: Fn() -> Option<PathBuf>,
{
    assert_eq!(resolve(), Some(fixture.expected_banner()));

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let banner = resolve().expect("benchmark banner should resolve");
        checksum = checksum.wrapping_add(banner.as_os_str().len());
        black_box(banner);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-banner phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        course_bench::BANNER_ENTRY_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_course_banner_alloc(iterations: usize) {
    let fixture = course_bench::BannerFixture::new();
    fixture.assert_behavior();
    run_course_banner_phase(&fixture, "legacy-five-scans", iterations, || {
        rssp::course::profile_course_banner(fixture.course_path(), "", true)
    });
    run_course_banner_phase(&fixture, "one-scan-full-path-stats", iterations, || {
        rssp::course::profile_course_banner_full_paths(fixture.course_path(), "")
    });
    run_course_banner_phase(&fixture, "one-scan-entry-types", iterations, || {
        rssp::course::profile_course_banner(fixture.course_path(), "", false)
    });
}

fn run_course_resolve_phase(
    fixture: &course_bench::ResolveFixture,
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    let resolve = || {
        rssp::course::profile_resolve_song_dir(
            fixture.songs_dir(),
            None,
            course_bench::RESOLVE_SONG,
            legacy,
        )
    };
    black_box(resolve().expect("benchmark song should resolve"));

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let song_dir = resolve().expect("benchmark song should resolve");
        checksum = checksum.wrapping_add(song_dir.as_os_str().len());
        black_box(song_dir);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=course-resolve phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        course_bench::RESOLVE_ENTRY_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_course_resolve_alloc(iterations: usize) {
    let fixture = course_bench::ResolveFixture::new();
    fixture.assert_behavior();
    run_course_resolve_phase(&fixture, "full-paths-metadata-keys", iterations, true);
    run_course_resolve_phase(&fixture, "entry-types-names", iterations, false);
}

fn run_pack_ini_phase(phase: &str, iterations: usize, mode: u8) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let parsed = match mode {
            0 => rssp::pack::profile_parse_pack_ini(black_box(pack_bench::PACK_INI_INPUT), true),
            1 => rssp::pack::profile_parse_pack_ini_dispatch(
                black_box(pack_bench::PACK_INI_INPUT),
                true,
            ),
            _ => rssp::pack::profile_parse_pack_ini_dispatch(
                black_box(pack_bench::PACK_INI_INPUT),
                false,
            ),
        };
        checksum = checksum.wrapping_add(parsed);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=pack-ini phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_bytes_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        pack_bench::PACK_INI_INPUT.len() as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_pack_ini_alloc(iterations: usize) {
    pack_bench::assert_pack_ini_behavior();
    run_pack_ini_phase("owned-fields", iterations, 0);
    run_pack_ini_phase("sequential-key-dispatch", iterations, 1);
    run_pack_ini_phase("indexed-key-dispatch", iterations, 2);
}

fn run_pack_hint_phase(phase: &str, iterations: usize, legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for _ in 0..pack_bench::HINT_NORM_BATCH {
            let hint = rssp::pack::profile_normalized_img_hint(
                black_box(pack_bench::HINT_NORM_INPUT),
                legacy,
            );
            checksum = checksum.wrapping_add(hint.as_deref().map_or(0, str::len));
            black_box(hint);
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=pack-hint phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_hints_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        pack_bench::HINT_NORM_BATCH as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_pack_hint_alloc(iterations: usize) {
    pack_bench::assert_hint_norm_behavior();
    run_pack_hint_phase("owned", iterations, true);
    run_pack_hint_phase("borrowed", iterations, false);
}

fn run_pack_root_phase<F>(phase: &str, iterations: usize, scan: F)
where
    F: Fn(
        &Path,
        rssp::pack::ScanOpt,
        &str,
        &str,
    ) -> Result<rssp::profile::PackRootResult, rssp::pack::ScanError>,
{
    let fixture = pack_bench::PackFixture::new();
    let scan = || {
        scan(
            fixture.pack_dir(),
            rssp::pack::ScanOpt::default(),
            pack_bench::BANNER_HINT,
            pack_bench::BACKGROUND_HINT,
        )
    };
    black_box(scan().expect("benchmark pack root should scan"));

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let (banner, background, songs) = scan().expect("benchmark pack root should scan");
        checksum = checksum
            .wrapping_add(banner.is_some() as usize)
            .wrapping_add(background.is_some() as usize)
            .wrapping_add(songs.len());
        black_box((banner, background, songs));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=pack-root phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        pack_bench::ROOT_ENTRY_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_pack_root_alloc(iterations: usize) {
    let fixture = pack_bench::PackFixture::new();
    fixture.assert_root_behavior();
    run_pack_root_phase(
        "legacy-repeated-scans",
        iterations,
        rssp::profile::pack_root_legacy,
    );
    run_pack_root_phase(
        "full-path-stats",
        iterations,
        rssp::profile::pack_root_full_paths,
    );
    run_pack_root_phase("cached-entry-types", iterations, rssp::profile::pack_root);
}

fn run_path_sort_phase(phase: &str, iterations: usize, legacy: bool) {
    let mut paths = path_sort_bench::paths();
    rssp::profile::sort_paths_ci(&mut paths, false);
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        paths.reverse();
        rssp::profile::sort_paths_ci(black_box(&mut paths), legacy);
        checksum = checksum.wrapping_add(paths[0].as_os_str().len());
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=path-sort phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_paths_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        path_sort_bench::PATH_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
    black_box(paths);
}

fn run_path_sort_alloc(iterations: usize) {
    path_sort_bench::assert_behavior();
    run_path_sort_phase("cached-strings", iterations, true);
    run_path_sort_phase("contiguous-keys", iterations, false);
}

fn run_pack_scan_alloc(iterations: usize) {
    let fixture = pack_bench::PackFixture::new();
    let image_fixture = pack_bench::ImageHintFixture::new();
    fixture.assert_song_behavior();
    fixture.assert_tree_behavior();
    fixture.assert_songs_behavior();
    fixture.assert_parent_img_behavior();
    image_fixture.assert_behavior();
    run_subdir_img_phase(
        &image_fixture,
        "full-paths",
        iterations,
        rssp::profile::pack_subdir_img_legacy,
    );
    run_subdir_img_phase(
        &image_fixture,
        "candidate-names",
        iterations,
        rssp::profile::pack_subdir_img,
    );
    run_parent_img_phase(
        &fixture,
        "full-path-stats",
        iterations,
        rssp::profile::pack_parent_img_legacy,
    );
    run_parent_img_phase(
        &fixture,
        "candidate-names",
        iterations,
        rssp::profile::pack_parent_img,
    );
    run_songs_root_phase(
        &fixture,
        "probe-every-entry",
        iterations,
        rssp::profile::scan_songs_dir_legacy,
    );
    run_songs_root_phase(
        &fixture,
        "cached-dir-types",
        iterations,
        rssp::pack::scan_songs_dir,
    );
    run_simfile_tree_phase(
        &fixture,
        "rescan-subdirs",
        iterations,
        rssp::profile::find_simfiles_legacy,
    );
    run_simfile_tree_phase(
        &fixture,
        "one-snapshot",
        iterations,
        rssp::pack::find_simfiles,
    );
    run_song_scan_phase(
        fixture.song_dir(),
        pack_bench::SONG_ENTRY_COUNT,
        "full-paths",
        iterations,
        rssp::pack::ScanOpt::default(),
        rssp::profile::scan_song_dir_full_paths,
    );
    run_song_scan_phase(
        fixture.song_dir(),
        pack_bench::SONG_ENTRY_COUNT,
        "candidate-names",
        iterations,
        rssp::pack::ScanOpt::default(),
        rssp::pack::scan_song_dir,
    );
    let duplicate_opt = rssp::pack::ScanOpt {
        dup: rssp::pack::DupPolicy::Error,
    };
    run_song_scan_phase(
        fixture.song_dir(),
        pack_bench::SONG_ENTRY_COUNT,
        "joined-paths-error",
        iterations,
        duplicate_opt,
        rssp::profile::scan_song_dir_joined_paths,
    );
    run_song_scan_phase(
        fixture.song_dir(),
        pack_bench::SONG_ENTRY_COUNT,
        "deferred-paths-error",
        iterations,
        duplicate_opt,
        rssp::pack::scan_song_dir,
    );
    run_song_scan_phase(
        fixture.single_song_dir(),
        pack_bench::SINGLE_SONG_ENTRY_COUNT,
        "growing-names-error-single",
        iterations,
        duplicate_opt,
        rssp::profile::scan_song_dir_growing_names,
    );
    run_song_scan_phase(
        fixture.single_song_dir(),
        pack_bench::SINGLE_SONG_ENTRY_COUNT,
        "inline-first-error-single",
        iterations,
        duplicate_opt,
        rssp::pack::scan_song_dir,
    );
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

fn run_subdir_img_phase<F>(
    fixture: &pack_bench::ImageHintFixture,
    phase: &str,
    iterations: usize,
    pick: F,
) where
    F: Fn(&Path, &str) -> Option<PathBuf>,
{
    black_box(pick(fixture.pack_dir(), pack_bench::SUBDIR_HINT));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let image = pick(
            black_box(fixture.pack_dir()),
            black_box(pack_bench::SUBDIR_HINT),
        );
        checksum = checksum.wrapping_add(usize::from(image.is_some()));
        black_box(image);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=pack-subdir-img phase={} iters={} checksum={} elapsed_s={:.6} ",
            "entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        pack_bench::HINT_ENTRY_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_parent_img_phase<F>(
    fixture: &pack_bench::PackFixture,
    phase: &str,
    iterations: usize,
    pick: F,
) where
    F: Fn(&Path, &str) -> Option<PathBuf>,
{
    black_box(pick(fixture.pack_dir(), "Performance Pack"));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let image = pick(black_box(fixture.pack_dir()), black_box("Performance Pack"));
        checksum = checksum.wrapping_add(usize::from(image.is_some()));
        black_box(image);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=pack-parent-img phase={} iters={} checksum={} elapsed_s={:.6} ",
            "entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        pack_bench::SONGS_ROOT_ENTRY_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_songs_root_phase<F>(
    fixture: &pack_bench::PackFixture,
    phase: &str,
    iterations: usize,
    scan: F,
) where
    F: Fn(&Path, rssp::pack::ScanOpt) -> Result<Vec<rssp::pack::PackScan>, rssp::pack::ScanError>,
{
    black_box(
        scan(fixture.tree_root(), rssp::pack::ScanOpt::default())
            .expect("benchmark Songs root should scan"),
    );
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let packs = scan(
            black_box(fixture.tree_root()),
            black_box(rssp::pack::ScanOpt::default()),
        )
        .expect("benchmark Songs root should scan");
        checksum = checksum.wrapping_add(packs.len());
        black_box(packs);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=songs-root phase={} iters={} checksum={} elapsed_s={:.6} ",
            "entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        pack_bench::SONGS_ROOT_ENTRY_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_simfile_tree_phase<F>(
    fixture: &pack_bench::PackFixture,
    phase: &str,
    iterations: usize,
    find: F,
) where
    F: Fn(&Path, rssp::pack::ScanOpt) -> Vec<PathBuf>,
{
    black_box(find(fixture.tree_root(), rssp::pack::ScanOpt::default()));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let paths = find(
            black_box(fixture.tree_root()),
            black_box(rssp::pack::ScanOpt::default()),
        );
        checksum = checksum.wrapping_add(paths.len());
        black_box(paths);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=simfile-tree phase={} iters={} checksum={} elapsed_s={:.6} ",
            "entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        pack_bench::TREE_ENTRY_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_song_scan_phase<F>(
    dir: &Path,
    entry_count: usize,
    phase: &str,
    iterations: usize,
    opt: rssp::pack::ScanOpt,
    scan: F,
) where
    F: Fn(
        &Path,
        rssp::pack::ScanOpt,
    ) -> Result<Option<rssp::pack::SongScan>, rssp::pack::ScanError>,
{
    fn result_len(result: Result<Option<rssp::pack::SongScan>, rssp::pack::ScanError>) -> usize {
        match result {
            Ok(song) => usize::from(song.is_some()),
            Err(rssp::pack::ScanError::DuplicateSimfile { paths, .. }) => paths.len(),
            Err(error) => panic!("benchmark song should scan: {error:?}"),
        }
    }

    black_box(result_len(scan(dir, opt)));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let count = result_len(scan(black_box(dir), black_box(opt)));
        checksum = checksum.wrapping_add(count);
        black_box(count);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=song-scan phase={} iters={} checksum={} elapsed_s={:.6} ",
            "entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        entry_count as f64 * divisor / elapsed.as_secs_f64(),
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
    selectable_bench::assert_behavior();
    run_selectable_alloc::<true>("owned_compare", iterations);
    run_selectable_alloc::<false>("borrowed_compare", iterations);

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

fn run_selectable_alloc<const LEGACY: bool>(phase: &str, iterations: usize) {
    black_box(selectable_bench::run::<LEGACY>());
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(selectable_bench::run::<LEGACY>());
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=metadata-analyze phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_tags_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        selectable_bench::BATCH as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_text_report_phase(
    summary: &rssp::report::SimfileSummary,
    full: bool,
    phase: &str,
    legacy: bool,
    iterations: usize,
) {
    let mut sizing = Vec::new();
    text_report_bench::write(summary, &mut sizing, full, false);
    let mut output = Vec::with_capacity(sizing.len());
    black_box(text_report_bench::write(summary, &mut output, full, legacy));

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(text_report_bench::write(
            black_box(summary),
            black_box(&mut output),
            full,
            legacy,
        ));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=text-report phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_charts_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
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

fn run_text_report_alloc(iterations: usize) {
    let fixture = metadata_bench::fixture("0.83");
    let summary = rssp::analyze(fixture.as_bytes(), "ssc", &metadata_bench::options())
        .expect("text report fixture should analyze");
    text_report_bench::assert_behavior(&summary);
    for (phase, full, legacy) in [
        ("pretty-materialized", false, true),
        ("pretty-streamed", false, false),
        ("full-materialized", true, true),
        ("full-streamed", true, false),
    ] {
        run_text_report_phase(&summary, full, phase, legacy, iterations);
    }
}

fn run_json_report_phase(
    mode: &str,
    phase: &str,
    item_count: usize,
    iterations: usize,
    summary: &rssp::report::SimfileSummary,
    write: impl Fn(&rssp::report::SimfileSummary, &mut Vec<u8>) -> std::io::Result<()>,
) {
    let mut warm_output = Vec::new();
    write(summary, &mut warm_output).expect("JSON benchmark should write");
    black_box(warm_output);

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let mut output = Vec::new();
        write(black_box(summary), black_box(&mut output)).expect("JSON benchmark should write");
        checksum = checksum.wrapping_add(output.len());
        black_box(output);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode={} phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_items_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        mode,
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        item_count as f64 * divisor / elapsed.as_secs_f64(),
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
    run_json_report_phase(
        "json-timing",
        "materialized",
        report_timing_bench::SEGMENT_COUNT,
        iterations,
        &summary,
        rssp::profile::write_json_materialized,
    );
    run_json_report_phase(
        "json-timing",
        "streamed",
        report_timing_bench::SEGMENT_COUNT,
        iterations,
        &summary,
        |summary, output| {
            rssp::report::write_reports(summary, rssp::report::OutputMode::JSON, output)
        },
    );
}

fn run_bpm_text_json_alloc(iterations: usize) {
    let fixture = report_timing_bench::fixture();
    let summary = rssp::analyze(fixture.as_bytes(), "ssc", &report_timing_bench::options())
        .expect("BPM text JSON benchmark should analyze");
    run_json_report_phase(
        "json-bpm-text",
        "materialized",
        report_timing_bench::SEGMENT_COUNT,
        iterations,
        &summary,
        rssp::profile::write_json_bpm_text_report_materialized,
    );
    run_json_report_phase(
        "json-bpm-text",
        "streamed",
        report_timing_bench::SEGMENT_COUNT,
        iterations,
        &summary,
        |summary, output| {
            rssp::report::write_reports(summary, rssp::report::OutputMode::JSON, output)
        },
    );
}

fn run_hash_bpms_json_alloc(iterations: usize) {
    let fixture = report_timing_bench::chart_bpm_fixture();
    let summary = rssp::analyze(fixture.as_bytes(), "ssc", &report_timing_bench::options())
        .expect("hash BPM JSON benchmark should analyze");
    let mut legacy = summary.clone();
    legacy
        .charts
        .first_mut()
        .expect("hash BPM fixture should contain a chart")
        .chart_bpms_norm = None;
    run_json_report_phase(
        "json-hash-bpms",
        "renormalized",
        report_timing_bench::SEGMENT_COUNT,
        iterations,
        &legacy,
        |summary, output| {
            rssp::report::write_reports(summary, rssp::report::OutputMode::JSON, output)
        },
    );
    run_json_report_phase(
        "json-hash-bpms",
        "precomputed",
        report_timing_bench::SEGMENT_COUNT,
        iterations,
        &summary,
        |summary, output| {
            rssp::report::write_reports(summary, rssp::report::OutputMode::JSON, output)
        },
    );
}

fn run_custom_pattern_json_alloc(iterations: usize) {
    let summary = report_patterns_bench::summary();
    let pattern_count = summary
        .charts
        .iter()
        .map(|chart| chart.custom_patterns.len())
        .sum();
    run_json_report_phase(
        "json-custom-patterns",
        "materialized-map",
        pattern_count,
        iterations,
        &summary,
        rssp::profile::write_json_custom_report_materialized,
    );
    run_json_report_phase(
        "json-custom-patterns",
        "streamed",
        pattern_count,
        iterations,
        &summary,
        |summary, output| {
            rssp::report::write_reports(summary, rssp::report::OutputMode::JSON, output)
        },
    );
}

fn run_nps_json_alloc(iterations: usize) {
    let fixture = report_nps_bench::fixture();
    let summary = rssp::analyze(&fixture, "ssc", &report_nps_bench::options())
        .expect("NPS JSON benchmark should analyze");
    run_json_report_phase(
        "json-nps",
        "materialized",
        report_nps_bench::MEASURE_COUNT,
        iterations,
        &summary,
        rssp::profile::write_json_nps_report_materialized,
    );
    run_json_report_phase(
        "json-nps",
        "streamed",
        report_nps_bench::MEASURE_COUNT,
        iterations,
        &summary,
        |summary, output| {
            rssp::report::write_reports(summary, rssp::report::OutputMode::JSON, output)
        },
    );
}

fn run_stream_json_alloc(iterations: usize) {
    let fixture = report_nps_bench::fixture();
    let summary = rssp::analyze(&fixture, "ssc", &report_nps_bench::options())
        .expect("stream JSON benchmark should analyze");
    run_json_report_phase(
        "json-streams",
        "materialized",
        report_nps_bench::MEASURE_COUNT,
        iterations,
        &summary,
        rssp::profile::write_json_streams_report_materialized,
    );
    run_json_report_phase(
        "json-streams",
        "streamed",
        report_nps_bench::MEASURE_COUNT,
        iterations,
        &summary,
        |summary, output| {
            rssp::report::write_reports(summary, rssp::report::OutputMode::JSON, output)
        },
    );
}

fn run_bgchanges_phase(
    phase: &str,
    iterations: usize,
    item_count: usize,
    fixture: &assets_bench::AssetFixture,
    simfile: &[u8],
    resolve: impl Fn(&std::path::Path, &[u8]) -> Vec<rssp::assets::ResolvedBackgroundChange>,
) {
    black_box(resolve(fixture.song_dir(), simfile));

    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let changes = resolve(black_box(fixture.song_dir()), black_box(simfile));
        checksum = checksum.wrapping_add(changes.len());
        black_box(changes);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=background-changes phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_changes_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        item_count as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_bgchange_sort_phase(phase: &str, iterations: usize, legacy: bool) {
    let mut changes = assets_bench::ordered_changes();
    rssp::profile::sort_background_changes(&mut changes, true, legacy);
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u32;
    for _ in 0..iterations {
        rssp::profile::sort_background_changes(black_box(&mut changes), true, legacy);
        checksum = checksum
            .wrapping_add(changes[0].start_beat.to_bits())
            .wrapping_add(changes[changes.len() - 1].start_beat.to_bits());
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=background-ordered-sort phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_changes_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        assets_bench::ORDERED_CHANGE_COUNT as f64 * divisor / elapsed.as_secs_f64(),
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
    let fixture = assets_bench::AssetFixture::with_movies(1);
    fixture.assert_background_behavior();
    fixture.assert_catalog_behavior();
    assets_bench::assert_bgchange_sort_behavior();
    run_bgchange_sort_phase("always-sort", iterations, true);
    run_bgchange_sort_phase("ordered-fast-path", iterations, false);
    let ordered = fixture.simfile();
    run_bgchanges_phase(
        "root-rescan",
        iterations,
        assets_bench::CHANGE_COUNT,
        &fixture,
        ordered,
        rssp::profile::background_changes_legacy,
    );
    run_bgchanges_phase(
        "double-find",
        iterations,
        assets_bench::CHANGE_COUNT,
        &fixture,
        ordered,
        rssp::profile::background_changes_double_find,
    );
    run_bgchanges_phase(
        "materialized-values",
        iterations,
        assets_bench::CHANGE_COUNT,
        &fixture,
        ordered,
        rssp::profile::background_changes_materialized,
    );
    run_bgchanges_phase(
        "path-metadata",
        iterations,
        assets_bench::CHANGE_COUNT,
        &fixture,
        ordered,
        rssp::profile::background_changes_path_metadata,
    );
    run_bgchanges_phase(
        "always-sort",
        iterations,
        assets_bench::CHANGE_COUNT,
        &fixture,
        ordered,
        rssp::profile::background_changes_always_sort,
    );
    run_bgchanges_phase(
        "growing-paths",
        iterations,
        assets_bench::CHANGE_COUNT,
        &fixture,
        ordered,
        rssp::profile::background_changes_growing_paths,
    );
    run_bgchanges_phase(
        "preallocated-paths",
        iterations,
        assets_bench::CHANGE_COUNT,
        &fixture,
        ordered,
        rssp::assets::resolve_background_changes_like_itg,
    );

    let unordered = fixture.unordered_simfile();
    fixture.assert_unordered_behavior(&unordered);
    run_bgchanges_phase(
        "unordered-linear-upsert",
        iterations,
        assets_bench::UNORDERED_PAIR_COUNT,
        &fixture,
        &unordered,
        rssp::profile::background_changes_linear_upsert,
    );
    run_bgchanges_phase(
        "unordered-growing-paths",
        iterations,
        assets_bench::UNORDERED_PAIR_COUNT,
        &fixture,
        &unordered,
        rssp::profile::background_changes_growing_paths,
    );
    run_bgchanges_phase(
        "unordered-preallocated-paths",
        iterations,
        assets_bench::UNORDERED_PAIR_COUNT,
        &fixture,
        &unordered,
        rssp::assets::resolve_background_changes_like_itg,
    );

    let tags = assets_bench::bgchange_tags();
    assets_bench::assert_bgchange_values_behavior(&tags);
    run_bgchange_values_phase("materialized", iterations, &tags, |data| {
        let values = rssp::parse::extract_bgchanges_values(data);
        let count = values.len();
        black_box(values);
        count
    });
    run_bgchange_values_phase("streamed", iterations, &tags, |data| {
        rssp::parse::bgchanges_values(data).count()
    });
}

fn run_bgchange_values_phase(
    phase: &str,
    iterations: usize,
    input: &[u8],
    scan: impl Fn(&[u8]) -> usize,
) {
    black_box(scan(input));
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(scan(black_box(input)));
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=background-change-values phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_tags_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        assets_bench::BG_TAG_COUNT as f64 * divisor / elapsed.as_secs_f64(),
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
    fixture.assert_music_behavior();
    fixture.assert_rel_path_behavior();
    run_music_fallback_phase(&fixture, "full-paths", iterations, true);
    run_music_fallback_phase(&fixture, "candidate-names", iterations, false);
    let relative_paths = assets_bench::relative_paths();
    run_relative_asset_phase(
        &fixture,
        &relative_paths,
        "materialized-components",
        iterations,
        true,
    );
    run_relative_asset_phase(
        &fixture,
        &relative_paths,
        "inline-components",
        iterations,
        false,
    );
    let relative_component_paths = assets_bench::relative_component_paths();
    assets_bench::assert_rel_component_behavior(&relative_component_paths);
    run_relative_component_phase(
        &relative_component_paths,
        "materialized-components",
        iterations,
        true,
    );
    run_relative_component_phase(
        &relative_component_paths,
        "inline-components",
        iterations,
        false,
    );

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

fn run_relative_asset_phase(
    fixture: &assets_bench::AssetFixture,
    paths: &[String],
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        for path in black_box(paths) {
            let resolved = rssp::profile::relative_asset_path(
                black_box(fixture.relative_dir()),
                black_box(path),
                legacy,
            );
            checksum = checksum.wrapping_add(usize::from(resolved.is_some()));
            black_box(resolved);
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=asset-fallbacks stage=relative-path phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_paths_s={:.3} alloc_calls_per_iter={:.1} dealloc_calls_per_iter={:.1} ",
            "realloc_calls_per_iter={:.1} alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        assets_bench::REL_PATH_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_relative_component_phase(paths: &[String], phase: &str, iterations: usize, legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        for path in black_box(paths) {
            checksum = checksum.rotate_left(1)
                ^ rssp::profile::relative_asset_parts_hash(black_box(path), legacy);
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=asset-fallbacks stage=relative-components phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_paths_s={:.3} alloc_calls_per_iter={:.1} dealloc_calls_per_iter={:.1} ",
            "realloc_calls_per_iter={:.1} alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        assets_bench::REL_COMPONENT_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_music_fallback_phase(
    fixture: &assets_bench::AssetFixture,
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let music = if legacy {
            rssp::profile::music_path_legacy(black_box(fixture.song_dir()), black_box(""))
        } else {
            rssp::assets::resolve_music_path_like_itg(black_box(fixture.song_dir()), black_box(""))
        };
        checksum = checksum.wrapping_add(usize::from(music.is_some()));
        black_box(music);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=music-fallback phase={} iters={} checksum={} elapsed_s={:.6} ",
            "entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        assets_bench::SOUND_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_song_assets_phase(
    fixture: &assets_bench::AssetFixture,
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let (banner, background) = if legacy {
            rssp::profile::song_assets_legacy(
                black_box(fixture.image_dir()),
                black_box(""),
                black_box(""),
            )
        } else {
            rssp::assets::resolve_song_assets(
                black_box(fixture.image_dir()),
                black_box(""),
                black_box(""),
            )
        };
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
            "mode=song-assets phase={} iters={} checksum={} elapsed_s={:.6} ",
            "entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
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

fn run_song_assets_alloc(iterations: usize) {
    let fixture = assets_bench::AssetFixture::new();
    fixture.assert_song_assets_behavior();
    run_song_assets_phase(&fixture, "full-candidate-paths", iterations, true);
    run_song_assets_phase(&fixture, "candidate-names", iterations, false);
}

fn run_translate_markers_phase(input: &str, phase: &str, iterations: usize, legacy: bool) {
    let mut text = String::with_capacity(input.len());
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        text.clear();
        text.push_str(input);
        rssp::translate::profile_replace_markers(black_box(&mut text), legacy);
        checksum = checksum.wrapping_add(text.len());
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=translate-markers phase={} iters={} checksum={} elapsed_s={:.6} ",
            "markers_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        translate_bench::MARKER_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_alias_build_phase(phase: &str, iterations: usize, legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0usize;
    for index in 0..iterations {
        let value = rssp::translate::profile_alias_build(black_box(index), legacy);
        checksum = checksum
            .wrapping_add(value.0)
            .wrapping_add(value.1 as usize);
        black_box(value);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=alias-table stage=build phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_tables_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_alias_lookup_phase(phase: &str, iterations: usize, aliases: &[&str], legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u32;
    for _ in 0..iterations {
        for &alias in aliases {
            checksum = checksum.wrapping_add(
                rssp::translate::profile_alias_lookup(black_box(alias), legacy)
                    .map_or(0, u32::from),
            );
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=alias-table stage=lookup phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_lookups_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        aliases.len() as f64 * divisor / elapsed.as_secs_f64(),
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
    const ALIASES: [&str; 12] = [
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
    translate_bench::assert_behavior();
    assert!(rssp::translate::profile_alias_tables_match());
    let (legacy_bytes, compact_bytes) = rssp::translate::profile_alias_table_sizes();
    println!("mode=alias-table legacy_bytes={legacy_bytes} compact_bytes={compact_bytes}");
    run_alias_build_phase("runtime", iterations, true);
    run_alias_build_phase("static", iterations, false);
    run_alias_lookup_phase("legacy", iterations, &ALIASES, true);
    run_alias_lookup_phase("compact", iterations, &ALIASES, false);
    let input = translate_bench::alias_input();
    run_translate_markers_phase(&input, "allocating", iterations, true);
    run_translate_markers_phase(&input, "compact", iterations, false);
}

fn run_last_beat_phase(chart: &[u8], phase: &str, iterations: usize, legacy: bool) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        for _ in 0..last_beat_bench::LAST_BEAT_BATCH {
            let beat =
                rssp::stats::chart_last_beat_for_bench(black_box(chart), black_box(4), legacy);
            checksum = checksum.wrapping_add(beat.to_bits());
            black_box(beat);
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=last-beat phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_mib_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        chart.len() as f64 * last_beat_bench::LAST_BEAT_BATCH as f64 * divisor
            / elapsed.as_secs_f64()
            / (1024.0 * 1024.0),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_last_beat_alloc(iterations: usize) {
    last_beat_bench::assert_behavior();
    let chart = last_beat_bench::chart(last_beat_bench::MEASURE_COUNT, last_beat_bench::ROW_COUNT);
    run_last_beat_phase(&chart, "heap-measure", iterations, true);
    run_last_beat_phase(&chart, "stack-measure", iterations, false);
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

fn run_matrix_rating_phase(
    phase: &str,
    iterations: usize,
    profiles: &[rssp::matrix::MatrixProfile],
    legacy: bool,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        for profile in profiles {
            let rating = if legacy {
                rssp::matrix::matrix_rating_at_rate_legacy_for_bench(profile, 1.25)
            } else {
                profile.rating_at_rate(1.25)
            };
            checksum = checksum.wrapping_add(rating.to_bits());
        }
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    let entries = profiles.iter().map(|profile| profile.len()).sum::<usize>() as f64 * divisor;
    println!(
        concat!(
            "mode=matrix-rating phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_entries_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        entries / elapsed.as_secs_f64(),
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
    run_matrix_rating_phase("legacy-search", iterations, &profiles, true);
    run_matrix_rating_phase("compile-time-lookup", iterations, &profiles, false);
}

fn run_elapsed_phase(
    fixture: &elapsed_bench::ElapsedFixture,
    phase: &str,
    iterations: usize,
    legacy: bool,
) {
    reset_counters();
    let before = Counters::read();
    let start = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        let elapsed = rssp::bpm::get_elapsed_time_for_bench(
            black_box(fixture.target),
            black_box(&fixture.bpms),
            black_box(&fixture.stops),
            black_box(&fixture.delays),
            black_box(&fixture.warps),
            legacy,
        );
        checksum = checksum.wrapping_add(elapsed.to_bits());
        black_box(elapsed);
    }
    let elapsed = start.elapsed();
    let after = Counters::read();
    let divisor = iterations as f64;
    println!(
        concat!(
            "mode=elapsed-events phase={} iters={} checksum={} elapsed_s={:.6} ",
            "throughput_events_s={:.3} alloc_calls_per_iter={:.1} ",
            "dealloc_calls_per_iter={:.1} realloc_calls_per_iter={:.1} ",
            "alloc_bytes_per_iter={:.1} realloc_bytes_per_iter={:.1} ",
            "live_growth_bytes={} peak_live_growth_bytes={}"
        ),
        phase,
        iterations,
        black_box(checksum),
        elapsed.as_secs_f64(),
        elapsed_bench::EVENT_COUNT as f64 * divisor / elapsed.as_secs_f64(),
        (after.alloc_calls - before.alloc_calls) as f64 / divisor,
        (after.dealloc_calls - before.dealloc_calls) as f64 / divisor,
        (after.realloc_calls - before.realloc_calls) as f64 / divisor,
        (after.alloc_bytes - before.alloc_bytes) as f64 / divisor,
        (after.realloc_bytes - before.realloc_bytes) as f64 / divisor,
        after.live_bytes as isize - before.live_bytes as isize,
        after.peak_live_bytes.saturating_sub(before.live_bytes),
    );
}

fn run_elapsed_alloc(iterations: usize) {
    let fixture = elapsed_bench::ElapsedFixture::new();
    elapsed_bench::assert_behavior(&fixture);
    run_elapsed_phase(&fixture, "collect-sort", iterations, true);
    run_elapsed_phase(&fixture, "stable-merge", iterations, false);
}

fn main() {
    let (mode, iterations) = parse_args();
    match mode {
        Mode::ParseDispatch => {
            run_parse_dispatch_alloc(iterations);
            return;
        }
        Mode::ParseAppend => {
            run_parse_append_alloc(iterations);
            return;
        }
        Mode::ParseReserve => {
            run_parse_reserve_alloc(iterations);
            return;
        }
        Mode::TechPrefix => {
            run_tech_prefix_alloc(iterations);
            return;
        }
        Mode::BpmStats => {
            run_bpm_stats_alloc(iterations);
            return;
        }
        Mode::ElapsedEvents => {
            run_elapsed_alloc(iterations);
            return;
        }
        Mode::CleanMap => {
            run_clean_map_alloc(iterations);
            return;
        }
        Mode::NormalizeMap => {
            run_normalize_map_alloc(iterations);
            return;
        }
        Mode::FusedMap => {
            run_fused_map_alloc(iterations);
            return;
        }
        Mode::DisplayBpm => {
            run_display_bpm_alloc(iterations);
            return;
        }
        Mode::BpmDisplayTags => {
            run_bpm_display_tags_alloc(iterations);
            return;
        }
        Mode::CustomCompile => {
            run_custom_pattern_alloc(iterations);
            return;
        }
        Mode::DefaultPatternDfa => {
            run_default_dfa_alloc(iterations);
            return;
        }
        Mode::StreamOutputs => {
            run_stream_outputs_alloc(iterations);
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
        Mode::CourseHashDedup => {
            run_course_hash_dedup_alloc(iterations);
            return;
        }
        Mode::CourseParse => {
            run_course_parse_alloc(iterations);
            return;
        }
        Mode::CourseEntryReserve => {
            run_course_entry_reserve_alloc(iterations);
            return;
        }
        Mode::CourseMods => {
            run_course_mods_alloc(iterations);
            return;
        }
        Mode::CourseSelectMods => {
            run_select_mods_alloc(iterations);
            return;
        }
        Mode::CourseSelectParse => {
            run_select_parse_alloc(iterations);
            return;
        }
        Mode::TimingBuild => {
            run_timing_build_alloc(iterations);
            return;
        }
        Mode::TimingSort => {
            run_timing_sort_alloc(iterations);
            return;
        }
        Mode::SmTiming => {
            run_sm_timing_alloc(iterations);
            return;
        }
        Mode::TimingMerge => {
            run_timing_merge_alloc(iterations);
            return;
        }
        Mode::TimingText => {
            run_timing_text_alloc(iterations);
            return;
        }
        Mode::Serialize => {
            run_serialize_alloc(iterations);
            return;
        }
        Mode::NpsStats => {
            run_nps_stats_alloc(iterations);
            return;
        }
        Mode::NpsCursor => {
            run_nps_cursor_alloc(iterations);
            return;
        }
        Mode::CourseStepType => {
            run_stepstype_alloc(iterations);
            return;
        }
        Mode::CourseTitleMatch => {
            run_title_match_alloc(iterations);
            return;
        }
        Mode::CourseBanner => {
            run_course_banner_alloc(iterations);
            return;
        }
        Mode::CourseResolve => {
            run_course_resolve_alloc(iterations);
            return;
        }
        Mode::PackIni => {
            run_pack_ini_alloc(iterations);
            return;
        }
        Mode::PackHintNormalize => {
            run_pack_hint_alloc(iterations);
            return;
        }
        Mode::PackRoot => {
            run_pack_root_alloc(iterations);
            return;
        }
        Mode::PackScan => {
            run_pack_scan_alloc(iterations);
            return;
        }
        Mode::PathSort => {
            run_path_sort_alloc(iterations);
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
        Mode::LastBeat => {
            run_last_beat_alloc(iterations);
            return;
        }
        Mode::MetadataAnalyze => {
            run_metadata_analyze_alloc(iterations);
            return;
        }
        Mode::TextReport => {
            run_text_report_alloc(iterations);
            return;
        }
        Mode::JsonNps => {
            run_nps_json_alloc(iterations);
            return;
        }
        Mode::JsonBpmText => {
            run_bpm_text_json_alloc(iterations);
            return;
        }
        Mode::JsonCustomPatterns => {
            run_custom_pattern_json_alloc(iterations);
            return;
        }
        Mode::JsonHashBpms => {
            run_hash_bpms_json_alloc(iterations);
            return;
        }
        Mode::JsonStreams => {
            run_stream_json_alloc(iterations);
            return;
        }
        Mode::JsonTiming => {
            run_timing_json_alloc(iterations);
            return;
        }
        Mode::ParitySingle => {
            run_note_parse_alloc(iterations);
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
    if matches!(mode, Mode::Nps) {
        run_nps_alloc(iterations, &corpus);
        return;
    }
    if matches!(mode, Mode::Durations) {
        run_duration_alloc(iterations, &corpus);
        return;
    }
    if matches!(mode, Mode::Minimize) {
        run_minimize_alloc(iterations, &corpus);
        return;
    }
    let options = options_for(mode);
    if matches!(mode, Mode::AnalysisReuse) {
        run_analysis_reuse_alloc(iterations, &corpus, &options);
        return;
    }
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
