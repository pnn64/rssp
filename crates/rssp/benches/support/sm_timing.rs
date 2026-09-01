pub const BPM_COUNT: usize = 4_096;
pub const STOP_COUNT: usize = 2_048;
pub const INPUT_COUNT: u64 = (BPM_COUNT + STOP_COUNT) as u64;
pub const WARP_COUNT: usize = 2_048;

pub struct SmTimingFixture {
    pub bpms: Vec<(f64, f64)>,
    pub stops: Vec<rssp::timing::Segment>,
}

impl SmTimingFixture {
    pub fn new() -> Self {
        let bpms = (0..BPM_COUNT)
            .map(|index| (index as f64 * 4.0, 60.0 + f64::from((index % 300) as u16)))
            .collect();
        let stops = (0..STOP_COUNT)
            .map(|index| rssp::timing::Segment {
                beat: index as f64 * 8.0 + 2.0,
                value: 0.125,
            })
            .collect();
        Self { bpms, stops }
    }
}

pub fn extra_warps() -> Vec<rssp::timing::Segment> {
    (0..WARP_COUNT)
        .map(|index| rssp::timing::Segment {
            beat: index as f64 * 4.0 + 2.0,
            value: 1.0,
        })
        .collect()
}

pub fn warp_inputs() -> (Vec<(f64, f64)>, Vec<rssp::timing::Segment>) {
    let bpms = vec![(0.0, 120.0)];
    let stops = (0..WARP_COUNT)
        .map(|index| rssp::timing::Segment {
            beat: index as f64 * 4.0 + 2.0,
            value: -0.5,
        })
        .collect();
    (bpms, stops)
}

type SmTimingOutput = (
    Vec<(f64, f64)>,
    Vec<rssp::timing::Segment>,
    Vec<rssp::timing::Segment>,
    f64,
);

fn assert_segments_eq(actual: &[rssp::timing::Segment], expected: &[rssp::timing::Segment]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.beat.to_bits(), expected.beat.to_bits());
        assert_eq!(actual.value.to_bits(), expected.value.to_bits());
    }
}

fn assert_output_eq(actual: &SmTimingOutput, expected: &SmTimingOutput) {
    assert_eq!(actual.0.len(), expected.0.len());
    for (actual, expected) in actual.0.iter().zip(&expected.0) {
        assert_eq!(actual.0.to_bits(), expected.0.to_bits());
        assert_eq!(actual.1.to_bits(), expected.1.to_bits());
    }
    assert_eq!(actual.1.len(), expected.1.len());
    for (actual, expected) in actual.1.iter().zip(&expected.1) {
        assert_eq!(actual.beat.to_bits(), expected.beat.to_bits());
        assert_eq!(actual.value.to_bits(), expected.value.to_bits());
    }
    assert_eq!(actual.2.len(), expected.2.len());
    for (actual, expected) in actual.2.iter().zip(&expected.2) {
        assert_eq!(actual.beat.to_bits(), expected.beat.to_bits());
        assert_eq!(actual.value.to_bits(), expected.value.to_bits());
    }
    assert_eq!(actual.3.to_bits(), expected.3.to_bits());
}

fn assert_case(bpms: &[(f64, f64)], stops: &[rssp::timing::Segment]) -> SmTimingOutput {
    let current = rssp::timing::process_sm_timing_for_bench(bpms, stops, false);
    let legacy = rssp::timing::process_sm_timing_for_bench(bpms, stops, true);
    assert_output_eq(&current, &legacy);
    current
}

pub fn assert_behavior() {
    let fixture = SmTimingFixture::new();
    let output = assert_case(&fixture.bpms, &fixture.stops);
    assert_eq!(output.0.len(), BPM_COUNT);
    assert_eq!(output.1.len(), STOP_COUNT);
    assert!(output.2.is_empty());

    let simple = assert_case(&[(0.0, 120.0), (4.0, 180.0), (8.0, 180.0)], &[]);
    assert_eq!(simple.0, [(0.0, 120.0), (4.0, 180.0)]);
    assert!(simple.1.is_empty());
    assert!(simple.2.is_empty());
    assert_eq!(simple.3.to_bits(), 0.0f64.to_bits());

    let edge_bpms = [
        (16.0, 180.0),
        (-4.0, 90.0),
        (0.0, 120.0),
        (4.0, -60.0),
        (8.0, 20_000_000.0),
        (12.0, 150.0),
        (12.0, 240.0),
        (f64::NAN, 100.0),
        (20.0, f64::INFINITY),
        (24.0, 0.0),
    ];
    let edge_stops = [
        rssp::timing::Segment {
            beat: 10.0,
            value: 0.5,
        },
        rssp::timing::Segment {
            beat: -2.0,
            value: 0.25,
        },
        rssp::timing::Segment {
            beat: 6.0,
            value: -0.5,
        },
        rssp::timing::Segment {
            beat: f64::NAN,
            value: 1.0,
        },
        rssp::timing::Segment {
            beat: 14.0,
            value: f64::INFINITY,
        },
    ];
    assert_case(&edge_bpms, &edge_stops);

    let empty = assert_case(&[], &[]);
    assert_eq!(empty.0, [(0.0, 60.0)]);
    assert!(empty.1.is_empty());
    assert!(empty.2.is_empty());

    let extra = extra_warps();
    let copied = rssp::timing::merge_extra_warps_for_bench(Vec::new(), extra.clone(), false);
    let reused = rssp::timing::merge_extra_warps_for_bench(Vec::new(), extra.clone(), true);
    assert_segments_eq(&reused, &copied);
    assert_eq!(reused.len(), WARP_COUNT);

    let explicit = vec![rssp::timing::Segment {
        beat: -4.0,
        value: 2.0,
    }];
    let copied = rssp::timing::merge_extra_warps_for_bench(explicit.clone(), extra.clone(), false);
    let reused = rssp::timing::merge_extra_warps_for_bench(explicit, extra, true);
    assert_segments_eq(&reused, &copied);
    assert_eq!(reused.len(), WARP_COUNT + 1);

    let (bpms, stops) = warp_inputs();
    let copied = rssp::timing::process_sm_warp_merge_for_bench(&bpms, &stops, false);
    let reused = rssp::timing::process_sm_warp_merge_for_bench(&bpms, &stops, true);
    assert_output_eq(&reused, &copied);
    assert_eq!(reused.2.len(), WARP_COUNT);
}
