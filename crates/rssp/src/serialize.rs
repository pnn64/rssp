use std::io;

use rssp_core::parse::extension_is_ssc;

use crate::{SimfileSummary, stats::RADAR_CATEGORY_COUNT};

const DEFAULT_BPMS: &str = "0.000=60.000";
const DEFAULT_TIME_SIGNATURES: &str = "0.000000=4=4";
const DEFAULT_SPEEDS: &str = "0.000000=1.000000=0.000000=0";
const DEFAULT_SCROLLS: &str = "0.000000=1.000000";

fn serialize_prop(key: &[u8], value: &[u8], out: &mut dyn io::Write) -> Result<usize, io::Error> {
    let written_bytes = out.write(b"#")?
        + out.write(key)?
        + out.write(b":")?
        + out.write(value)?
        + out.write(b";\n")?;
    Ok(written_bytes)
}

fn serialize_sm_chart_field(value: &str, out: &mut dyn io::Write) -> Result<usize, io::Error> {
    let written_bytes = out.write(b"     ")? + out.write(value.as_bytes())? + out.write(b":\n")?;
    Ok(written_bytes)
}

fn format_version(ssc_version: f32) -> String {
    format!("{:.2}", ssc_version)
}

fn format_f64(value: f64) -> String {
    format!("{:.6}", value)
}

fn format_f32(value: f32) -> String {
    format!("{:.3}", value)
}

fn format_radar_values(radar_values: Option<[f32; RADAR_CATEGORY_COUNT]>) -> String {
    match radar_values {
        Some(radar_values) => radar_values
            .iter()
            .map(|&f| format_f32(f))
            .collect::<Vec<String>>()
            .join(":"),
        None => String::from(""),
    }
}

pub fn serialize_simfile(
    summary: SimfileSummary,
    extension: &str,
    out: &mut dyn io::Write,
) -> io::Result<usize> {
    let ssc = extension_is_ssc(extension)?;

    let mut written_bytes = 0;

    {
        let properties: Vec<(&[u8], String, bool)> = vec![
            (b"VERSION", format_version(summary.ssc_version), ssc),
            (b"TITLE", summary.title_str, true),
            (b"SUBTITLE", summary.subtitle_str, true),
            (b"ARTIST", summary.artist_str, true),
            (b"TITLETRANSLIT", summary.titletranslit_str, true),
            (b"SUBTITLETRANSLIT", summary.subtitletranslit_str, true),
            (b"ARTISTTRANSLIT", summary.artisttranslit_str, true),
            (b"GENRE", summary.genre_str, true),
            (b"OFFSET", format_f64(summary.offset), true),
            // TODO: store attacks on SimfileSummary
            (b"ATTACKS", String::from(""), true),
            (b"BPMS", summary.normalized_bpms, true),
            (b"STOPS", summary.normalized_stops, true),
            (b"DELAYS", summary.normalized_delays, true),
            (b"TIMESIGNATURES", summary.normalized_time_signatures, true),
            (b"TICKCOUNTS", summary.normalized_tickcounts, true),
            (b"BANNER", summary.banner_path, true),
            (b"BACKGROUND", summary.background_path, true),
            (b"CDTITLE", summary.cdtitle_path, true),
            (b"JACKET", summary.jacket_path, true),
            (b"MUSIC", summary.music_path, true),
            (b"SAMPLESTART", format_f64(summary.sample_start), true),
            (b"SAMPLELENGTH", format_f64(summary.sample_length), true),
            (b"DISPLAYBPM", summary.display_bpm_str, true),
            (b"FAKES", summary.normalized_fakes, ssc),
            (b"WARPS", summary.normalized_warps, ssc),
            (b"SPEEDS", summary.normalized_speeds, ssc),
            (b"SCROLLS", summary.normalized_scrolls, ssc),
            (b"LABELS", summary.normalized_labels, ssc),
            (b"COMBOS", summary.normalized_combos, ssc),
        ];

        for property in properties {
            if property.2 {
                written_bytes += serialize_prop(property.0, property.1.as_bytes(), out)?;
            }
        }
    }

    match ssc {
        true => {
            for chart in summary.charts {
                {
                    let chart_properties: Vec<(&[u8], String)> = vec![
                        (b"NOTEDATA", String::from("")),
                        (b"CHARTNAME", chart.chart_name_str),
                        (b"STEPSTYPE", chart.step_type_str),
                        (b"DESCRIPTION", chart.description_str),
                        // TODO: store chart_style_str on ChartSummary
                        (b"CHARTSTYLE", String::from("")),
                        (b"DIFFICULTY", chart.difficulty_str),
                        (b"METER", chart.rating_str),
                        (
                            b"RADARVALUES",
                            format_radar_values(chart.cached_radar_values),
                        ),
                        (b"CREDIT", chart.step_artist_str),
                    ];

                    for property in chart_properties {
                        written_bytes += serialize_prop(property.0, property.1.as_bytes(), out)?;
                    }
                }

                if chart.chart_has_own_timing {
                    let chart_timing_properties: Vec<(&[u8], String)> = vec![
                        (b"OFFSET", format_f64(chart.chart_offset_seconds)),
                        (
                            b"BPMS",
                            chart.chart_bpms.unwrap_or(String::from(DEFAULT_BPMS)),
                        ),
                        (b"STOPS", chart.chart_stops.unwrap_or(String::from(""))),
                        (b"DELAYS", chart.chart_delays.unwrap_or(String::from(""))),
                        (b"WARPS", chart.chart_warps.unwrap_or(String::from(""))),
                        (
                            b"TIMESIGNATURES",
                            chart
                                .chart_time_signatures
                                .unwrap_or(String::from(DEFAULT_TIME_SIGNATURES)),
                        ),
                        (
                            b"TICKCOUNTS",
                            chart.chart_tickcounts.unwrap_or(String::from("")),
                        ),
                        (b"COMBOS", chart.chart_combos.unwrap_or(String::from(""))),
                        (
                            b"SPEEDS",
                            chart.chart_speeds.unwrap_or(String::from(DEFAULT_SPEEDS)),
                        ),
                        (
                            b"SCROLLS",
                            chart.chart_scrolls.unwrap_or(String::from(DEFAULT_SCROLLS)),
                        ),
                        (b"FAKES", chart.chart_fakes.unwrap_or(String::from(""))),
                        (b"LABELS", chart.chart_labels.unwrap_or(String::from(""))),
                        (b"ATTACKS", chart.chart_attacks.unwrap_or(String::from(""))),
                        (
                            b"DISPLAYBPM",
                            chart.chart_display_bpm.unwrap_or(String::from("")),
                        ),
                    ];

                    for property in chart_timing_properties {
                        written_bytes += serialize_prop(property.0, property.1.as_bytes(), out)?;
                    }
                }

                // TODO: handle #NOTES2 correctly
                written_bytes += out.write(b"#NOTES:\n")?;
                written_bytes += out.write(&chart.minimized_note_data)?;
                written_bytes += out.write(b";\n")?;
            }
        }
        // SM
        false => {
            for chart in summary.charts {
                written_bytes += out.write(b"#NOTES:\n")?;
                written_bytes += serialize_sm_chart_field(&chart.step_type_str, out)?;
                written_bytes += serialize_sm_chart_field(&chart.description_str, out)?;
                written_bytes += serialize_sm_chart_field(&chart.difficulty_str, out)?;
                written_bytes += serialize_sm_chart_field(&chart.rating_str, out)?;
                // TODO: verify whether this actually mirrors the "description" field
                written_bytes +=
                    serialize_sm_chart_field(&format_radar_values(chart.cached_radar_values), out)?;
                written_bytes += out.write(&chart.minimized_note_data)?;
                written_bytes += out.write(b";\n")?;
            }
        }
    }

    Ok(written_bytes)
}
