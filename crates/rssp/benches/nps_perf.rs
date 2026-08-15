use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/watch_yo_step.ssc");
const EXTENSION: &str = "ssc";

fn large_pair_map(entries: usize) -> String {
    use std::fmt::Write;

    let mut map = String::with_capacity(entries * 20);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        write!(&mut map, "{}={}", idx * 4, 60 + idx % 300).unwrap();
    }
    map
}

fn large_stop_map(entries: usize) -> String {
    use std::fmt::Write;

    let mut map = String::with_capacity(entries * 16);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        write!(&mut map, "{}=0.125", idx * 8).unwrap();
    }
    map
}

fn nps_checksum(values: &[f64]) -> u64 {
    values.iter().fold(0u64, |sum, value| {
        sum.rotate_left(7) ^ value.to_bits().wrapping_mul(0x9e37_79b9_7f4a_7c15)
    })
}

fn assert_nps_bpm_edges() {
    assert_eq!(
        rssp::bpm::compute_measure_nps_vec(&[16, 20], &[]),
        [0.0, 0.0]
    );
    assert_eq!(
        rssp::bpm::compute_measure_nps_vec(&[16, 16], &[(8.0, 120.0)]),
        [8.0, 8.0]
    );
    assert_eq!(
        rssp::bpm::compute_measure_nps_vec(&[16, 16], &[(0.0, 120.0), (4.0, 150.0), (4.0, 180.0)]),
        [8.0, 12.0]
    );
}

fn inherited_timing_fixture(chart_count: usize, bpm_count: usize) -> String {
    use std::fmt::Write;

    let mut fixture = String::with_capacity(chart_count * 160 + bpm_count * 20);
    fixture.push_str("#VERSION:0.83;\n#OFFSET:0;\n#BPMS:");
    for idx in 0..bpm_count {
        if idx != 0 {
            fixture.push(',');
        }
        write!(&mut fixture, "{}={}", idx * 4, 120 + idx % 180).unwrap();
    }
    fixture.push_str(";\n");
    for idx in 0..chart_count {
        write!(
            &mut fixture,
            concat!(
                "#NOTEDATA:;\n",
                "#STEPSTYPE:dance-single;\n",
                "#DESCRIPTION:cache-{};\n",
                "#DIFFICULTY:Challenge;\n",
                "#METER:10;\n",
                "#CREDIT:;\n",
                "#NOTES:\n1000\n0000\n0000\n0000\n;\n"
            ),
            idx
        )
        .unwrap();
    }
    fixture
}

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
    let timing_format = rssp::timing::timing_format_from_ext(EXTENSION);
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

        let timing = rssp::timing::timing_data_from_chart_data(
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
    let inherited_fixture = inherited_timing_fixture(32, 256);
    let mut group = c.benchmark_group("nps_pipeline");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("compute_chart_peak_nps_materialized", |b| {
        b.iter(|| {
            let nps = rssp::nps::compute_chart_peak_nps_legacy_for_bench(
                black_box(fixture),
                black_box(EXTENSION),
            )
            .expect("nps should succeed");
            black_box(nps);
        });
    });
    group.bench_function("compute_chart_peak_nps_reused", |b| {
        b.iter(|| {
            let nps = rssp::compute_chart_peak_nps(black_box(fixture), black_box(EXTENSION))
                .expect("nps should succeed");
            black_box(nps);
        });
    });
    group.bench_function("compute_chart_peak_nps_many_inherited", |b| {
        b.iter(|| {
            let nps = rssp::compute_chart_peak_nps(
                black_box(inherited_fixture.as_bytes()),
                black_box(EXTENSION),
            )
            .expect("nps should succeed");
            black_box(nps);
        });
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

                let timing = rssp::timing::timing_data_from_chart_data(
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
        });
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
        });
    });
    group.bench_function("reused_median_scratch", |b| {
        b.iter(|| {
            let mut outputs = Vec::with_capacity(timing_inputs.len());
            let mut scratch = Vec::new();
            for entry in &timing_inputs {
                let measure_nps_vec = rssp::bpm::compute_measure_nps_vec_with_timing(
                    black_box(&entry.measure_densities),
                    black_box(&entry.timing),
                );
                let stats = rssp::bpm::get_nps_stats_with_scratch(&measure_nps_vec, &mut scratch);
                outputs.push(stats);
            }
            black_box(outputs);
        });
    });
    group.bench_function("peak_only_with_timing", |b| {
        b.iter(|| {
            let mut outputs = Vec::with_capacity(timing_inputs.len());
            for entry in &timing_inputs {
                outputs.push(rssp::nps::compute_peak_nps_with_timing(
                    black_box(&entry.measure_densities),
                    black_box(&entry.timing),
                ));
            }
            black_box(outputs);
        });
    });
    group.finish();
}

fn equally_spaced_chart(measures: usize, rows_per_measure: usize) -> Vec<u8> {
    let mut chart = Vec::with_capacity(measures * rows_per_measure * 5);
    for measure in 0..measures {
        for row in 0..rows_per_measure {
            let lane = (measure + row) & 3;
            for col in 0..4 {
                chart.push(if col == lane && row % 5 != 0 {
                    b'1'
                } else {
                    b'0'
                });
            }
            chart.push(b'\n');
        }
        if measure + 1 != measures {
            chart.extend_from_slice(b",\n");
        }
    }
    chart
}

fn bench_equally_spaced(c: &mut Criterion) {
    let chart = equally_spaced_chart(1_024, 48);
    let minimized = rssp::stats::minimize_chart_for_hash(&chart, 4);
    let mut group = c.benchmark_group("equally_spaced");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(1_024));
    group.bench_function("dense_chart", |b| {
        b.iter(|| {
            black_box(rssp::stats::measure_equally_spaced(
                black_box(&chart),
                black_box(4),
            ));
        });
    });
    group.bench_function("materialized_minimized", |b| {
        b.iter(|| {
            black_box(rssp::stats::measure_equally_spaced(
                black_box(&minimized),
                black_box(4),
            ));
        });
    });
    group.bench_function("visitor_minimized", |b| {
        b.iter(|| {
            let mut checksum = 0usize;
            rssp::stats::visit_measure_spacing(black_box(&minimized), black_box(4), |spaced| {
                checksum = checksum.wrapping_add(usize::from(spaced));
                Ok::<(), std::convert::Infallible>(())
            })
            .expect("infallible spacing visitor should succeed");
            black_box(checksum);
        });
    });
    group.finish();
}

fn bench_nps_timing_cursor(c: &mut Criterion) {
    let bpms = large_pair_map(512);
    let stops = large_stop_map(256);
    let timing = rssp::timing::timing_data_from_chart_data(
        0.0,
        0.0,
        None,
        &bpms,
        None,
        &stops,
        None,
        "",
        None,
        "",
        None,
        "",
        None,
        "",
        None,
        "",
        rssp::timing::TimingFormat::Ssc,
        true,
    );
    let densities: Vec<_> = (0..512)
        .map(|idx| [0, 16, 20, 24, 32][(idx * 7) % 5])
        .collect();
    let checksum = nps_checksum(&rssp::bpm::compute_measure_nps_vec_with_timing(
        &densities, &timing,
    ));
    assert_eq!(checksum, 5_059_034_228_849_603_396);

    let mut group = c.benchmark_group("nps_timing_cursor");
    group.throughput(Throughput::Elements(densities.len() as u64));
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("measure_512", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_measure_nps_vec_with_timing(
                black_box(&densities),
                black_box(&timing),
            ));
        });
    });
    group.finish();
}

fn bench_nps_bpm_cursor(c: &mut Criterion) {
    assert_nps_bpm_edges();
    let bpms: Vec<_> = (0..4_096)
        .map(|idx| (idx as f64 * 4.0, 60.0 + ((idx * 37) % 300) as f64))
        .collect();
    let densities: Vec<_> = (0..4_096)
        .map(|idx| [0, 16, 20, 24, 32][(idx * 7) % 5])
        .collect();
    let checksum = nps_checksum(&rssp::bpm::compute_measure_nps_vec(&densities, &bpms));
    assert_eq!(checksum, 8_159_583_529_960_956_954);

    let mut group = c.benchmark_group("nps_bpm_cursor");
    group.throughput(Throughput::Elements(densities.len() as u64));
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("measure_4096", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_measure_nps_vec(
                black_box(&densities),
                black_box(&bpms),
            ));
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_nps_pipeline,
    bench_nps_inner,
    bench_nps_stats,
    bench_nps_timing_cursor,
    bench_nps_bpm_cursor,
    bench_equally_spaced
);
criterion_main!(benches);
