use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

const FIXTURE: &str = include_str!("fixtures/camellia_mix.ssc");
const EXTENSION: &str = "ssc";

fn bench_metadata_pipeline(c: &mut Criterion) {
    let fixture = FIXTURE.as_bytes();
    let timing_format = rssp::timing::TimingFormat::from_extension(EXTENSION);
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
                    rssp::parse::clean_tag(&rssp::parse::unescape_tag(
                        rssp::parse::decode_bytes(b).as_ref(),
                    ))
                })
                .unwrap_or_else(|| "<invalid-title>".to_string());
            let trimmed_title = title.trim();
            if trimmed_title.len() != title.len() {
                title = trimmed_title.to_string();
            }

            let mut subtitle = parsed
                .subtitle
                .map(|b| rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()))
                .unwrap_or_default();
            let trimmed_subtitle = subtitle.trim();
            if trimmed_subtitle.len() != subtitle.len() {
                subtitle = trimmed_subtitle.to_string();
            }

            let mut artist = parsed
                .artist
                .map(|b| rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()))
                .unwrap_or_default();
            let trimmed_artist = artist.trim();
            if trimmed_artist.len() != artist.len() {
                artist = trimmed_artist.to_string();
            }

            let title_translit = parsed
                .title_translit
                .map(|b| rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()))
                .unwrap_or_default();
            let subtitle_translit = parsed
                .subtitle_translit
                .map(|b| rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()))
                .unwrap_or_default();
            let mut artist_translit = parsed
                .artist_translit
                .map(|b| rssp::parse::unescape_tag(rssp::parse::decode_bytes(b).as_ref()))
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
                let (fields, _) = rssp::parse::split_notes_fields(&entry.notes);
                if fields.len() < 5 {
                    continue;
                }

                let step_type =
                    rssp::parse::unescape_trim(rssp::parse::decode_bytes(fields[0]).as_ref());
                if step_type == "lights-cabinet" {
                    continue;
                }
                let desc_raw =
                    rssp::parse::unescape_trim(rssp::parse::decode_bytes(fields[1]).as_ref());
                let description =
                    rssp::parse::normalize_chart_desc(desc_raw, timing_format, ssc_version);
                let difficulty =
                    rssp::parse::unescape_trim(rssp::parse::decode_bytes(fields[2]).as_ref());
                let meter =
                    rssp::parse::unescape_trim(rssp::parse::decode_bytes(fields[3]).as_ref());
                let credit =
                    rssp::parse::unescape_tag(rssp::parse::decode_bytes(fields[4]).as_ref());

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
        })
    });
    group.finish();
}

criterion_group!(benches, bench_metadata_pipeline);
criterion_main!(benches);
