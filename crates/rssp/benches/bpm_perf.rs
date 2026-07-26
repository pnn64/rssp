use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/bpm_fixture.ssc");

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
    let inherited_fixture = inherited_timing_fixture(32, 256);
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
    group.bench_function("chart_bpm_snapshots_many_inherited", |b| {
        b.iter(|| {
            let snapshots =
                rssp::bpm::chart_bpm_snapshots(black_box(inherited_fixture.as_bytes()), "ssc")
                    .expect("inherited BPM snapshots should succeed");
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

                let bpms_formatted = rssp::timing::format_bpm_segments_like_itg(
                    &rssp::timing::bpm_segments(&timing),
                );
                outputs.push((hash_bpms, bpms_formatted));
            }
            black_box(outputs);
        });
    });
    group.finish();
}

fn bench_bpm_format(c: &mut Criterion) {
    let bpms: Vec<_> = (0..512)
        .map(|idx| {
            (
                idx as f64 * 0.125,
                60.0 + f64::from((idx * 37 % 360) as u32) / 3.0,
            )
        })
        .collect();

    let mut group = c.benchmark_group("bpm_format");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("format_segments", |b| {
        b.iter(|| {
            black_box(rssp::timing::format_bpm_segments_like_itg(black_box(&bpms)));
        });
    });
    group.finish();
}

fn mine_chart(measures: usize, rows_per_measure: usize) -> Vec<u8> {
    let mut chart = Vec::with_capacity(measures * rows_per_measure * 5);
    for measure in 0..measures {
        for row in 0..rows_per_measure {
            let lane = (measure + row) & 3;
            let mine = (measure * rows_per_measure + row) % 5 == 0;
            for col in 0..4 {
                chart.push(if mine && col == lane { b'M' } else { b'0' });
            }
            chart.push(b'\n');
        }
        if measure + 1 != measures {
            chart.extend_from_slice(b",\n");
        }
    }
    chart
}

fn bench_mines_nonfake(c: &mut Criterion) {
    let chart = mine_chart(1_024, 48);
    let warps: Vec<_> = (0..64).map(|idx| (idx as f64 * 64.0 + 8.0, 4.0)).collect();
    let fakes: Vec<_> = (0..64).map(|idx| (idx as f64 * 64.0 + 24.0, 8.0)).collect();

    let mut group = c.benchmark_group("mines_nonfake");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("dense_chart", |b| {
        b.iter(|| {
            black_box(rssp::bpm::compute_mines_nonfake(
                black_box(&chart),
                black_box(4),
                black_box(&warps),
                black_box(&fakes),
            ));
        });
    });
    group.finish();
}

fn large_bpm_map(entries: usize) -> String {
    let mut map = String::with_capacity(entries * 20);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        use std::fmt::Write;
        write!(&mut map, "{}={}", idx * 4, 60 + idx % 300).unwrap();
    }
    map
}

fn large_speed_map(entries: usize) -> String {
    let mut map = String::with_capacity(entries * 28);
    for idx in 0..entries {
        if idx != 0 {
            map.push(',');
        }
        use std::fmt::Write;
        write!(&mut map, "{}={}=0={}", idx * 4, 1 + idx % 7, idx & 1).unwrap();
    }
    map
}

fn bench_parse_bpm_map(c: &mut Criterion) {
    let map = large_bpm_map(4_096);
    let mut group = c.benchmark_group("parse_bpm_map");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("ordered_map", |b| {
        b.iter(|| {
            black_box(rssp::bpm::parse_bpm_map(black_box(&map)));
        });
    });
    group.finish();
}

fn bench_timing_segment_cleanup(c: &mut Criterion) {
    let ordered_map = large_bpm_map(4_096);
    let ordered_speeds = large_speed_map(4_096);
    let mut group = c.benchmark_group("timing_segment_cleanup");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("ordered_maps", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                black_box(0.0),
                black_box(0.0),
                None,
                black_box("0=120"),
                None,
                black_box(&ordered_map),
                None,
                black_box(&ordered_map),
                None,
                black_box(&ordered_map),
                None,
                "",
                None,
                "",
                None,
                black_box(&ordered_map),
                rssp::timing::TimingFormat::Ssc,
                true,
            ));
        });
    });
    group.bench_function("ordered_bpms", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                black_box(0.0),
                black_box(0.0),
                None,
                black_box(&ordered_map),
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
                None,
                "",
                rssp::timing::TimingFormat::Ssc,
                true,
            ));
        });
    });
    group.bench_function("ordered_speeds", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                black_box(0.0),
                black_box(0.0),
                None,
                black_box("0=120"),
                None,
                "",
                None,
                "",
                None,
                "",
                None,
                black_box(&ordered_speeds),
                None,
                "",
                None,
                "",
                rssp::timing::TimingFormat::Ssc,
                true,
            ));
        });
    });
    group.bench_function("ordered_scrolls", |b| {
        b.iter(|| {
            black_box(rssp::timing::timing_data_from_chart_data(
                black_box(0.0),
                black_box(0.0),
                None,
                black_box("0=120"),
                None,
                "",
                None,
                "",
                None,
                "",
                None,
                "",
                None,
                black_box(&ordered_map),
                None,
                "",
                rssp::timing::TimingFormat::Ssc,
                true,
            ));
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_bpm_pipeline,
    bench_bpm_inner,
    bench_bpm_format,
    bench_mines_nonfake,
    bench_parse_bpm_map,
    bench_timing_segment_cleanup
);
criterion_main!(benches);
