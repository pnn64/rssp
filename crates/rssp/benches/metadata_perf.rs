use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/camellia_mix.ssc");
const EXTENSION: &str = "ssc";

#[path = "support/metadata.rs"]
mod metadata_bench;
#[path = "support/parse_dispatch.rs"]
mod parse_dispatch_bench;
#[path = "support/selectable.rs"]
mod selectable_bench;
#[path = "support/text_report.rs"]
mod text_report_bench;
#[path = "support/translate.rs"]
mod translate_bench;

type MetadataStrings = (String, String, String, String, String, String, String);

fn legacy_chart_metadata_strings(
    fields: [&[u8]; 5],
    chart_name: Option<&[u8]>,
    timing_format: rssp::timing::TimingFormat,
    ssc_version: f32,
    extension: &str,
) -> MetadataStrings {
    let step_type = rssp::parse::unescape_trim(rssp::parse::decode_bytes(fields[0]).as_ref());
    let description_raw = rssp::parse::unescape_trim(rssp::parse::decode_bytes(fields[1]).as_ref());
    let chart_name_raw = chart_name.map_or_else(String::new, |bytes| {
        rssp::parse::unescape_trim(rssp::parse::decode_bytes(bytes).as_ref())
    });
    let description =
        rssp::parse::normalize_chart_desc(description_raw.clone(), timing_format, ssc_version);
    let chart_name = rssp::parse::normalize_chart_name(
        chart_name_raw,
        &description_raw,
        timing_format,
        ssc_version,
    );
    let difficulty_raw = rssp::parse::unescape_trim(rssp::parse::decode_bytes(fields[2]).as_ref());
    let rating = rssp::parse::unescape_trim(rssp::parse::decode_bytes(fields[3]).as_ref());
    let difficulty =
        rssp::resolve_difficulty_label(&difficulty_raw, &description, &rating, extension);
    let is_ssc = extension.eq_ignore_ascii_case("ssc");
    let credit_decoded = if is_ssc {
        rssp::parse::decode_bytes(fields[4])
    } else {
        std::borrow::Cow::Borrowed("")
    };
    let credit = rssp::parse::unescape_tag(credit_decoded.as_ref());
    let tech_notation = rssp::tech::parse_tech_notation(credit.as_ref(), &description);
    let step_artist = if is_ssc {
        credit.into_owned()
    } else {
        description.clone()
    };
    (
        step_type,
        step_artist,
        description,
        chart_name,
        difficulty,
        rating,
        tech_notation,
    )
}

fn bench_metadata_pipeline(c: &mut Criterion) {
    let fixture = FIXTURE.as_bytes();
    let timing_format = rssp::timing::timing_format_from_ext(EXTENSION);
    let mut group = c.benchmark_group("metadata");
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(2));
    group.bench_function("parse_metadata", |b| {
        b.iter(|| {
            let parsed = rssp::parse::extract_sections(black_box(fixture), black_box(EXTENSION))
                .expect("fixture should parse");
            let mut title = parsed
                .title
                .map(|b| {
                    let decoded = rssp::parse::decode_bytes(b);
                    let unescaped = rssp::parse::unescape_tag(decoded.as_ref());
                    rssp::parse::clean_tag(unescaped.as_ref()).into_owned()
                })
                .unwrap_or_else(|| "<invalid-title>".to_string());
            let trimmed_title = title.trim();
            if trimmed_title.len() != title.len() {
                title = trimmed_title.to_string();
            }

            let mut subtitle = parsed
                .subtitle
                .map(|b| {
                    rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()).into_owned()
                })
                .unwrap_or_default();
            let trimmed_subtitle = subtitle.trim();
            if trimmed_subtitle.len() != subtitle.len() {
                subtitle = trimmed_subtitle.to_string();
            }

            let mut artist = parsed
                .artist
                .map(|b| {
                    rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()).into_owned()
                })
                .unwrap_or_default();
            let trimmed_artist = artist.trim();
            if trimmed_artist.len() != artist.len() {
                artist = trimmed_artist.to_string();
            }

            let title_translit = parsed
                .title_translit
                .map(|b| {
                    rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()).into_owned()
                })
                .unwrap_or_default();
            let subtitle_translit = parsed
                .subtitle_translit
                .map(|b| {
                    rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()).into_owned()
                })
                .unwrap_or_default();
            let mut artist_translit = parsed
                .artist_translit
                .map(|b| {
                    rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()).into_owned()
                })
                .unwrap_or_default();

            if artist.is_empty() && artist_translit.trim().is_empty() {
                let unknown = "Unknown artist".to_string();
                artist = unknown.clone();
                artist_translit = unknown;
            }

            let (title_out, subtitle_out, artist_out) = rssp::display_metadata(
                &title,
                &subtitle,
                &artist,
                &title_translit,
                &subtitle_translit,
                &artist_translit,
                false,
            );

            let ssc_version = rssp::parse::parse_version(parsed.version, timing_format);
            let mut chart_meta_bytes = 0usize;
            let mut chart_count = 0usize;

            for entry in parsed.notes_list {
                if entry.field_count < 5 {
                    continue;
                }

                let step_type =
                    rssp::parse::unescape_trim(rssp::parse::decode_bytes(entry.fields[0]).as_ref());
                if step_type == "lights-cabinet" {
                    continue;
                }
                let desc_raw =
                    rssp::parse::unescape_trim(rssp::parse::decode_bytes(entry.fields[1]).as_ref());
                let description =
                    rssp::parse::normalize_chart_desc(desc_raw, timing_format, ssc_version);
                let difficulty =
                    rssp::parse::unescape_trim(rssp::parse::decode_bytes(entry.fields[2]).as_ref());
                let meter =
                    rssp::parse::unescape_trim(rssp::parse::decode_bytes(entry.fields[3]).as_ref());
                let credit_decoded = rssp::parse::decode_bytes(entry.fields[4]);
                let credit = rssp::parse::unescape_tag(credit_decoded.as_ref());

                chart_meta_bytes += step_type.len()
                    + description.len()
                    + difficulty.len()
                    + meter.len()
                    + credit.len();
                chart_count += 1;
            }

            black_box(title_out);
            black_box(subtitle_out);
            black_box(artist_out);
            black_box(chart_meta_bytes);
            black_box(chart_count);
        });
    });
    group.finish();
}

fn bench_chart_metadata_analysis(c: &mut Criterion) {
    let modern = metadata_bench::fixture("0.83");
    let legacy = metadata_bench::fixture("0.70");
    let options = metadata_bench::options();

    let mut group = c.benchmark_group("chart_metadata_analysis");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(metadata_bench::CHART_COUNT as u64));
    group.bench_function("analyze_256_modern_ssc", |b| {
        b.iter(|| {
            black_box(
                rssp::analyze(
                    black_box(modern.as_bytes()),
                    black_box(EXTENSION),
                    black_box(&options),
                )
                .expect("modern metadata fixture should analyze"),
            );
        });
    });
    group.bench_function("analyze_256_legacy_ssc", |b| {
        b.iter(|| {
            black_box(
                rssp::analyze(
                    black_box(legacy.as_bytes()),
                    black_box(EXTENSION),
                    black_box(&options),
                )
                .expect("legacy metadata fixture should analyze"),
            );
        });
    });
    group.finish();
}

fn bench_selectable(c: &mut Criterion) {
    selectable_bench::assert_behavior();
    let mut group = c.benchmark_group("selectable_4096");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(selectable_bench::BATCH as u64));
    group.bench_function("owned_compare", |b| {
        b.iter(|| black_box(selectable_bench::run::<true>()));
    });
    group.bench_function("borrowed_compare", |b| {
        b.iter(|| black_box(selectable_bench::run::<false>()));
    });
    group.finish();
}

fn bench_text_report(c: &mut Criterion) {
    let fixture = metadata_bench::fixture("0.83");
    let summary = rssp::analyze(fixture.as_bytes(), EXTENSION, &metadata_bench::options())
        .expect("text report fixture should analyze");
    text_report_bench::assert_behavior(&summary);

    for (group_name, full) in [
        ("text_report_pretty_256", false),
        ("text_report_full_256", true),
    ] {
        let mut sizing = Vec::new();
        text_report_bench::write(&summary, &mut sizing, full, false);
        let mut group = c.benchmark_group(group_name);
        group.sample_size(100);
        group.measurement_time(Duration::from_secs(3));
        group.throughput(Throughput::Elements(metadata_bench::CHART_COUNT as u64));
        for (phase, legacy) in [("materialized", true), ("streamed", false)] {
            let mut output = Vec::with_capacity(sizing.len());
            group.bench_function(phase, |b| {
                b.iter(|| {
                    black_box(text_report_bench::write(
                        black_box(&summary),
                        black_box(&mut output),
                        full,
                        legacy,
                    ));
                });
            });
        }
        group.finish();
    }
}

fn bench_chart_metadata_strings(c: &mut Criterion) {
    let modern = metadata_bench::fixture("0.83");
    let legacy = metadata_bench::fixture("0.70");
    let modern_entries = rssp::parse::extract_sections(modern.as_bytes(), EXTENSION)
        .expect("modern metadata fixture should parse")
        .notes_list;
    let legacy_entries = rssp::parse::extract_sections(legacy.as_bytes(), EXTENSION)
        .expect("legacy metadata fixture should parse")
        .notes_list;
    let timing_format = rssp::timing::TimingFormat::Ssc;

    let mut group = c.benchmark_group("chart_metadata_strings_256");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(metadata_bench::CHART_COUNT as u64));
    group.bench_function("legacy_materialized_modern", |b| {
        b.iter(|| {
            for entry in black_box(&modern_entries) {
                black_box(legacy_chart_metadata_strings(
                    entry.fields,
                    entry.chart_name,
                    timing_format,
                    0.83,
                    EXTENSION,
                ));
            }
        });
    });
    group.bench_function("borrowed_owned_modern", |b| {
        b.iter(|| {
            for entry in black_box(&modern_entries) {
                black_box(rssp::analysis::profile_chart_metadata_strings(
                    entry.fields,
                    entry.chart_name,
                    timing_format,
                    0.83,
                    EXTENSION,
                ));
            }
        });
    });
    group.bench_function("legacy_materialized_old_ssc", |b| {
        b.iter(|| {
            for entry in black_box(&legacy_entries) {
                black_box(legacy_chart_metadata_strings(
                    entry.fields,
                    entry.chart_name,
                    timing_format,
                    0.70,
                    EXTENSION,
                ));
            }
        });
    });
    group.bench_function("borrowed_owned_old_ssc", |b| {
        b.iter(|| {
            for entry in black_box(&legacy_entries) {
                black_box(rssp::analysis::profile_chart_metadata_strings(
                    entry.fields,
                    entry.chart_name,
                    timing_format,
                    0.70,
                    EXTENSION,
                ));
            }
        });
    });
    group.finish();
}

fn bench_marker_translation(c: &mut Criterion) {
    translate_bench::assert_behavior();
    let unknown = translate_bench::unknown_input();
    let aliases = translate_bench::alias_input();

    let mut group = c.benchmark_group("marker_translation");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(translate_bench::MARKER_COUNT as u64));
    for (name, legacy) in [
        ("unknown_allocating_512", true),
        ("unknown_compact_512", false),
    ] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || unknown.clone(),
                |mut input| {
                    rssp::translate::profile_replace_markers(black_box(&mut input), legacy);
                    black_box(input);
                },
                BatchSize::SmallInput,
            );
        });
    }
    for (name, legacy) in [
        ("aliases_allocating_512", true),
        ("aliases_compact_512", false),
    ] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || aliases.clone(),
                |mut input| {
                    rssp::translate::profile_replace_markers(black_box(&mut input), legacy);
                    black_box(input);
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_parse_dispatch(c: &mut Criterion) {
    let fixture = parse_dispatch_bench::fixture();
    parse_dispatch_bench::assert_behavior(&fixture);
    parse_dispatch_bench::assert_pair(FIXTURE.as_bytes(), EXTENSION);
    for (name, data) in [
        ("parse_dispatch_128_charts", fixture.as_slice()),
        ("parse_dispatch_real_ssc", FIXTURE.as_bytes()),
    ] {
        let mut group = c.benchmark_group(name);
        group.sample_size(100);
        group.measurement_time(Duration::from_secs(3));
        group.throughput(Throughput::Bytes(data.len() as u64));
        for (phase, legacy) in [("indexed_tags", false), ("sequential_tags", true)] {
            group.bench_function(phase, |b| {
                b.iter(|| {
                    black_box(parse_dispatch_bench::parse(
                        black_box(data),
                        EXTENSION,
                        legacy,
                    ));
                });
            });
        }
        group.finish();
    }
}

fn bench_parse_reserve(c: &mut Criterion) {
    parse_dispatch_bench::assert_reserve_behavior();
    let typical =
        parse_dispatch_bench::fixture_with_charts(parse_dispatch_bench::TYPICAL_CHART_COUNT);
    let large = parse_dispatch_bench::fixture();
    let sm = parse_dispatch_bench::sm_fixture(parse_dispatch_bench::TYPICAL_CHART_COUNT);
    for (name, data, ext) in [
        ("parse_reserve_ssc_10_charts", typical.as_slice(), "ssc"),
        ("parse_reserve_ssc_128_charts", large.as_slice(), "ssc"),
        ("parse_reserve_sm_10_charts", sm.as_slice(), "sm"),
    ] {
        let mut group = c.benchmark_group(name);
        group.sample_size(100);
        group.measurement_time(Duration::from_secs(3));
        group.throughput(Throughput::Bytes(data.len() as u64));
        for (phase, legacy) in [("growing_vec", true), ("presized_vec", false)] {
            group.bench_function(phase, |b| {
                b.iter(|| {
                    black_box(parse_dispatch_bench::parse_reserved(
                        black_box(data),
                        ext,
                        legacy,
                    ));
                });
            });
        }
        group.finish();
    }
}

fn bench_parse_append(c: &mut Criterion) {
    let fixture = parse_dispatch_bench::fixture();
    parse_dispatch_bench::assert_append_behavior(&fixture, EXTENSION);
    let mut group = c.benchmark_group("parse_attack_append_128_charts");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(fixture.len() as u64));
    for (phase, legacy) in [("allocate_then_grow", true), ("presized_copy", false)] {
        group.bench_function(phase, |b| {
            b.iter(|| {
                black_box(parse_dispatch_bench::parse_append(
                    black_box(&fixture),
                    EXTENSION,
                    legacy,
                ));
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_metadata_pipeline,
    bench_chart_metadata_analysis,
    bench_selectable,
    bench_text_report,
    bench_chart_metadata_strings,
    bench_marker_translation,
    bench_parse_dispatch,
    bench_parse_reserve,
    bench_parse_append,
);
criterion_main!(benches);
