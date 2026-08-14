pub const PATTERN_COUNT: usize = 128;

fn patterns() -> Vec<String> {
    const STEP: [u8; 4] = *b"LDUR";
    (0..PATTERN_COUNT)
        .map(|mut index| {
            let mut pattern = String::with_capacity(8);
            for _ in 0..8 {
                pattern.push(char::from(STEP[index & 3]));
                index >>= 2;
            }
            pattern
        })
        .collect()
}

pub fn summary() -> rssp::SimfileSummary {
    const FIXTURE: &[u8] = include_bytes!("../fixtures/hash_fixture.ssc");
    let options = rssp::AnalysisOptions {
        custom_patterns: patterns(),
        compute_tech_counts: false,
        ..rssp::AnalysisOptions::default()
    };
    rssp::analyze(FIXTURE, "ssc", &options).expect("custom pattern report fixture should analyze")
}
