use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Duration;

#[path = "support/bpm_display.rs"]
mod bpm_display_bench;
#[path = "support/bpm_summary.rs"]
mod bpm_summary_bench;
#[path = "support/elapsed.rs"]
mod elapsed_bench;
#[path = "support/timing_borrow.rs"]
mod timing_borrow_bench;
#[path = "support/timing_merge.rs"]
mod timing_merge_bench;
#[path = "support/timing_sort.rs"]
mod timing_sort_bench;

const FIXTURE: &str = include_str!("fixtures/bpm_fixture.ssc");

fn control_pair_map(entries: usize) -> (String, String) {
    let mut raw = String::with_capacity(entries * 20);
    let mut expected = String::with_capacity(entries * 16);
    for idx in 0..entries {
        if idx != 0 {
            raw.push(',');
            expected.push(',');
        }
        write!(&mut raw, "\u{000b}{}={}\u{000b}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
        write!(&mut expected, "{}={}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
    }
    (raw, expected)
}

fn control_normalize_map(entries: usize) -> (String, String, String) {
    let mut raw = String::with_capacity(entries * 20);
    let mut cleaned = String::with_capacity(entries * 16);
    let mut expected = String::with_capacity(entries * 24);
    for idx in 0..entries {
        if idx != 0 {
            raw.push(',');
            cleaned.push(',');
            expected.push(',');
        }
        write!(&mut raw, "+\u{000b}{}=+\u{000b}{}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
        write!(&mut cleaned, "+{}=+{}", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
        write!(&mut expected, "{}.000={}.000", idx * 4, 60 + idx % 300)
            .expect("writing to a String cannot fail");
    }
    (raw, cleaned, expected)
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

fn bench_display_tags(c: &mut Criterion) {
    let fixture = bpm_display_bench::fixture();
    bpm_display_bench::assert_behavior(&fixture);
    let mut group = c.benchmark_group("bpm_display_tags_256");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(criterion::Throughput::Elements(
        bpm_display_bench::CHART_COUNT as u64,
    ));
    group.bench_function("owned_temporary", |b| {
        b.iter(|| black_box(bpm_display_bench::compute(black_box(&fixture), true)));
    });
    group.bench_function("borrowed_tag", |b| {
        b.iter(|| black_box(bpm_display_bench::compute(black_box(&fixture), false)));
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

    let native_bpms: Vec<_> = bpms
        .iter()
        .map(|&(beat, bpm)| (beat as f32, bpm as f32))
        .collect();
    let mut group = c.benchmark_group("native_bpm_format");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("materialized", |b| {
        b.iter(|| {
            black_box(rssp::timing::format_bpm_segments_f32_like_itg(black_box(
                &native_bpms,
            )));
        });
    });
    group.bench_function("streamed", |b| {
        let mut output = String::with_capacity(native_bpms.len() * 24);
        b.iter(|| {
            output.clear();
            write!(
                &mut output,
                "{}",
                rssp::timing::native_bpms_display(black_box(&native_bpms))
            )
            .expect("BPM display should write to String");
            black_box(output.len());
        });
    });
    group.finish();
}

fn bench_clean_timing_map(c: &mut Criterion) {
    const CASES: [(&str, &str); 6] = [
        ("", ""),
        (",0=120,,4=180,", "0=120,4=180"),
        (" \u{000b}\t0=120\t\u{000b} ", "0=120"),
        ("\u{000b}0=\t120\u{000b}", "0=120"),
        ("0=\t120", "0=\t120"),
        ("\u{00a0}\u{000b}0=120\u{000b}\u{00a0}", "0=120"),
    ];
    for (raw, expected) in CASES {
        assert_eq!(rssp::bpm::clean_timing_map(raw), expected);
    }

    let (raw, expected) = control_pair_map(512);
    assert_eq!(rssp::bpm::clean_timing_map(&raw), expected);

    let mut group = c.benchmark_group("clean_timing_map");
    group.throughput(criterion::Throughput::Bytes(raw.len() as u64));
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("control_512", |b| {
        b.iter(|| {
            black_box(rssp::bpm::clean_timing_map(black_box(&raw)));
        });
    });
    group.finish();
}

fn bench_control_normalize(c: &mut Criterion) {
    const CASES: [(&str, &str); 4] = [
        ("+\u{000b}0=+\u{000b}120", "0.000=120.000"),
        ("+\u{000b}0=+\t\u{000b}120", "0.000=120.000"),
        ("0=1\t20", ""),
        ("+\u{000b}0=+\u{0085}\u{000b}120", "0.000=120.000"),
    ];
    for (raw, expected) in CASES {
        assert_eq!(rssp::bpm::normalize_float_digits(raw), expected);
    }
    let long = format!("+\u{000b}{}=120", "0".repeat(80));
    assert_eq!(rssp::bpm::normalize_float_digits(&long), "0.000=120.000");

    let (raw, _, expected) = control_normalize_map(512);
    assert_eq!(rssp::bpm::normalize_float_digits(&raw), expected);

    let mut group = c.benchmark_group("control_normalize");
    group.throughput(criterion::Throughput::Bytes(raw.len() as u64));
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("pair_512", |b| {
        b.iter(|| {
            black_box(rssp::bpm::normalize_float_digits(black_box(&raw)));
        });
    });
    group.finish();
}

fn bench_control_fused(c: &mut Criterion) {
    let (raw, cleaned, normalized) = control_normalize_map(512);
    assert_eq!(
        rssp::bpm::clean_and_normalize_float_digits(&raw),
        (cleaned, normalized)
    );
    assert_eq!(
        rssp::bpm::clean_and_normalize_speeds_float_digits("+\u{000b}0=+\u{000b}1=+\u{000b}2=0"),
        ("+0=+1=+2=0".to_owned(), "0.000=1.000=2.000=0".to_owned())
    );

    let mut group = c.benchmark_group("control_fused");
    group.throughput(criterion::Throughput::Bytes(raw.len() as u64));
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("pair_512", |b| {
        b.iter(|| {
            black_box(rssp::bpm::clean_and_normalize_float_digits(black_box(&raw)));
        });
    });
    group.finish();
}

fn bench_timing_borrow(c: &mut Criterion) {
    let maps = timing_borrow_bench::TimingMaps::new();
    timing_borrow_bench::assert_behavior(&maps);
    let mut group = c.benchmark_group("clean_normalize_timing_maps");
    group.throughput(criterion::Throughput::Bytes(maps.bytes()));
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.bench_function("owned_cleaned_maps", |b| {
        b.iter(|| black_box(maps.owned()));
    });
    group.bench_function("borrowed_clean_maps", |b| {
        b.iter(|| black_box(maps.borrowed()));
    });
    group.finish();
}

fn bench_display_bpm(c: &mut Criterion) {
    const CASES: [(Option<&str>, f64, f64, f64); 4] = [
        (None, 120.0, 180.0, 1.0),
        (Some("150"), 120.0, 180.0, 1.0),
        (Some("120:180"), 120.0, 180.0, 1.25),
        (Some("*"), 90.0, 240.0, 1.1),
    ];
    let mut checksum = 0u64;
    for (tag, min, max, rate) in CASES {
        let result = rssp::bpm::resolve_display_bpm(tag, min, max, rate);
        checksum ^= result.0.to_bits().rotate_left(7) ^ result.1.to_bits();
        for byte in result.2.bytes() {
            checksum = checksum.rotate_left(5) ^ u64::from(byte);
        }
    }
    assert_eq!(checksum, 17_060_450_905_395_141_691);

    let mut group = c.benchmark_group("display_bpm");
    group.throughput(criterion::Throughput::Elements(1_024));
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("mixed_1024", |b| {
        b.iter(|| {
            for _ in 0..256 {
                for (tag, min, max, rate) in CASES {
                    black_box(rssp::bpm::resolve_display_bpm(
                        black_box(tag),
                        black_box(min),
                        black_box(max),
                        black_box(rate),
                    ));
                }
            }
        });
    });
    group.finish();
}

fn bench_bpm_stats(c: &mut Criterion) {
    let map = bpm_summary_bench::fixture();
    bpm_summary_bench::assert_behavior(&map);
    let mut legacy_values = Vec::with_capacity(map.len());
    let mut fused_values = Vec::with_capacity(map.len());

    let mut group = c.benchmark_group("bpm_range_stats");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(criterion::Throughput::Elements(map.len() as u64));
    for (phase, legacy) in [("sum_after_fill", true), ("sum_while_fill", false)] {
        let values = if legacy {
            &mut legacy_values
        } else {
            &mut fused_values
        };
        group.bench_function(phase, |b| {
            b.iter(|| {
                black_box(bpm_summary_bench::compute(
                    black_box(&map),
                    black_box(values),
                    legacy,
                ));
            });
        });
    }
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

fn bench_timing_segment_sort(c: &mut Criterion) {
    let fixture = timing_sort_bench::fixture();
    timing_sort_bench::assert_behavior(&fixture);
    let mut group = c.benchmark_group("timing_segment_sort_4096");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(criterion::Throughput::Elements(
        timing_sort_bench::ENTRY_COUNT as u64,
    ));
    for (phase, legacy) in [("packed_records", true), ("key_indices", false)] {
        group.bench_function(phase, |b| {
            b.iter_batched(
                || fixture.clone(),
                |input| black_box(timing_sort_bench::tidy(input, legacy)),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_sm_stop_merge(c: &mut Criterion) {
    timing_merge_bench::assert_behavior();
    let fixture = timing_merge_bench::TimingMergeFixture::new();
    let mut group = c.benchmark_group("sm_stop_merge_2048_each");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(criterion::Throughput::Elements(
        timing_merge_bench::MERGE_INPUT_COUNT,
    ));
    group.bench_function("materialize_warps", |b| {
        b.iter(|| {
            black_box(timing_merge_bench::legacy_convert(
                black_box(&fixture.bpms),
                black_box(&fixture.stops),
                black_box(&fixture.delays),
                black_box(&fixture.warps),
            ));
        });
    });
    group.bench_function("fused_warps", |b| {
        b.iter(|| {
            black_box(rssp::timing::convert_warps_and_delays_to_sm_stops(
                black_box(&fixture.bpms),
                black_box(&fixture.stops),
                black_box(&fixture.delays),
                black_box(&fixture.warps),
            ));
        });
    });
    group.finish();
}

fn bench_elapsed_events(c: &mut Criterion) {
    let fixture = elapsed_bench::ElapsedFixture::new();
    elapsed_bench::assert_behavior(&fixture);
    let mut group = c.benchmark_group("elapsed_events_512");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(criterion::Throughput::Elements(elapsed_bench::EVENT_COUNT));
    for (name, legacy) in [("collect_sort", true), ("stable_merge", false)] {
        group.bench_function(name, |b| {
            b.iter(|| {
                black_box(rssp::bpm::get_elapsed_time_for_bench(
                    black_box(fixture.target),
                    black_box(&fixture.bpms),
                    black_box(&fixture.stops),
                    black_box(&fixture.delays),
                    black_box(&fixture.warps),
                    legacy,
                ));
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_bpm_pipeline,
    bench_display_tags,
    bench_bpm_inner,
    bench_bpm_format,
    bench_clean_timing_map,
    bench_control_normalize,
    bench_control_fused,
    bench_timing_borrow,
    bench_display_bpm,
    bench_bpm_stats,
    bench_mines_nonfake,
    bench_parse_bpm_map,
    bench_timing_segment_cleanup,
    bench_timing_segment_sort,
    bench_sm_stop_merge,
    bench_elapsed_events
);
criterion_main!(benches);
