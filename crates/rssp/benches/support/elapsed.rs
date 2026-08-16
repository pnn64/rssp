pub const BPM_COUNT: usize = 256;
pub const STOP_COUNT: usize = 128;
pub const DELAY_COUNT: usize = 64;
pub const WARP_COUNT: usize = 64;
pub const EVENT_COUNT: u64 = (BPM_COUNT + STOP_COUNT + DELAY_COUNT + WARP_COUNT) as u64;

pub struct ElapsedFixture {
    pub target: f64,
    pub bpms: Vec<(f64, f64)>,
    pub stops: Vec<(f64, f64)>,
    pub delays: Vec<(f64, f64)>,
    pub warps: Vec<(f64, f64)>,
}

impl ElapsedFixture {
    pub fn new() -> Self {
        Self {
            target: 2_048.0,
            bpms: (0..BPM_COUNT)
                .map(|index| (index as f64 * 8.0, 90.0 + (index % 181) as f64))
                .collect(),
            stops: (0..STOP_COUNT)
                .map(|index| (index as f64 * 16.0 + 2.0, 0.0625))
                .collect(),
            delays: (0..DELAY_COUNT)
                .map(|index| (index as f64 * 32.0 + 4.0, 0.03125))
                .collect(),
            warps: (0..WARP_COUNT)
                .map(|index| (index as f64 * 32.0 + 6.0, 1.0))
                .collect(),
        }
    }
}

fn assert_case(
    target: f64,
    bpms: &[(f64, f64)],
    stops: &[(f64, f64)],
    delays: &[(f64, f64)],
    warps: &[(f64, f64)],
) {
    let legacy = rssp::bpm::get_elapsed_time_for_bench(target, bpms, stops, delays, warps, true);
    let merged = rssp::bpm::get_elapsed_time_for_bench(target, bpms, stops, delays, warps, false);
    assert_eq!(merged.to_bits(), legacy.to_bits());
}

pub fn assert_behavior(fixture: &ElapsedFixture) {
    assert_case(
        fixture.target,
        &fixture.bpms,
        &fixture.stops,
        &fixture.delays,
        &fixture.warps,
    );
    let bpms = [(-4.0, 90.0), (0.0, 120.0), (8.0, 180.0), (8.0, 240.0)];
    let stops = [(-2.0, 0.25), (4.0, 0.5), (8.0, 0.125), (8.0, 0.25)];
    let delays = [(0.0, 0.125), (8.0, 0.0625), (12.0, -0.25)];
    let warps = [(2.0, 4.0), (8.0, 2.0), (8.0, 8.0)];
    for target in [-8.0, 0.0, 3.0, 8.0, 9.0, 16.0, 32.0] {
        assert_case(target, &bpms, &stops, &delays, &warps);
    }
    assert_case(16.0, &[], &stops, &delays, &warps);
    assert_case(16.0, &[(0.0, 0.0), (8.0, -120.0)], &stops, &[], &[]);

    let unsorted_bpms = [(8.0, 180.0), (0.0, 120.0), (4.0, 150.0)];
    let unsorted_stops = [(12.0, 0.25), (2.0, 0.5)];
    assert_case(16.0, &unsorted_bpms, &unsorted_stops, &[], &[]);
    assert_case(4.0, &unsorted_bpms, &[], &[], &[]);
    assert_case(16.0, &[(f64::NAN, 120.0)], &[(2.0, 0.5)], &[], &[]);
    assert_case(16.0, &[(f64::INFINITY, 120.0)], &[(2.0, 0.5)], &[], &[]);
}
