pub const ENTRY_COUNT: usize = 4_096;
pub const OUTPUT_COUNT: usize = ENTRY_COUNT / 2;

pub fn fixture() -> Vec<rssp::timing::Segment> {
    (0..ENTRY_COUNT)
        .map(|index| {
            let row = (index * 2_053) % OUTPUT_COUNT;
            rssp::timing::Segment {
                beat: row as f64 / 48.0,
                value: index as f64 + 0.25,
            }
        })
        .collect()
}

pub fn tidy(segments: Vec<rssp::timing::Segment>, legacy: bool) -> Vec<rssp::timing::Segment> {
    rssp::timing::tidy_row_segments_for_bench(segments, legacy)
}

fn assert_same(input: &[rssp::timing::Segment]) {
    let legacy = tidy(input.to_vec(), true);
    let current = tidy(input.to_vec(), false);
    assert_eq!(current.len(), legacy.len());
    for (current, legacy) in current.iter().zip(&legacy) {
        assert_eq!(current.beat.to_bits(), legacy.beat.to_bits());
        assert_eq!(current.value.to_bits(), legacy.value.to_bits());
    }
}

pub fn assert_behavior(input: &[rssp::timing::Segment]) {
    assert_same(input);
    assert_eq!(tidy(input.to_vec(), false).len(), OUTPUT_COUNT);
    assert_same(&[]);
    assert_same(&[
        rssp::timing::Segment {
            beat: 2.0,
            value: 1.0,
        },
        rssp::timing::Segment {
            beat: 0.019,
            value: 2.0,
        },
        rssp::timing::Segment {
            beat: 0.021,
            value: 3.0,
        },
        rssp::timing::Segment {
            beat: -1.0,
            value: 4.0,
        },
    ]);
}
