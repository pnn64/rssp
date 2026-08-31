pub const SINGLE_ROW_COUNT: usize = 2_048;
pub const DOUBLE_ROW_COUNT: usize = 512;
pub const NOTE_DATA_ROW_COUNT: usize = 2_048;

pub const SINGLE_MASKS: &[u8] = &[
    0b0001, 0b0100, 0b1000, 0b0010, 0b0011, 0b1100, 0b0101, 0b1010, 0b0010, 0b1000, 0b0100, 0b0001,
    0b1001, 0b0110,
];

pub const DOUBLE_MASKS: &[u8] = &[
    0b0000_0001,
    0b0001_0000,
    0b1000_0000,
    0b0000_1000,
    0b0001_0001,
    0b1000_1000,
    0b0010_0100,
    0b0100_0010,
    0b0000_0011,
    0b1100_0000,
];

pub fn rows<const LANES: usize>(count: usize, masks: &[u8]) -> Vec<[u8; LANES]> {
    (0..count)
        .map(|idx| {
            let mask = masks[idx % masks.len()];
            std::array::from_fn(|lane| {
                if mask & (1u8 << lane) != 0 {
                    b'1'
                } else {
                    b'0'
                }
            })
        })
        .collect()
}

pub fn hold_rows<const LANES: usize>(count: usize, masks: &[u8]) -> Vec<[u8; LANES]> {
    let mut rows = Vec::with_capacity(count);
    let mut hold_lane = usize::MAX;
    for idx in 0..count {
        let mask = masks[idx % masks.len()];
        let mut row = std::array::from_fn(|lane| {
            if mask & (1u8 << lane) != 0 {
                b'1'
            } else {
                b'0'
            }
        });
        match idx % 16 {
            0 => {
                hold_lane = (idx / 16) % LANES;
                row[hold_lane] = b'2';
            }
            8 => {
                debug_assert_ne!(hold_lane, usize::MAX);
                row[hold_lane] = b'3';
                hold_lane = usize::MAX;
            }
            _ if hold_lane != usize::MAX => row[hold_lane] = b'0',
            _ => {}
        }
        rows.push(row);
    }
    rows
}

pub fn beats(count: usize) -> Vec<f32> {
    (0..count).map(|idx| idx as f32 * 0.25).collect()
}

pub fn note_data() -> Vec<u8> {
    const MEASURE_ROWS: usize = 16;
    let rows = hold_rows::<4>(NOTE_DATA_ROW_COUNT, SINGLE_MASKS);
    let mut data = Vec::with_capacity(NOTE_DATA_ROW_COUNT * 5 + NOTE_DATA_ROW_COUNT / 16);
    for (index, row) in rows.iter().enumerate() {
        data.extend_from_slice(row);
        data.push(b'\n');
        if (index + 1) % MEASURE_ROWS == 0 && index + 1 != rows.len() {
            data.push(b',');
        }
    }
    data
}

pub fn assert_note_data_behavior(data: &[u8]) {
    let materialized = rssp::step_parity::parse_notes_for_bench(data, 4, false);
    let fused = rssp::step_parity::parse_notes_for_bench(data, 4, true);
    assert_eq!(fused, materialized);
    assert_eq!(
        rssp::step_parity::analyze_note_data_for_bench(data, 4, true),
        rssp::step_parity::analyze_note_data_for_bench(data, 4, false)
    );
}

pub fn timing() -> rssp::timing::TimingData {
    rssp::timing::timing_data_from_chart_data(
        0.0,
        0.0,
        None,
        "0=150",
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
        None,
        "",
        rssp::timing::TimingFormat::Ssc,
        true,
    )
}
