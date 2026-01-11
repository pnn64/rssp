use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/watch_yo_step.ssc");
const EXTENSION: &str = "ssc";

#[derive(Clone)]
struct NpsChartInput {
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
struct NpsGlobals {
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
struct NpsTimingInput {
    measure_densities: Vec<usize>,
    timing: rssp::timing::TimingData,
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

fn build_nps_inputs() -> (Vec<NpsChartInput>, NpsGlobals) {
    let parsed =
        rssp::parse::extract_sections(FIXTURE.as_bytes(), EXTENSION).expect("fixture should parse");
    let timing_format = rssp::timing::TimingFormat::from_extension(EXTENSION);
    let ssc_version = rssp::parse::parse_version(parsed.version, timing_format);
    let allow_steps_timing = rssp::timing::steps_timing_allowed(ssc_version, timing_format);

    let globals = NpsGlobals {
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
            if entry.field_count < 5 {
                return None;
            }

            let step_type = std::str::from_utf8(entry.fields[0]).unwrap_or("").trim();
            if step_type == "lights-cabinet" {
                return None;
            }

            Some(NpsChartInput {
                chart_data: entry.note_data.to_vec(),
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

fn build_nps_timing_inputs(charts: &[NpsChartInput], globals: &NpsGlobals) -> Vec<NpsTimingInput> {
    let mut inputs = Vec::with_capacity(charts.len());
    for chart in charts {
        let (_minimized, _stats, measure_densities) =
            rssp::stats::minimize_chart_and_count_with_lanes(&chart.chart_data, chart.lanes);

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

        inputs.push(NpsTimingInput {
            measure_densities,
            timing,
        });
    }
    inputs
}

fn bench_nps_pipeline(c: &mut Criterion) {
    let fixture = FIXTURE.as_bytes();
    let mut group = c.benchmark_group("nps_pipeline");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_chart_peak_nps", |b| {
        b.iter(|| {
            let nps = rssp::compute_chart_peak_nps(black_box(fixture), black_box(EXTENSION))
                .expect("nps should succeed");
            black_box(nps);
        })
    });
    group.finish();
}

fn bench_nps_inner(c: &mut Criterion) {
    let (charts, globals) = build_nps_inputs();
    let mut group = c.benchmark_group("nps_inner");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_nps_inner", |b| {
        b.iter(|| {
            let mut outputs = Vec::with_capacity(charts.len());
            for chart in &charts {
                let (_minimized, _stats, measure_densities) =
                    rssp::stats::minimize_chart_and_count_with_lanes(
                        black_box(&chart.chart_data),
                        black_box(chart.lanes),
                    );

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

                let measure_nps_vec =
                    rssp::bpm::compute_measure_nps_vec_with_timing(&measure_densities, &timing);
                let stats = rssp::bpm::get_nps_stats(&measure_nps_vec);
                outputs.push(stats);
            }
            black_box(outputs);
        })
    });
    group.finish();
}

fn bench_nps_stats(c: &mut Criterion) {
    let (charts, globals) = build_nps_inputs();
    let timing_inputs = build_nps_timing_inputs(&charts, &globals);
    let mut group = c.benchmark_group("nps_stats");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_measure_nps_vec_with_timing", |b| {
        b.iter(|| {
            let mut outputs = Vec::with_capacity(timing_inputs.len());
            for entry in &timing_inputs {
                let measure_nps_vec = rssp::bpm::compute_measure_nps_vec_with_timing(
                    black_box(&entry.measure_densities),
                    black_box(&entry.timing),
                );
                let stats = rssp::bpm::get_nps_stats(&measure_nps_vec);
                outputs.push(stats);
            }
            black_box(outputs);
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_nps_pipeline,
    bench_nps_inner,
    bench_nps_stats
);
criterion_main!(benches);
