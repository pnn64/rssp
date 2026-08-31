pub const MEASURE_COUNT: usize = 1_024;
const ROW_COUNTS: [usize; 8] = [4, 8, 12, 16, 24, 32, 48, 64];
pub const ROW_COUNT: usize =
    MEASURE_COUNT / ROW_COUNTS.len() * (4 + 8 + 12 + 16 + 24 + 32 + 48 + 64);

pub fn fixture() -> Vec<u8> {
    let mut data = Vec::with_capacity(ROW_COUNT * 5 + MEASURE_COUNT * 2);
    for measure in 0..MEASURE_COUNT {
        for row in 0..ROW_COUNTS[measure % ROW_COUNTS.len()] {
            data.extend_from_slice(match row & 3 {
                0 => b"1000\n",
                1 => b"0100\n",
                2 => b"0010\n",
                _ => b"0001\n",
            });
        }
        if measure + 1 != MEASURE_COUNT {
            data.extend_from_slice(b",\n");
        }
    }
    data
}

pub fn compute(data: &[u8], legacy: bool) -> Vec<f32> {
    rssp::timing::compute_row_to_beat_for_bench(data, legacy)
}

pub fn assert_behavior(data: &[u8]) {
    let legacy = compute(data, true);
    let current = compute(data, false);
    assert_eq!(legacy.len(), ROW_COUNT);
    assert_eq!(current.len(), legacy.len());
    for (actual, expected) in current.iter().zip(legacy) {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
}
