use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/watch_yo_step.ssc");
const EXTENSION: &str = "ssc";

#[derive(Clone)]
struct DurationChartInput {
    chart_data: Vec<u8>,
    lanes: usize,
    chart_offset: Option<f64>,
    chart_bpms: Option<String>,
    chart_stops: Option<String>,
    chart_delays: Option<String>,
    chart_warps: Option<String>,
    chart_speeds: Option<String>,
    chart_scrolls: Option<String>,
    chart_fakes: Option<String>,
}

#[derive(Clone)]
struct DurationGlobals {
    bpms_raw: String,
    stops_raw: String,
    delays_raw: String,
    warps_raw: String,
    speeds_raw: String,
    scrolls_raw: String,
    fakes_raw: String,
    song_offset: f64,
    timing_format: rssp::timing::TimingFormat,
    allow_steps_timing: bool,
}

#[derive(Clone)]
struct MinimizedChartInput {
    minimized_chart: Vec<u8>,
    lanes: usize,
}

#[derive(Clone)]
struct TimingEvalInput {
    target_beat: f64,
    chart_offset: f64,
    chart_has_timing: bool,
    chart_bpms: Option<String>,
    chart_stops: Option<String>,
    chart_delays: Option<String>,
    chart_warps: Option<String>,
    chart_speeds: Option<String>,
    chart_scrolls: Option<String>,
    chart_fakes: Option<String>,
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

fn chart_offset_seconds(tag: Option<&[u8]>) -> Option<f64> {
    tag.map(|bytes| rssp::parse::parse_offset_seconds(Some(bytes)))
}

fn build_duration_inputs() -> (Vec<DurationChartInput>, DurationGlobals) {
    let parsed =
        rssp::parse::extract_sections(FIXTURE.as_bytes(), EXTENSION).expect("fixture should parse");
    let timing_format = rssp::timing::TimingFormat::from_extension(EXTENSION);
    let ssc_version = rssp::parse::parse_version(parsed.version, timing_format);
    let allow_steps_timing = rssp::timing::steps_timing_allowed(ssc_version, timing_format);

    let globals = DurationGlobals {
        bpms_raw: clean_tag_bytes(parsed.bpms),
        stops_raw: clean_tag_bytes(parsed.stops),
        delays_raw: clean_tag_bytes(parsed.delays),
        warps_raw: clean_tag_bytes(parsed.warps),
        speeds_raw: clean_tag_bytes(parsed.speeds),
        scrolls_raw: clean_tag_bytes(parsed.scrolls),
        fakes_raw: clean_tag_bytes(parsed.fakes),
        song_offset: rssp::parse::parse_offset_seconds(parsed.offset),
        timing_format,
        allow_steps_timing,
    };

    let charts = parsed
        .notes_list
        .into_iter()
        .filter_map(|entry| {
            let (fields, chart_data) = rssp::parse::split_notes_fields(&entry.notes);
            if fields.len() < 5 {
                return None;
            }

            let step_type = std::str::from_utf8(fields[0]).unwrap_or("").trim();
            if step_type == "lights-cabinet" {
                return None;
            }

            Some(DurationChartInput {
                chart_data: chart_data.to_vec(),
                lanes: rssp::step_type_lanes(step_type),
                chart_offset: chart_offset_seconds(entry.chart_offset.as_deref()),
                chart_bpms: clean_chart_tag(entry.chart_bpms.as_deref()),
                chart_stops: clean_chart_tag(entry.chart_stops.as_deref()),
                chart_delays: clean_chart_tag(entry.chart_delays.as_deref()),
                chart_warps: clean_chart_tag(entry.chart_warps.as_deref()),
                chart_speeds: clean_chart_tag(entry.chart_speeds.as_deref()),
                chart_scrolls: clean_chart_tag(entry.chart_scrolls.as_deref()),
                chart_fakes: clean_chart_tag(entry.chart_fakes.as_deref()),
            })
        })
        .collect();

    (charts, globals)
}

fn build_minimized_inputs(charts: &[DurationChartInput]) -> Vec<MinimizedChartInput> {
    let mut minimized = Vec::with_capacity(charts.len());
    for chart in charts {
        let (mut minimized_chart, _stats, _measure_densities) =
            rssp::stats::minimize_chart_and_count_with_lanes(&chart.chart_data, chart.lanes);
        if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
            minimized_chart.truncate(pos + 1);
        }
        minimized.push(MinimizedChartInput {
            minimized_chart,
            lanes: chart.lanes,
        });
    }
    minimized
}

fn build_timing_eval_inputs(
    charts: &[DurationChartInput],
    globals: &DurationGlobals,
) -> Vec<TimingEvalInput> {
    let minimized = build_minimized_inputs(charts);
    minimized
        .into_iter()
        .zip(charts.iter())
        .map(|(minimized, chart)| {
            let target_beat =
                rssp::bpm::compute_last_beat(&minimized.minimized_chart, minimized.lanes);
            let chart_offset = if globals.allow_steps_timing && chart.chart_offset.is_some() {
                chart.chart_offset.unwrap()
            } else {
                globals.song_offset
            };
            let chart_has_timing = globals.allow_steps_timing
                && (chart.chart_bpms.is_some()
                    || chart.chart_stops.is_some()
                    || chart.chart_delays.is_some()
                    || chart.chart_warps.is_some()
                    || chart.chart_speeds.is_some()
                    || chart.chart_scrolls.is_some()
                    || chart.chart_fakes.is_some());
            TimingEvalInput {
                target_beat,
                chart_offset,
                chart_has_timing,
                chart_bpms: chart.chart_bpms.clone(),
                chart_stops: chart.chart_stops.clone(),
                chart_delays: chart.chart_delays.clone(),
                chart_warps: chart.chart_warps.clone(),
                chart_speeds: chart.chart_speeds.clone(),
                chart_scrolls: chart.chart_scrolls.clone(),
                chart_fakes: chart.chart_fakes.clone(),
            }
        })
        .collect()
}

fn bench_duration_pipeline(c: &mut Criterion) {
    let fixture = FIXTURE.as_bytes();
    let mut group = c.benchmark_group("duration_pipeline");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_chart_durations", |b| {
        b.iter(|| {
            let durations = rssp::compute_chart_durations(
                black_box(fixture),
                black_box(EXTENSION),
                rssp::TimingOffsets::default(),
            )
            .expect("duration compute should succeed");
            black_box(durations);
        })
    });
    group.finish();
}

fn bench_duration_inner(c: &mut Criterion) {
    let (charts, globals) = build_duration_inputs();
    let mut group = c.benchmark_group("duration_inner");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_durations_inner", |b| {
        b.iter(|| {
            let mut durations = Vec::with_capacity(charts.len());
            for chart in &charts {
                let (mut minimized_chart, _stats, _measure_densities) =
                    rssp::stats::minimize_chart_and_count_with_lanes(
                        black_box(&chart.chart_data),
                        black_box(chart.lanes),
                    );
                if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
                    minimized_chart.truncate(pos + 1);
                }

                let target_beat = rssp::bpm::compute_last_beat(&minimized_chart, chart.lanes);
                let chart_offset = if globals.allow_steps_timing && chart.chart_offset.is_some() {
                    chart.chart_offset.unwrap()
                } else {
                    globals.song_offset
                };
                let chart_has_timing = globals.allow_steps_timing
                    && (chart.chart_bpms.is_some()
                        || chart.chart_stops.is_some()
                        || chart.chart_delays.is_some()
                        || chart.chart_warps.is_some()
                        || chart.chart_speeds.is_some()
                        || chart.chart_scrolls.is_some()
                        || chart.chart_fakes.is_some());
                let (
                    timing_bpms_global,
                    timing_stops_global,
                    timing_delays_global,
                    timing_warps_global,
                    timing_speeds_global,
                    timing_scrolls_global,
                    timing_fakes_global,
                ) = if chart_has_timing {
                    ("", "", "", "", "", "", "")
                } else {
                    (
                        globals.bpms_raw.as_str(),
                        globals.stops_raw.as_str(),
                        globals.delays_raw.as_str(),
                        globals.warps_raw.as_str(),
                        globals.speeds_raw.as_str(),
                        globals.scrolls_raw.as_str(),
                        globals.fakes_raw.as_str(),
                    )
                };

                let timing = rssp::timing::TimingData::from_chart_data(
                    chart_offset,
                    0.0,
                    if globals.allow_steps_timing {
                        chart.chart_bpms.as_deref()
                    } else {
                        None
                    },
                    timing_bpms_global,
                    if globals.allow_steps_timing {
                        chart.chart_stops.as_deref()
                    } else {
                        None
                    },
                    timing_stops_global,
                    if globals.allow_steps_timing {
                        chart.chart_delays.as_deref()
                    } else {
                        None
                    },
                    timing_delays_global,
                    if globals.allow_steps_timing {
                        chart.chart_warps.as_deref()
                    } else {
                        None
                    },
                    timing_warps_global,
                    if globals.allow_steps_timing {
                        chart.chart_speeds.as_deref()
                    } else {
                        None
                    },
                    timing_speeds_global,
                    if globals.allow_steps_timing {
                        chart.chart_scrolls.as_deref()
                    } else {
                        None
                    },
                    timing_scrolls_global,
                    if globals.allow_steps_timing {
                        chart.chart_fakes.as_deref()
                    } else {
                        None
                    },
                    timing_fakes_global,
                    globals.timing_format,
                    true,
                );
                let duration = timing.get_time_for_beat(target_beat);
                let duration = rssp::timing::round_millis(duration);
                durations.push(duration);
            }
            black_box(durations);
        })
    });
    group.finish();
}

fn bench_duration_last_beat(c: &mut Criterion) {
    let (charts, _globals) = build_duration_inputs();
    let minimized = build_minimized_inputs(&charts);
    let mut group = c.benchmark_group("duration_last_beat");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_last_beat", |b| {
        b.iter(|| {
            let mut beats = Vec::with_capacity(minimized.len());
            for entry in &minimized {
                let beat = rssp::bpm::compute_last_beat(
                    black_box(&entry.minimized_chart),
                    black_box(entry.lanes),
                );
                beats.push(beat);
            }
            black_box(beats);
        })
    });
    group.finish();
}

fn bench_duration_timing(c: &mut Criterion) {
    let (charts, globals) = build_duration_inputs();
    let timing_inputs = build_timing_eval_inputs(&charts, &globals);
    let mut group = c.benchmark_group("duration_timing");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("timing_data_get_time", |b| {
        b.iter(|| {
            let mut durations = Vec::with_capacity(timing_inputs.len());
            for entry in &timing_inputs {
                let (
                    timing_bpms_global,
                    timing_stops_global,
                    timing_delays_global,
                    timing_warps_global,
                    timing_speeds_global,
                    timing_scrolls_global,
                    timing_fakes_global,
                ) = if entry.chart_has_timing {
                    ("", "", "", "", "", "", "")
                } else {
                    (
                        globals.bpms_raw.as_str(),
                        globals.stops_raw.as_str(),
                        globals.delays_raw.as_str(),
                        globals.warps_raw.as_str(),
                        globals.speeds_raw.as_str(),
                        globals.scrolls_raw.as_str(),
                        globals.fakes_raw.as_str(),
                    )
                };
                let timing = rssp::timing::TimingData::from_chart_data(
                    entry.chart_offset,
                    0.0,
                    if globals.allow_steps_timing {
                        entry.chart_bpms.as_deref()
                    } else {
                        None
                    },
                    timing_bpms_global,
                    if globals.allow_steps_timing {
                        entry.chart_stops.as_deref()
                    } else {
                        None
                    },
                    timing_stops_global,
                    if globals.allow_steps_timing {
                        entry.chart_delays.as_deref()
                    } else {
                        None
                    },
                    timing_delays_global,
                    if globals.allow_steps_timing {
                        entry.chart_warps.as_deref()
                    } else {
                        None
                    },
                    timing_warps_global,
                    if globals.allow_steps_timing {
                        entry.chart_speeds.as_deref()
                    } else {
                        None
                    },
                    timing_speeds_global,
                    if globals.allow_steps_timing {
                        entry.chart_scrolls.as_deref()
                    } else {
                        None
                    },
                    timing_scrolls_global,
                    if globals.allow_steps_timing {
                        entry.chart_fakes.as_deref()
                    } else {
                        None
                    },
                    timing_fakes_global,
                    globals.timing_format,
                    true,
                );
                let duration = timing.get_time_for_beat(entry.target_beat);
                durations.push(rssp::timing::round_millis(duration));
            }
            black_box(durations);
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_duration_pipeline,
    bench_duration_inner,
    bench_duration_last_beat,
    bench_duration_timing
);
criterion_main!(benches);
