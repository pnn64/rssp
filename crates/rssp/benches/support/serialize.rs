use std::sync::Arc;

use super::report_timing_bench;

pub struct SerializeFixture {
    pub summary: rssp::SimfileSummary,
    pub output_len: usize,
}

pub fn write(summary: &rssp::SimfileSummary, output: &mut Vec<u8>, legacy: bool) -> usize {
    rssp::serialize::profile_serialize_simfile(summary, "ssc", output, legacy)
        .expect("benchmark summary should serialize")
}

fn assert_same(summary: &rssp::SimfileSummary, extension: &str) -> usize {
    let mut legacy = Vec::new();
    let legacy_len =
        rssp::serialize::profile_serialize_simfile(summary, extension, &mut legacy, true)
            .expect("legacy benchmark summary should serialize");
    let mut current = Vec::new();
    let current_len =
        rssp::serialize::profile_serialize_simfile(summary, extension, &mut current, false)
            .expect("benchmark summary should serialize");
    assert_eq!(current_len, legacy_len);
    assert_eq!(current_len, current.len());
    assert_eq!(current, legacy);

    let mut public = Vec::new();
    let public_len = rssp::serialize::serialize_simfile(summary, extension, &mut public)
        .expect("public serializer should succeed");
    assert_eq!(public_len, current_len);
    assert_eq!(public, current);
    current_len
}

impl SerializeFixture {
    pub fn new() -> Self {
        let input = report_timing_bench::fixture();
        let summary = rssp::analyze(input.as_bytes(), "ssc", &report_timing_bench::options())
            .expect("serialization benchmark fixture should analyze");
        let output_len = assert_same(&summary, "ssc");
        Self {
            summary,
            output_len,
        }
    }
}

pub fn assert_behavior(fixture: &SerializeFixture) {
    assert_same(&fixture.summary, "ssc");
    assert_same(&fixture.summary, "sm");

    let mut edge = fixture.summary.clone();
    edge.ssc_version = -12.345;
    edge.offset = -0.0;
    edge.sample_start = f64::NAN;
    edge.sample_length = f64::INFINITY;
    edge.last_second_hint = Some(f64::NEG_INFINITY);
    let timing = Arc::make_mut(&mut edge.global_timing_segments);
    timing.bpms = vec![
        (-0.0, f32::NAN),
        (f32::INFINITY, f32::NEG_INFINITY),
        (f32::MAX, f32::MIN_POSITIVE),
    ];
    timing.speeds = vec![
        (
            -0.0,
            f32::NAN,
            f32::INFINITY,
            rssp::timing::SpeedUnit::Seconds,
        ),
        (
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN_POSITIVE,
            rssp::timing::SpeedUnit::Beats,
        ),
    ];
    edge.charts[0].cached_radar_values = Some([f32::NAN; rssp::stats::RADAR_CATEGORY_COUNT]);
    assert_same(&edge, "ssc");
    assert_same(&edge, "sm");
}
