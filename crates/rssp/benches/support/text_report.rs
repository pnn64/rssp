pub fn write(
    summary: &rssp::report::SimfileSummary,
    output: &mut Vec<u8>,
    full: bool,
    legacy: bool,
) -> usize {
    output.clear();
    rssp::profile::write_text_report(summary, output, full, legacy)
        .expect("text report should write");
    output.len()
}

fn assert_pair(summary: &rssp::report::SimfileSummary) {
    for (full, mode) in [
        (false, rssp::report::OutputMode::Pretty),
        (true, rssp::report::OutputMode::Full),
    ] {
        let mut legacy = Vec::new();
        let mut current = Vec::new();
        let mut production = Vec::new();
        write(summary, &mut legacy, full, true);
        write(summary, &mut current, full, false);
        rssp::report::write_reports(summary, mode, &mut production)
            .expect("production text report should write");
        assert_eq!(
            current, legacy,
            "legacy/current output differs for {mode:?}"
        );
        assert_eq!(production, legacy, "production output differs for {mode:?}");
    }
}

pub fn assert_behavior(summary: &rssp::report::SimfileSummary) {
    assert_pair(summary);

    let mut edge = summary.clone();
    edge.charts.truncate(1);
    edge.title_str = "Café 二".to_string();
    edge.subtitle_str.clear();
    edge.charts[0].difficulty_str = "難易度".to_string();
    edge.charts[0].step_artist_str = "long UTF-8 artist — ".repeat(8);
    for seconds in [i32::MIN, -3_661, -61, -1, 0, 59, 60, 3_661, i32::MAX] {
        edge.total_length = seconds;
        assert_pair(&edge);
    }
}
