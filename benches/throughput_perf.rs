use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

const DEFAULT_MAX_FILES: usize = 96;
const DEFAULT_MAX_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_SAMPLE_SIZE: usize = 20;
const DEFAULT_MEASURE_SECS: u64 = 12;

#[derive(Debug)]
struct SimInput {
    extension: &'static str,
    raw: Vec<u8>,
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name).ok().map_or(default, |v| {
        !matches!(
            v.trim(),
            "0" | "false" | "False" | "FALSE" | "no" | "No" | "NO"
        )
    })
}

fn inner_extension(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    if !ext.eq_ignore_ascii_case("zst") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let inner_ext = Path::new(stem).extension()?.to_str()?;
    if inner_ext.eq_ignore_ascii_case("sm") {
        Some("sm")
    } else if inner_ext.eq_ignore_ascii_case("ssc") {
        Some("ssc")
    } else {
        None
    }
}

fn collect_pack_files(packs_dir: &Path) -> Vec<(PathBuf, &'static str)> {
    let mut files = Vec::new();
    for entry in WalkDir::new(packs_dir)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(extension) = inner_extension(path) else {
            continue;
        };
        files.push((path.to_path_buf(), extension));
    }
    files.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    files
}

fn load_pack_corpus(packs_dir: &Path, max_files: usize, max_bytes: usize) -> Vec<SimInput> {
    let mut corpus = Vec::new();
    let mut total_bytes = 0usize;

    for (path, extension) in collect_pack_files(packs_dir) {
        if corpus.len() >= max_files || total_bytes >= max_bytes {
            break;
        }
        let Ok(compressed) = fs::read(&path) else {
            continue;
        };
        let Ok(raw) = zstd::decode_all(&compressed[..]) else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        if total_bytes + raw.len() > max_bytes && !corpus.is_empty() {
            break;
        }
        total_bytes += raw.len();
        corpus.push(SimInput { extension, raw });
    }
    corpus
}

fn load_fixture_corpus(manifest_dir: &Path) -> Vec<SimInput> {
    const FIXTURES: [(&str, &str); 4] = [
        ("benches/fixtures/camellia_mix.ssc", "ssc"),
        ("benches/fixtures/hash_fixture.ssc", "ssc"),
        ("benches/fixtures/200000_step_challenge.sm", "sm"),
        ("benches/fixtures/24h_of_100bpm_stream.sm", "sm"),
    ];

    let mut corpus = Vec::new();
    for (rel, extension) in FIXTURES {
        let path = manifest_dir.join(rel);
        let Ok(raw) = fs::read(path) else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        corpus.push(SimInput { extension, raw });
    }
    corpus
}

fn load_corpus() -> Vec<SimInput> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    if env_bool("RSSP_BENCH_USE_PACKS", true) {
        let packs_dir = manifest_dir.join("tests/data/packs");
        if packs_dir.exists() {
            let max_files = env_usize("RSSP_BENCH_MAX_FILES", DEFAULT_MAX_FILES);
            let max_bytes = env_usize("RSSP_BENCH_MAX_BYTES", DEFAULT_MAX_BYTES);
            let packs = load_pack_corpus(&packs_dir, max_files, max_bytes);
            if !packs.is_empty() {
                return packs;
            }
        }
    }

    let fixtures = load_fixture_corpus(&manifest_dir);
    assert!(
        !fixtures.is_empty(),
        "no benchmark corpus found (packs missing and fixtures unreadable)"
    );
    fixtures
}

fn corpus_bytes(corpus: &[SimInput]) -> u64 {
    corpus.iter().map(|sim| sim.raw.len() as u64).sum()
}

fn corpus_bytes_for(corpus: &[SimInput], indexes: &[usize]) -> u64 {
    indexes
        .iter()
        .map(|&idx| corpus[idx].raw.len() as u64)
        .sum()
}

fn analyzable_indexes(corpus: &[SimInput], options: &rssp::AnalysisOptions) -> Vec<usize> {
    let mut indexes = Vec::with_capacity(corpus.len());
    for (idx, sim) in corpus.iter().enumerate() {
        if rssp::analyze(sim.raw.as_slice(), sim.extension, options).is_ok() {
            indexes.push(idx);
        }
    }
    indexes
}

fn analyze_full_loop(
    corpus: &[SimInput],
    indexes: &[usize],
    options: &rssp::AnalysisOptions,
) -> usize {
    let mut total_steps = 0usize;
    for &idx in indexes {
        let sim = &corpus[idx];
        let summary = rssp::analyze(
            black_box(sim.raw.as_slice()),
            black_box(sim.extension),
            black_box(options),
        )
        .expect("benchmark corpus should analyze");
        total_steps += summary
            .charts
            .iter()
            .map(|chart| chart.stats.total_steps as usize)
            .sum::<usize>();
    }
    total_steps
}

fn parse_only_loop(corpus: &[SimInput]) -> usize {
    let mut chart_count = 0usize;
    for sim in corpus {
        let parsed =
            rssp::parse::extract_sections(black_box(sim.raw.as_slice()), black_box(sim.extension))
                .expect("benchmark corpus should parse");
        chart_count += parsed.notes_list.len();
    }
    chart_count
}

fn bench_throughput(c: &mut Criterion) {
    let corpus = load_corpus();
    let parse_bytes = corpus_bytes(&corpus);
    let sample_size = env_usize("RSSP_BENCH_SAMPLE_SIZE", DEFAULT_SAMPLE_SIZE).clamp(10, 200);
    let measure_secs = env_u64("RSSP_BENCH_MEASURE_SECS", DEFAULT_MEASURE_SECS).max(1);

    let full_options = rssp::AnalysisOptions {
        mono_threshold: 6,
        ..rssp::AnalysisOptions::default()
    };
    let fast_options = rssp::AnalysisOptions {
        mono_threshold: 6,
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..rssp::AnalysisOptions::default()
    };
    let analyze_indexes = analyzable_indexes(&corpus, &fast_options);
    assert!(
        !analyze_indexes.is_empty(),
        "benchmark corpus has no analyzable charts"
    );
    let analyze_bytes = corpus_bytes_for(&corpus, &analyze_indexes);

    let mut group = c.benchmark_group("throughput");
    group.sample_size(sample_size);
    group.measurement_time(Duration::from_secs(measure_secs));
    group.warm_up_time(Duration::from_secs(2));

    group.throughput(Throughput::Bytes(parse_bytes));
    group.bench_function("parse_only", |b| {
        b.iter(|| black_box(parse_only_loop(&corpus)));
    });

    group.throughput(Throughput::Bytes(analyze_bytes));
    group.bench_function("analyze_full", |b| {
        b.iter(|| black_box(analyze_full_loop(&corpus, &analyze_indexes, &full_options)));
    });

    group.throughput(Throughput::Bytes(analyze_bytes));
    group.bench_function("analyze_fast", |b| {
        b.iter(|| black_box(analyze_full_loop(&corpus, &analyze_indexes, &fast_options)));
    });

    group.finish();
}

criterion_group!(benches, bench_throughput);
criterion_main!(benches);
