#![allow(clippy::float_cmp)]

use std::borrow::Cow;

pub const SEGMENT_COUNT: usize = 2_048;
pub const MERGE_INPUT_COUNT: u64 = (SEGMENT_COUNT * 3) as u64;

pub struct TimingMergeFixture {
    pub bpms: Vec<(f32, f32)>,
    pub stops: Vec<(f32, f32)>,
    pub delays: Vec<(f32, f32)>,
    pub warps: Vec<(f32, f32)>,
}

impl TimingMergeFixture {
    pub fn new() -> Self {
        let bpms = (0u16..512)
            .map(|index| (f32::from(index) * 16.0, 90.0 + f32::from(index % 7) * 30.0))
            .collect();
        let stops = (0u16..SEGMENT_COUNT as u16)
            .map(|index| (f32::from(index) * 4.0 + 1.0, 0.125))
            .collect();
        let delays = (0u16..SEGMENT_COUNT as u16)
            .map(|index| (f32::from(index) * 4.0 + 2.0, 0.0625))
            .collect();
        let warps = (0u16..SEGMENT_COUNT as u16)
            .map(|index| {
                let offset = [1.0, 2.0, 3.0][usize::from(index % 3)];
                (
                    f32::from(index) * 4.0 + offset,
                    f32::from(index % 8 + 1) * 0.25,
                )
            })
            .collect();
        Self {
            bpms,
            stops,
            delays,
            warps,
        }
    }
}

pub fn legacy_convert<'a>(
    bpms: &[(f32, f32)],
    stops: &'a [(f32, f32)],
    delays: &[(f32, f32)],
    warps: &[(f32, f32)],
) -> Cow<'a, [(f32, f32)]> {
    if delays.is_empty() && warps.is_empty() {
        return Cow::Borrowed(stops);
    }

    let mut warp_stops = Vec::new();
    let mut bpm_index = 0;
    for (warp_beat, warp_value) in warps {
        while bpm_index + 1 < bpms.len() && bpms[bpm_index + 1].0 <= *warp_beat {
            bpm_index += 1;
        }
        let seconds_per_beat = 60.0 / bpms[bpm_index].1;
        warp_stops.push((*warp_beat, -(seconds_per_beat * warp_value)));
    }

    let mut sm_stops = Vec::with_capacity(stops.len() + delays.len() + warp_stops.len());
    let mut stops_index = 0;
    let mut delays_index = 0;
    let mut warp_stops_index = 0;
    while stops_index < stops.len()
        || delays_index < delays.len()
        || warp_stops_index < warp_stops.len()
    {
        let mut key = None;
        if stops_index < stops.len() {
            key = Some(stops[stops_index].0);
        }
        if delays_index < delays.len() && key.is_none_or(|key| delays[delays_index].0 < key) {
            key = Some(delays[delays_index].0);
        }
        if warp_stops_index < warp_stops.len()
            && key.is_none_or(|key| warp_stops[warp_stops_index].0 < key)
        {
            key = Some(warp_stops[warp_stops_index].0);
        }

        let Some(key) = key else { break };
        let mut value = 0.0;
        if stops_index < stops.len() && stops[stops_index].0 == key {
            value += stops[stops_index].1;
            stops_index += 1;
        }
        if delays_index < delays.len() && delays[delays_index].0 == key {
            value += delays[delays_index].1;
            delays_index += 1;
        }
        if warp_stops_index < warp_stops.len() && warp_stops[warp_stops_index].0 == key {
            value += warp_stops[warp_stops_index].1;
            warp_stops_index += 1;
        }
        if value != 0.0 {
            sm_stops.push((key, value));
        }
    }
    Cow::Owned(sm_stops)
}

pub fn assert_behavior() {
    let fixture = TimingMergeFixture::new();
    assert_eq!(
        rssp::timing::convert_warps_and_delays_to_sm_stops(
            &fixture.bpms,
            &fixture.stops,
            &fixture.delays,
            &fixture.warps,
        ),
        legacy_convert(
            &fixture.bpms,
            &fixture.stops,
            &fixture.delays,
            &fixture.warps,
        )
    );

    let bpms = [(0.0, 120.0), (8.0, 240.0)];
    let stops = [(4.0, 0.5), (12.0, 0.25)];
    let delays = [(4.0, 0.25), (10.0, 0.5)];
    let warps = [(4.0, 2.0), (8.0, 4.0), (12.0, 1.0)];
    let current =
        rssp::timing::convert_warps_and_delays_to_sm_stops(&bpms, &stops, &delays, &warps);
    assert_eq!(current.as_ref(), [(4.0, -0.25), (8.0, -1.0), (10.0, 0.5)]);
    assert_eq!(current, legacy_convert(&bpms, &stops, &delays, &warps));

    let duplicate_warps = [(2.0, 1.0), (2.0, 2.0), (6.0, 0.0)];
    assert_eq!(
        rssp::timing::convert_warps_and_delays_to_sm_stops(&bpms, &stops, &[], &duplicate_warps,),
        legacy_convert(&bpms, &stops, &[], &duplicate_warps)
    );

    let borrowed = rssp::timing::convert_warps_and_delays_to_sm_stops(&[], &stops, &[], &[]);
    assert!(matches!(borrowed, Cow::Borrowed(value) if value == stops));
    assert!(matches!(
        legacy_convert(&[], &stops, &[], &[]),
        Cow::Borrowed(value) if value == stops
    ));
}
