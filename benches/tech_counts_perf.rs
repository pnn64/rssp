use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/camellia_mix.ssc");
const EXTENSION: &str = "ssc";

#[derive(Clone)]
struct TechChartInput {
    chart_data: Vec<u8>,
    lanes: usize,
    chart_bpms: Option<String>,
    chart_stops: Option<String>,
    chart_delays: Option<String>,
    chart_warps: Option<String>,
    chart_speeds: Option<String>,
    chart_scrolls: Option<String>,
    chart_fakes: Option<String>,
}

#[derive(Clone)]
struct TechGlobals {
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
struct TechParityInput {
    minimized_chart: Vec<u8>,
    timing: rssp::timing::TimingData,
    lanes: usize,
}

fn clean_tag_bytes(tag: Option<&[u8]>) -> String {
    tag.and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(rssp::bpm::clean_timing_map)
        .unwrap_or_default()
}

fn clean_chart_tag(tag: &Option<Vec<u8>>) -> Option<String> {
    tag.as_ref()
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(rssp::bpm::clean_timing_map)
        .filter(|s| !s.is_empty())
}

fn build_tech_inputs() -> (Vec<TechChartInput>, TechGlobals) {
    let parsed =
        rssp::parse::extract_sections(FIXTURE.as_bytes(), EXTENSION).expect("fixture should parse");
    let timing_format = rssp::timing::TimingFormat::from_extension(EXTENSION);
    let ssc_version = rssp::parse::parse_version(parsed.version, timing_format);
    let allow_steps_timing = rssp::timing::steps_timing_allowed(ssc_version, timing_format);

    let globals = TechGlobals {
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

            Some(TechChartInput {
                chart_data: chart_data.to_vec(),
                lanes: rssp::step_type_lanes(step_type),
                chart_bpms: clean_chart_tag(&entry.chart_bpms),
                chart_stops: clean_chart_tag(&entry.chart_stops),
                chart_delays: clean_chart_tag(&entry.chart_delays),
                chart_warps: clean_chart_tag(&entry.chart_warps),
                chart_speeds: clean_chart_tag(&entry.chart_speeds),
                chart_scrolls: clean_chart_tag(&entry.chart_scrolls),
                chart_fakes: clean_chart_tag(&entry.chart_fakes),
            })
        })
        .collect();

    (charts, globals)
}

fn minimize_chart(chart_data: &[u8], lanes: usize) -> Vec<u8> {
    let (mut minimized_chart, _stats, _measure_densities) =
        rssp::stats::minimize_chart_and_count_with_lanes(chart_data, lanes);
    if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
        minimized_chart.truncate(pos + 1);
    }
    minimized_chart
}

fn timing_for_chart(chart: &TechChartInput, globals: &TechGlobals) -> rssp::timing::TimingData {
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

    rssp::timing::TimingData::from_chart_data(
        globals.song_offset,
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
    )
}

fn build_parity_inputs(
    charts: &[TechChartInput],
    globals: &TechGlobals,
) -> Vec<TechParityInput> {
    let mut out = Vec::with_capacity(charts.len());
    for chart in charts {
        let minimized_chart = minimize_chart(&chart.chart_data, chart.lanes);
        let timing = timing_for_chart(chart, globals);
        out.push(TechParityInput {
            minimized_chart,
            timing,
            lanes: chart.lanes,
        });
    }
    out
}

fn bench_tech_counts_pipeline(c: &mut Criterion) {
    let fixture = FIXTURE.as_bytes();
    let options = rssp::AnalysisOptions::default();
    let mut group = c.benchmark_group("tech_counts_pipeline");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("analyze_tech_counts", |b| {
        b.iter(|| {
            let summary = rssp::analyze(
                black_box(fixture),
                black_box(EXTENSION),
                options.clone(),
            )
            .expect("analysis should succeed");
            let counts: Vec<_> = summary.charts.iter().map(|chart| chart.tech_counts).collect();
            black_box(counts);
        })
    });
    group.finish();
}

fn bench_tech_counts_inner(c: &mut Criterion) {
    let (charts, globals) = build_tech_inputs();
    let mut group = c.benchmark_group("tech_counts_inner");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_tech_counts", |b| {
        b.iter(|| {
            let mut outputs = Vec::with_capacity(charts.len());
            for chart in &charts {
                let minimized_chart = minimize_chart(
                    black_box(&chart.chart_data),
                    black_box(chart.lanes),
                );
                let timing = timing_for_chart(chart, &globals);
                let counts = rssp::step_parity::analyze_timing_lanes(
                    &minimized_chart,
                    &timing,
                    chart.lanes,
                );
                outputs.push(counts);
            }
            black_box(outputs);
        })
    });
    group.finish();
}

fn bench_tech_counts_step_parity(c: &mut Criterion) {
    let (charts, globals) = build_tech_inputs();
    let inputs = build_parity_inputs(&charts, &globals);
    let mut group = c.benchmark_group("tech_counts_step_parity");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("step_parity_analyze", |b| {
        b.iter(|| {
            let mut outputs = Vec::with_capacity(inputs.len());
            for entry in &inputs {
                let counts = rssp::step_parity::analyze_timing_lanes(
                    black_box(&entry.minimized_chart),
                    black_box(&entry.timing),
                    black_box(entry.lanes),
                );
                outputs.push(counts);
            }
            black_box(outputs);
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_tech_counts_pipeline,
    bench_tech_counts_inner,
    bench_tech_counts_step_parity
);
criterion_main!(benches);
