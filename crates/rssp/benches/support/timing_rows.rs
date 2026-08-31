pub const SEGMENT_COUNT: usize = 64;
pub const INPUT_COUNT: u64 = (SEGMENT_COUNT * 4) as u64;

pub struct TimingRowsFixture {
    pub stops: Vec<rssp::timing::Segment>,
    pub delays: Vec<rssp::timing::Segment>,
    pub warps: Vec<rssp::timing::Segment>,
    pub fakes: Vec<rssp::timing::Segment>,
}

impl TimingRowsFixture {
    pub fn new() -> Self {
        let make = |offset: f64| {
            (0..SEGMENT_COUNT)
                .map(|index| rssp::timing::Segment {
                    beat: index as f64 * 4.0 + offset,
                    value: 0.25,
                })
                .collect()
        };
        Self {
            stops: make(0.0),
            delays: make(1.0),
            warps: make(2.0),
            fakes: make(3.0),
        }
    }
}

fn split_rows(output: rssp::timing::SegmentRowsForBench) -> [Vec<i32>; 4] {
    match output {
        rssp::timing::SegmentRowsForBench::Split(rows) => rows,
        rssp::timing::SegmentRowsForBench::Packed { rows, offsets } => {
            std::array::from_fn(|index| rows[offsets[index]..offsets[index + 1]].to_vec())
        }
    }
}

pub fn assert_behavior(fixture: &TimingRowsFixture) {
    let build = |packed| {
        rssp::timing::build_segment_rows_for_bench(
            &fixture.stops,
            &fixture.delays,
            &fixture.warps,
            &fixture.fakes,
            packed,
        )
    };
    assert_eq!(split_rows(build(false)), split_rows(build(true)));
}

#[allow(dead_code)]
pub fn row_count(output: &rssp::timing::SegmentRowsForBench) -> usize {
    match output {
        rssp::timing::SegmentRowsForBench::Split(rows) => rows.iter().map(Vec::len).sum(),
        rssp::timing::SegmentRowsForBench::Packed { rows, .. } => rows.len(),
    }
}
