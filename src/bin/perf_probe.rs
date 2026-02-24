use std::fs;
use std::hint::black_box;
use std::path::PathBuf;

const FIXTURES: [(&str, &str); 4] = [
    ("benches/fixtures/camellia_mix.ssc", "ssc"),
    ("benches/fixtures/hash_fixture.ssc", "ssc"),
    ("benches/fixtures/200000_step_challenge.sm", "sm"),
    ("benches/fixtures/24h_of_100bpm_stream.sm", "sm"),
];

struct SimInput {
    ext: &'static str,
    raw: Vec<u8>,
}

#[derive(Clone, Copy)]
enum Mode {
    ParseOnly,
    AnalyzeFull,
    AnalyzeFast,
}

fn parse_mode(raw: &str) -> Option<Mode> {
    if raw.eq_ignore_ascii_case("parse_only") {
        Some(Mode::ParseOnly)
    } else if raw.eq_ignore_ascii_case("analyze_full") {
        Some(Mode::AnalyzeFull)
    } else if raw.eq_ignore_ascii_case("analyze_fast") {
        Some(Mode::AnalyzeFast)
    } else {
        None
    }
}

fn parse_usize(raw: Option<&str>, default: usize) -> usize {
    raw.and_then(|v| v.parse::<usize>().ok()).unwrap_or(default)
}

fn arg_value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.windows(2)
        .find_map(|pair| (pair[0] == key).then_some(pair[1].as_str()))
}

fn parse_args() -> (Mode, usize) {
    let args: Vec<String> = std::env::args().collect();
    let mode = parse_mode(arg_value(&args, "--mode").unwrap_or("analyze_fast"))
        .unwrap_or(Mode::AnalyzeFast);
    let iters = parse_usize(arg_value(&args, "--iters"), 256).max(1);
    (mode, iters)
}

fn load_fixture_corpus() -> Vec<SimInput> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut corpus = Vec::with_capacity(FIXTURES.len());

    for (rel, ext) in FIXTURES {
        let path = root.join(rel);
        let Ok(raw) = fs::read(path) else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        corpus.push(SimInput { ext, raw });
    }

    assert!(
        !corpus.is_empty(),
        "no fixture corpus found; run from repository checkout"
    );
    corpus
}

fn parse_only_loop(corpus: &[SimInput]) -> usize {
    let mut total = 0usize;
    for sim in corpus {
        let parsed = rssp::parse::extract_sections(black_box(sim.raw.as_slice()), black_box(sim.ext))
            .expect("fixture parse should succeed");
        total += parsed.notes_list.len();
    }
    total
}

fn analyzable_indexes(corpus: &[SimInput], opts: &rssp::AnalysisOptions) -> Vec<usize> {
    let mut out = Vec::with_capacity(corpus.len());
    for (i, sim) in corpus.iter().enumerate() {
        if rssp::analyze(sim.raw.as_slice(), sim.ext, opts).is_ok() {
            out.push(i);
        }
    }
    out
}

fn analyze_loop(corpus: &[SimInput], idxs: &[usize], opts: &rssp::AnalysisOptions) -> usize {
    let mut total = 0usize;
    for &idx in idxs {
        let sim = &corpus[idx];
        let summary = rssp::analyze(
            black_box(sim.raw.as_slice()),
            black_box(sim.ext),
            black_box(opts),
        )
        .expect("fixture analysis should succeed");
        total += summary
            .charts
            .iter()
            .map(|chart| chart.stats.total_steps as usize)
            .sum::<usize>();
    }
    total
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::ParseOnly => "parse_only",
        Mode::AnalyzeFull => "analyze_full",
        Mode::AnalyzeFast => "analyze_fast",
    }
}

fn corpus_bytes(corpus: &[SimInput]) -> usize {
    corpus.iter().map(|s| s.raw.len()).sum()
}

fn run_iters(
    mode: Mode,
    iters: usize,
    corpus: &[SimInput],
    idxs: &[usize],
    full: &rssp::AnalysisOptions,
    fast: &rssp::AnalysisOptions,
) -> usize {
    let mut checksum = 0usize;
    for _ in 0..iters {
        checksum = checksum.wrapping_add(match mode {
            Mode::ParseOnly => parse_only_loop(corpus),
            Mode::AnalyzeFull => analyze_loop(corpus, idxs, full),
            Mode::AnalyzeFast => analyze_loop(corpus, idxs, fast),
        });
    }
    checksum
}

fn main() {
    let (mode, iters) = parse_args();
    let corpus = load_fixture_corpus();

    let full = rssp::AnalysisOptions {
        mono_threshold: 6,
        ..rssp::AnalysisOptions::default()
    };
    let fast = rssp::AnalysisOptions {
        mono_threshold: 6,
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..rssp::AnalysisOptions::default()
    };
    let idxs = analyzable_indexes(&corpus, &fast);
    assert!(!idxs.is_empty(), "fixture corpus has no analyzable charts");

    let checksum = run_iters(mode, iters, &corpus, &idxs, &full, &fast);
    println!(
        "mode={} iters={} files={} bytes={} analyzable={} checksum={}",
        mode_name(mode),
        iters,
        corpus.len(),
        corpus_bytes(&corpus),
        idxs.len(),
        black_box(checksum)
    );
}
