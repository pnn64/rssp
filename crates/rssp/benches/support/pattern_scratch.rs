pub const ROW_COUNT: usize = 16_384;

pub fn rows() -> Vec<[u8; 4]> {
    (0..ROW_COUNT)
        .map(|index| {
            let mut row = [b'0'; 4];
            row[(index * 5 + index / 7) & 3] = b'1';
            row
        })
        .collect()
}

pub fn assert_behavior(
    rows: &[[u8; 4]],
    threshold: usize,
    compiled: &rssp::patterns::CompiledCustomPatterns,
) {
    let expected = rssp::patterns::analyze_patterns_from_rows(rows, threshold, compiled);
    let mut scratch = Vec::new();
    let actual = rssp::patterns::analyze_patterns_from_rows_with_scratch(
        rows,
        threshold,
        compiled,
        &mut scratch,
    );
    assert_eq!(actual, expected, "reused pattern counts changed analysis");

    let repeated = rssp::patterns::analyze_patterns_from_rows_with_scratch(
        rows,
        threshold,
        compiled,
        &mut scratch,
    );
    assert_eq!(repeated, expected, "dirty pattern scratch changed analysis");
}
