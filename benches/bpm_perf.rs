use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/bpm_fixture.ssc");

#[derive(Clone)]
struct ChartTimingInput {
    field_count: u8,
    fields: [&'static [u8]; 5],
    chart_bpms: Option<Vec<u8>>,
    chart_stops: Option<Vec<u8>>,
    chart_delays: Option<Vec<u8>>,
    chart_warps: Option<Vec<u8>>,
    chart_speeds: Option<Vec<u8>>,
    chart_scrolls: Option<Vec<u8>>,
    chart_fakes: Option<Vec<u8>>,
}

#[derive(Clone)]
struct TimingGlobals {
    bpms_raw: String,
    stops_raw: String,
    delays_raw: String,
    warps_raw: String,
    speeds_raw: String,
    scrolls_raw: String,
    fakes_raw: String,
    bpms_norm: String,
    song_offset: f64,
    timing_format: rssp::timing::TimingFormat,
    allow_steps_timing: bool,
}

#[derive(Clone)]
struct ChartTimingTags {
    bpms_raw: Option<String>,
    stops_raw: Option<String>,
    delays_raw: Option<String>,
    warps_raw: Option<String>,
    speeds_raw: Option<String>,
    scrolls_raw: Option<String>,
    fakes_raw: Option<String>,
    bpms_norm: Option<String>,
}

fn normalize_tag_bytes(tag: Option<&[u8]>) -> String {
    tag.and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(rssp::bpm::normalize_float_digits)
        .unwrap_or_default()
}

fn clean_tag_bytes(tag: Option<&[u8]>) -> String {
    tag.and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(rssp::bpm::clean_timing_map)
        .unwrap_or_default()
}

fn clean_chart_tag(tag: Option<&[u8]>) -> Option<String> {
    tag.and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(rssp::bpm::clean_timing_map)
        .filter(|s| !s.is_empty())
}

fn normalize_chart_tag(tag: Option<&[u8]>) -> Option<String> {
    tag.and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(rssp::bpm::normalize_float_digits)
        .filter(|s| !s.is_empty())
}

fn chart_timing_tags(entry: &ChartTimingInput) -> ChartTimingTags {
    ChartTimingTags {
        bpms_raw: clean_chart_tag(entry.chart_bpms.as_deref()),
        stops_raw: clean_chart_tag(entry.chart_stops.as_deref()),
        delays_raw: clean_chart_tag(entry.chart_delays.as_deref()),
        warps_raw: clean_chart_tag(entry.chart_warps.as_deref()),
        speeds_raw: clean_chart_tag(entry.chart_speeds.as_deref()),
        scrolls_raw: clean_chart_tag(entry.chart_scrolls.as_deref()),
        fakes_raw: clean_chart_tag(entry.chart_fakes.as_deref()),
        bpms_norm: normalize_chart_tag(entry.chart_bpms.as_deref()),
    }
}

fn build_timing_inputs() -> (Vec<ChartTimingInput>, TimingGlobals) {
    let parsed =
        rssp::parse::extract_sections(FIXTURE.as_bytes(), "ssc").expect("fixture should parse");
    let timing_format = rssp::timing::timing_format_from_ext("ssc");
    let ssc_version = rssp::parse::parse_version(parsed.version, timing_format);
    let allow_steps_timing = rssp::timing::steps_timing_allowed(ssc_version, timing_format);

    let globals = TimingGlobals {
        bpms_raw: clean_tag_bytes(parsed.bpms),
        stops_raw: clean_tag_bytes(parsed.stops),
        delays_raw: clean_tag_bytes(parsed.delays),
        warps_raw: clean_tag_bytes(parsed.warps),
        speeds_raw: clean_tag_bytes(parsed.speeds),
        scrolls_raw: clean_tag_bytes(parsed.scrolls),
        fakes_raw: clean_tag_bytes(parsed.fakes),
        bpms_norm: normalize_tag_bytes(parsed.bpms),
        song_offset: rssp::parse::parse_offset_seconds(parsed.offset),
        timing_format,
        allow_steps_timing,
    };

    let charts = parsed
        .notes_list
        .into_iter()
        .map(|entry| ChartTimingInput {
            field_count: entry.field_count,
            fields: entry.fields,
            chart_bpms: entry.chart_bpms.map(std::borrow::Cow::into_owned),
            chart_stops: entry.chart_stops.map(std::borrow::Cow::into_owned),
            chart_delays: entry.chart_delays.map(std::borrow::Cow::into_owned),
            chart_warps: entry.chart_warps.map(std::borrow::Cow::into_owned),
            chart_speeds: entry.chart_speeds.map(std::borrow::Cow::into_owned),
            chart_scrolls: entry.chart_scrolls.map(std::borrow::Cow::into_owned),
            chart_fakes: entry.chart_fakes.map(std::borrow::Cow::into_owned),
        })
        .collect();

    (charts, globals)
}

fn bench_bpm_pipeline(c: &mut Criterion) {
    let fixture = FIXTURE.as_bytes();
    let mut group = c.benchmark_group("bpm_pipeline");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("chart_bpm_snapshots", |b| {
        b.iter(|| {
            let snapshots = rssp::bpm::chart_bpm_snapshots(black_box(fixture), black_box("ssc"))
                .expect("bpm snapshots should succeed");
            black_box(snapshots);
        });
    });
    group.finish();
}

fn bench_bpm_inner(c: &mut Criterion) {
    let (charts, globals) = build_timing_inputs();
    let mut group = c.benchmark_group("bpm_inner");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("timing_data_and_format", |b| {
        b.iter(|| {
            let mut outputs = Vec::with_capacity(charts.len());
            for entry in &charts {
                if entry.field_count < 4 {
                    continue;
                }

                let step_type = std::str::from_utf8(entry.fields[0]).unwrap_or("").trim();
                if step_type == "lights-cabinet" {
                    continue;
                }

                let tags = chart_timing_tags(entry);
                let hash_bpms = tags
                    .bpms_norm
                    .clone()
                    .unwrap_or_else(|| globals.bpms_norm.clone());

                let timing = rssp::timing::timing_data_from_chart_data(
                    globals.song_offset,
                    0.0,
                    if globals.allow_steps_timing {
                        tags.bpms_raw.as_deref()
                    } else {
                        None
                    },
                    &globals.bpms_raw,
                    if globals.allow_steps_timing {
                        tags.stops_raw.as_deref()
                    } else {
                        None
                    },
                    &globals.stops_raw,
                    if globals.allow_steps_timing {
                        tags.delays_raw.as_deref()
                    } else {
                        None
                    },
                    &globals.delays_raw,
                    if globals.allow_steps_timing {
                        tags.warps_raw.as_deref()
                    } else {
                        None
                    },
                    &globals.warps_raw,
                    if globals.allow_steps_timing {
                        tags.speeds_raw.as_deref()
                    } else {
                        None
                    },
                    &globals.speeds_raw,
                    if globals.allow_steps_timing {
                        tags.scrolls_raw.as_deref()
                    } else {
                        None
                    },
                    &globals.scrolls_raw,
                    if globals.allow_steps_timing {
                        tags.fakes_raw.as_deref()
                    } else {
                        None
                    },
                    &globals.fakes_raw,
                    globals.timing_format,
                    true,
                );

                let bpms_formatted =
                    rssp::timing::format_bpm_segments_like_itg(&rssp::timing::bpm_segments(&timing));
                outputs.push((hash_bpms, bpms_formatted));
            }
            black_box(outputs);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_bpm_pipeline, bench_bpm_inner);
criterion_main!(benches);
