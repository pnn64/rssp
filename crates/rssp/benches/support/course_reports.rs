pub fn summary(length: usize) -> crate::report::CourseSummary {
    let mut simfile = crate::analyze(
        b"#VERSION:0.83;#BPMS:0=120,4=150;#STOPS:2=0.5;#LABELS:0=Start,4=End;#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:8;#NOTES:1000\n0100\n0010\n0001,\n2000\n0000\n3000\n1000;",
        "ssc", &crate::AnalysisOptions::default()).expect("valid course chart");
    crate::report::CourseSummary {
        course: "a".repeat(length),
        course_difficulty: "Medium".into(),
        step_type: "dance-single".into(),
        total_length: 42,
        entries: Vec::new(),
        chart: simfile.charts.remove(0),
        sha1_hashes: vec!["0123456789abcdef".into()],
        bpm_neutral_sha1_hashes: vec!["fedcba9876543210".into()],
        pattern_counts_enabled: true,
        tech_counts_enabled: true,
        total_elapsed: std::time::Duration::ZERO,
    }
}
