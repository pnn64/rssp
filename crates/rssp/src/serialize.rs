use std::io;

use rssp_core::parse::extension_is_ssc;

use crate::{SimfileSummary, stats::RADAR_CATEGORY_COUNT};

const DEFAULT_BPMS: &str = "0.000=60.000";
const DEFAULT_TIME_SIGNATURES: &str = "0.000000=4=4";
const DEFAULT_SPEEDS: &str = "0.000000=1.000000=0.000000=0";
const DEFAULT_SCROLLS: &str = "0.000000=1.000000";

enum ValueWriter {
    Plain,
    List,
}

macro_rules! write_all {
    ($out:expr, $value:expr) => {
        $out.write_all($value).map(|_| $value.len())
    };
}

fn write_prop(
    key: &[u8],
    value: &[u8],
    writer: ValueWriter,
    out: &mut dyn io::Write,
) -> Result<usize, io::Error> {
    let mut written_bytes = 0;
    written_bytes += write_all!(out, b"#")?;
    written_bytes += write_all!(out, key)?;
    written_bytes += write_all!(out, b":")?;
    written_bytes += match writer {
        ValueWriter::Plain => write_all!(out, value)?,
        ValueWriter::List => write_comma_separated_list(out, value)?,
    };
    written_bytes += write_all!(out, b";\n")?;
    Ok(written_bytes)
}

fn write_comma_separated_list(out: &mut dyn io::Write, value: &[u8]) -> io::Result<usize> {
    let mut written_bytes = 0;
    let mut start = 0;

    while let Some(offset) = value[start..].iter().position(|&b| b == b',') {
        let comma = start + offset;
        written_bytes += write_all!(out, &value[start..=comma])?;
        written_bytes += write_all!(out, b"\n")?;
        start = comma + 1;
    }

    written_bytes += write_all!(out, &value[start..])?;
    Ok(written_bytes)
}

fn write_sm_chart_field(value: &str, out: &mut dyn io::Write) -> Result<usize, io::Error> {
    let written_bytes =
        write_all!(out, b"     ")? + out.write(value.as_bytes())? + write_all!(out, b":\n")?;
    Ok(written_bytes)
}

#[inline(always)]
fn format_version(ssc_version: f32) -> String {
    format!("{:.2}", ssc_version)
}

#[inline(always)]
fn format_dot6_f64(value: f64) -> String {
    format!("{:.6}", value)
}

#[inline(always)]
fn format_dot6_f32(value: f32) -> String {
    format!("{:.6}", value)
}

fn format_radar_values(radar_values: Option<[f32; RADAR_CATEGORY_COUNT]>) -> String {
    match radar_values {
        Some(radar_values) => radar_values
            .iter()
            .map(|&f| format_dot6_f32(f))
            .collect::<Vec<String>>()
            .join(","),
        None => String::from(""),
    }
}

pub fn serialize_simfile(
    summary: &SimfileSummary,
    extension: &str,
    out: &mut dyn io::Write,
) -> io::Result<usize> {
    use ValueWriter::*;

    let ssc = extension_is_ssc(extension)?;

    let mut written_bytes = 0;

    let formatted_version = format_version(summary.ssc_version);
    let formatted_offset = format_dot6_f64(summary.offset);
    let formatted_sample_start = format_dot6_f64(summary.sample_start);
    let formatted_sample_length = format_dot6_f64(summary.sample_length);

    // (key, value, is_included, value_writer)
    let properties: Vec<(&[u8], &str, bool, ValueWriter)> = vec![
        (b"VERSION", &formatted_version, ssc, Plain),
        (b"TITLE", &summary.title_str, true, Plain),
        (b"SUBTITLE", &summary.subtitle_str, true, Plain),
        (b"ARTIST", &summary.artist_str, true, Plain),
        (b"TITLETRANSLIT", &summary.titletranslit_str, true, Plain),
        (
            b"SUBTITLETRANSLIT",
            &summary.subtitletranslit_str,
            true,
            Plain,
        ),
        (b"ARTISTTRANSLIT", &summary.artisttranslit_str, true, Plain),
        (b"GENRE", &summary.genre_str, true, Plain),
        (b"ORIGIN", "", true, Plain), // TODO
        (b"CREDIT", "", true, Plain), // TODO
        (b"BANNER", &summary.banner_path, true, Plain),
        (b"BACKGROUND", &summary.background_path, true, Plain),
        (b"PREVIEWVID", "", ssc, Plain), // TODO
        (b"JACKET", &summary.jacket_path, ssc, Plain),
        (b"CDIMAGE", "", ssc, Plain),     // TODO
        (b"DISCIMAGE", "", ssc, Plain),   // TODO
        (b"LYRICSPATH", "", true, Plain), // TODO
        (b"CDTITLE", &summary.cdtitle_path, true, Plain),
        (b"MUSIC", &summary.music_path, true, Plain),
        (b"OFFSET", &formatted_offset, true, Plain),
        (b"SAMPLESTART", &formatted_sample_start, true, Plain),
        (b"SAMPLELENGTH", &formatted_sample_length, true, Plain),
        (b"SELECTABLE", "YES", true, Plain), // TODO
        (
            b"DISPLAYBPM",
            &summary.display_bpm_str,
            !summary.display_bpm_str.is_empty(),
            Plain,
        ),
        (b"BPMS", &summary.normalized_bpms, true, List),
        (b"STOPS", &summary.normalized_stops, true, List),
        (b"DELAYS", &summary.normalized_delays, ssc, List),
        (b"WARPS", &summary.normalized_warps, ssc, List),
        (
            b"TIMESIGNATURES",
            &summary.normalized_time_signatures,
            ssc,
            List,
        ),
        (b"TICKCOUNTS", &summary.normalized_tickcounts, ssc, List),
        (b"COMBOS", &summary.normalized_combos, ssc, List),
        (b"SPEEDS", &summary.normalized_speeds, ssc, List),
        (b"SCROLLS", &summary.normalized_scrolls, ssc, List),
        (b"FAKES", &summary.normalized_fakes, ssc, List),
        (b"LABELS", &summary.normalized_labels, ssc, List),
        (b"BGCHANGES", "", true, List), // TODO
        (b"KEYSOUNDS", "", true, List), // TODO
        (b"ATTACKS", "", true, List),   // TODO
    ];

    for property in properties {
        if property.2 {
            written_bytes += write_prop(property.0, property.1.as_bytes(), property.3, out)?;
        }
    }

    written_bytes += write_all!(out, b"\n")?;

    match ssc {
        true => {
            for chart in &summary.charts {
                written_bytes += serialize_ssc_chart(out, chart)?;
                written_bytes += write_all!(out, b"\n")?;
            }
        }
        false => {
            for chart in &summary.charts {
                written_bytes += serialize_sm_chart(out, chart)?;
                written_bytes += write_all!(out, b"\n")?;
            }
        }
    }

    Ok(written_bytes)
}

fn serialize_sm_chart(
    out: &mut dyn io::Write,
    chart: &crate::ChartSummary,
) -> Result<usize, io::Error> {
    let mut written_bytes = 0;
    written_bytes += write_all!(out, b"#NOTES:\n")?;
    written_bytes += write_sm_chart_field(&chart.step_type_str, out)?;
    written_bytes += write_sm_chart_field(&chart.description_str, out)?;
    written_bytes += write_sm_chart_field(&chart.difficulty_str, out)?;
    written_bytes += write_sm_chart_field(&chart.rating_str, out)?;
    written_bytes += write_sm_chart_field(&format_radar_values(chart.cached_radar_values), out)?;
    written_bytes += write_all!(out, &chart.minimized_note_data)?;
    written_bytes += write_all!(out, b";\n")?;
    Ok(written_bytes)
}

fn serialize_ssc_chart(
    out: &mut dyn io::Write,
    chart: &crate::ChartSummary,
) -> Result<usize, io::Error> {
    let mut written_bytes = 0;
    let formatted_radar_values = format_radar_values(chart.cached_radar_values);
    let chart_properties: Vec<(&[u8], &str)> = vec![
        (b"NOTEDATA", ""),
        (b"CHARTNAME", &chart.chart_name_str),
        (b"STEPSTYPE", &chart.step_type_str),
        (b"DESCRIPTION", &chart.description_str),
        // TODO: store chart_style_str on ChartSummary
        (b"CHARTSTYLE", ""),
        (b"DIFFICULTY", &chart.difficulty_str),
        (b"METER", &chart.rating_str),
        (b"RADARVALUES", &formatted_radar_values),
        (b"CREDIT", &chart.step_artist_str),
    ];

    for property in chart_properties {
        written_bytes += write_prop(property.0, property.1.as_bytes(), ValueWriter::Plain, out)?;
    }

    if chart.chart_has_own_timing {
        written_bytes += serialize_ssc_chart_timing_fields(out, chart)?;
    }

    written_bytes += write_all!(out, b"#NOTES:\n")?;
    written_bytes += write_all!(out, &chart.minimized_note_data)?;
    written_bytes += write_all!(out, b";\n")?;

    Ok(written_bytes)
}

fn serialize_ssc_chart_timing_fields(
    out: &mut dyn io::Write,
    chart: &crate::ChartSummary,
) -> Result<usize, io::Error> {
    use ValueWriter::*;

    let mut written_bytes = 0;
    let formatted_chart_offset = format_dot6_f64(chart.chart_offset_seconds);
    let chart_timing_properties: Vec<(&[u8], &str, ValueWriter, bool)> = vec![
        (b"OFFSET", &formatted_chart_offset, Plain, true),
        (
            b"BPMS",
            &chart.chart_bpms.as_deref().unwrap_or_else(|| DEFAULT_BPMS),
            List,
            true,
        ),
        (
            b"STOPS",
            chart.chart_stops.as_deref().unwrap_or_default(),
            List,
            true,
        ),
        (
            b"DELAYS",
            chart.chart_delays.as_deref().unwrap_or_default(),
            List,
            true,
        ),
        (
            b"WARPS",
            chart.chart_warps.as_deref().unwrap_or_default(),
            List,
            true,
        ),
        (
            b"TIMESIGNATURES",
            chart
                .chart_time_signatures
                .as_deref()
                .unwrap_or_else(|| DEFAULT_TIME_SIGNATURES),
            List,
            true,
        ),
        (
            b"TICKCOUNTS",
            chart.chart_tickcounts.as_deref().unwrap_or_default(),
            List,
            true,
        ),
        (
            b"COMBOS",
            chart.chart_combos.as_deref().unwrap_or_default(),
            List,
            true,
        ),
        (
            b"SPEEDS",
            chart
                .chart_speeds
                .as_deref()
                .unwrap_or_else(|| DEFAULT_SPEEDS),
            List,
            true,
        ),
        (
            b"SCROLLS",
            chart
                .chart_scrolls
                .as_deref()
                .unwrap_or_else(|| DEFAULT_SCROLLS),
            List,
            true,
        ),
        (
            b"FAKES",
            chart.chart_fakes.as_deref().unwrap_or_default(),
            List,
            true,
        ),
        (
            b"LABELS",
            chart.chart_labels.as_deref().unwrap_or_default(),
            List,
            true,
        ),
        (
            b"ATTACKS",
            chart.chart_attacks.as_deref().unwrap_or_default(),
            List,
            !chart.chart_attacks.as_ref().is_none_or(|v| v.is_empty()),
        ),
        (
            b"DISPLAYBPM",
            chart.chart_display_bpm.as_deref().unwrap_or_default(),
            Plain,
            !chart
                .chart_display_bpm
                .as_ref()
                .is_none_or(|v| v.is_empty()),
        ),
    ];

    for property in chart_timing_properties {
        if property.3 {
            written_bytes += write_prop(property.0, property.1.as_bytes(), property.2, out)?;
        }
    }

    Ok(written_bytes)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use std::io;
    use std::sync::Arc;

    #[test]
    fn serialize_simfile_with_ssc() -> io::Result<()> {
        let mut summary = simfile_summary_with_all_fields();
        summary.charts.push(chart_summary_with_all_fields(false));
        summary.charts.push(chart_summary_with_all_fields(true));
        let mut buffer = vec![];
        {
            let mut cursor = io::Cursor::new(&mut buffer);
            super::serialize_simfile(&summary, "ssc", &mut cursor)?;
            cursor.get_ref()
        };

        let output = String::from_utf8(buffer).unwrap();

        let expected = "#VERSION:0.83;\n\
            #TITLE:Title;\n\
            #SUBTITLE:Subtitle;\n\
            #ARTIST:Artist;\n\
            #TITLETRANSLIT:Title translit;\n\
            #SUBTITLETRANSLIT:Subtitle translit;\n\
            #ARTISTTRANSLIT:Artist translit;\n\
            #GENRE:Genre;\n\
            #ORIGIN:;\n\
            #CREDIT:;\n\
            #BANNER:banner.png;\n\
            #BACKGROUND:background.png;\n\
            #PREVIEWVID:;\n\
            #JACKET:jacket.png;\n\
            #CDIMAGE:;\n\
            #DISCIMAGE:;\n\
            #LYRICSPATH:;\n\
            #CDTITLE:cdtitle.png;\n\
            #MUSIC:music.ogg;\n\
            #OFFSET:0.123000;\n\
            #SAMPLESTART:10.000000;\n\
            #SAMPLELENGTH:16.000000;\n\
            #SELECTABLE:YES;\n\
            #DISPLAYBPM:150;\n\
            #BPMS:0.000=120.000,\n\
            16.000=240.000,\n\
            48.000=120.000;\n\
            #STOPS:1.000=1.250,\n\
            1.500=1.750;\n\
            #DELAYS:2.000=2.250,\n\
            2.500=2.750;\n\
            #WARPS:3.000=3.250,\n\
            3.500=3.750;\n\
            #TIMESIGNATURES:0.000=4=4,\n\
            16.000=8=4,\n\
            48.000=4=4;\n\
            #TICKCOUNTS:0.000=4,\n\
            16.000=2,\n\
            48.000=4;\n\
            #COMBOS:0.000=1,\n\
            16.000=2,\n\
            48.000=1;\n\
            #SPEEDS:0.000=1.000=0.000=0,\n\
            12.000=0.500=4.000=0,\n\
            48.000=1.000=0.000=1;\n\
            #SCROLLS:0.000=1.000,\n\
            16.000=2.000,\n\
            48.000=1.000;\n\
            #FAKES:4.000=4.250,\n\
            4.500=4.750;\n\
            #LABELS:0.000=Song Start,\n\
            16.000=Speedup;\n\
            #BGCHANGES:;\n\
            #KEYSOUNDS:;\n\
            #ATTACKS:;\n\
            \n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #RADARVALUES:0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000;\n\
            #CREDIT:Step artist;\n\
            #NOTES:\n\
            ;\n\
            \n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #RADARVALUES:0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000;\n\
            #CREDIT:Step artist;\n\
            #OFFSET:0.000000;\n\
            #BPMS:0.000=120.000,\n\
            16.000=240.000,\n\
            48.000=120.000;\n\
            #STOPS:1.000=1.250,\n\
            1.500=1.750;\n\
            #DELAYS:2.000=2.250,\n\
            2.500=2.750;\n\
            #WARPS:3.000=3.250,\n\
            3.500=3.750;\n\
            #TIMESIGNATURES:0.000=4=4,\n\
            16.000=8=4,\n\
            48.000=4=4;\n\
            #TICKCOUNTS:0.000=4,\n\
            16.000=2,\n\
            48.000=4;\n\
            #COMBOS:0.000=1,\n\
            16.000=2,\n\
            48.000=1;\n\
            #SPEEDS:0.000=1.000=0.000=0,\n\
            12.000=0.500=4.000=0,\n\
            48.000=1.000=0.000=1;\n\
            #SCROLLS:0.000=1.000,\n\
            16.000=2.000,\n\
            48.000=1.000;\n\
            #FAKES:4.000=4.250,\n\
            4.500=4.750;\n\
            #LABELS:0.000=Song Start,\n\
            16.000=Speedup;\n\
            #DISPLAYBPM:150;\n\
            #NOTES:\n\
            ;\n\
            \n";

        assert_eq!(expected, output);

        Ok(())
    }

    fn simfile_summary_with_all_fields() -> crate::SimfileSummary {
        crate::SimfileSummary {
            title_str: String::from("Title"),
            subtitle_str: String::from("Subtitle"),
            artist_str: String::from("Artist"),
            genre_str: String::from("Genre"),
            titletranslit_str: String::from("Title translit"),
            subtitletranslit_str: String::from("Subtitle translit"),
            artisttranslit_str: String::from("Artist translit"),
            offset: 0.123,
            normalized_bpms: String::from("0.000=120.000,16.000=240.000,48.000=120.000"),
            normalized_stops: String::from("1.000=1.250,1.500=1.750"),
            normalized_delays: String::from("2.000=2.250,2.500=2.750"),
            normalized_warps: String::from("3.000=3.250,3.500=3.750"),
            normalized_speeds: String::from(
                "0.000=1.000=0.000=0,12.000=0.500=4.000=0,48.000=1.000=0.000=1",
            ),
            normalized_scrolls: String::from("0.000=1.000,16.000=2.000,48.000=1.000"),
            normalized_fakes: String::from("4.000=4.250,4.500=4.750"),
            normalized_time_signatures: String::from("0.000=4=4,16.000=8=4,48.000=4=4"),
            normalized_labels: String::from("0.000=Song Start,16.000=Speedup"),
            normalized_tickcounts: String::from("0.000=4,16.000=2,48.000=4"),
            normalized_combos: String::from("0.000=1,16.000=2,48.000=1"),
            ssc_version: 0.83,
            timing_format: rssp_core::timing::TimingFormat::Ssc,
            banner_path: String::from("banner.png"),
            background_path: String::from("background.png"),
            cdtitle_path: String::from("cdtitle.png"),
            jacket_path: String::from("jacket.png"),
            music_path: String::from("music.ogg"),
            display_bpm_str: String::from("150"),
            sample_start: 10.0,
            sample_length: 16.0,

            // To be populated by the test
            charts: vec![],

            // The remaining fields are irrelevant to serialization
            min_bpm: Default::default(),
            max_bpm: Default::default(),
            median_bpm: Default::default(),
            average_bpm: Default::default(),
            total_length: Default::default(),
            pattern_counts_enabled: Default::default(),
            tech_counts_enabled: Default::default(),
            total_elapsed: Default::default(),
        }
    }

    fn chart_summary_with_all_fields(has_own_timing: bool) -> crate::ChartSummary {
        crate::ChartSummary {
            step_type_str: String::from("dance-single"),
            step_artist_str: String::from("Step artist"),
            description_str: String::from("Description"),
            chart_name_str: String::from("Chart name"),
            difficulty_str: String::from("Challenge"),
            rating_str: String::from("17"),
            cached_radar_values: Some([
                0.010, 0.020, 0.030, 0.040, 0.050, 0.060, 0.070, 0.080, 0.090, 0.100, 0.110, 0.120,
                0.130, 0.140,
            ]),
            tech_notation_str: String::from("BR FS XO"),
            minimized_note_data: Default::default(), // TODO

            // Timing fields
            chart_has_own_timing: has_own_timing,
            music_path: match has_own_timing {
                true => String::from("music.ogg"),
                false => String::from(""),
            },
            chart_attacks: Default::default(), // TODO
            chart_bpms: has_own_timing
                .then(|| String::from("0.000=120.000,16.000=240.000,48.000=120.000")),
            chart_stops: has_own_timing.then(|| String::from("1.000=1.250,1.500=1.750")),
            chart_delays: has_own_timing.then(|| String::from("2.000=2.250,2.500=2.750")),
            chart_warps: has_own_timing.then(|| String::from("3.000=3.250,3.500=3.750")),
            chart_speeds: has_own_timing.then(|| {
                String::from("0.000=1.000=0.000=0,12.000=0.500=4.000=0,48.000=1.000=0.000=1")
            }),
            chart_scrolls: has_own_timing
                .then(|| String::from("0.000=1.000,16.000=2.000,48.000=1.000")),
            chart_fakes: has_own_timing.then(|| String::from("4.000=4.250,4.500=4.750")),
            chart_time_signatures: has_own_timing
                .then(|| String::from("0.000=4=4,16.000=8=4,48.000=4=4")),
            chart_labels: has_own_timing.then(|| String::from("0.000=Song Start,16.000=Speedup")),
            chart_tickcounts: has_own_timing.then(|| String::from("0.000=4,16.000=2,48.000=4")),
            chart_combos: has_own_timing.then(|| String::from("0.000=1,16.000=2,48.000=1")),
            chart_display_bpm: has_own_timing.then(|| String::from("150")),

            // The remaining fields are irrelevant to serialization
            timing_segments: Arc::new(crate::timing::TimingSegments {
                beat0_offset_adjust: Default::default(),
                bpms: Default::default(),
                stops: Default::default(),
                delays: Default::default(),
                warps: Default::default(),
                speeds: Default::default(),
                scrolls: Default::default(),
                fakes: Default::default(),
            }),
            matrix_rating: Default::default(),
            tier_bpm: Default::default(),
            stats: Default::default(),
            stream_counts: Default::default(),
            total_measures: Default::default(),
            total_streams: Default::default(),
            mines_nonfake: Default::default(),
            sn_detailed_breakdown: Default::default(),
            sn_partial_breakdown: Default::default(),
            sn_simple_breakdown: Default::default(),
            detailed_breakdown: Default::default(),
            partial_breakdown: Default::default(),
            simple_breakdown: Default::default(),
            max_nps: Default::default(),
            median_nps: Default::default(),
            duration_seconds: Default::default(),
            detected_patterns: [Default::default(); rssp_core::patterns::PATTERN_COUNT],
            anchor_left: Default::default(),
            anchor_down: Default::default(),
            anchor_up: Default::default(),
            anchor_right: Default::default(),
            facing_left: Default::default(),
            facing_right: Default::default(),
            mono_total: Default::default(),
            mono_percent: Default::default(),
            candle_total: Default::default(),
            candle_percent: Default::default(),
            tech_counts: Default::default(),
            note_annotations: Default::default(),
            custom_patterns: Default::default(),
            short_hash: Default::default(),
            bpm_neutral_hash: Default::default(),
            elapsed: Default::default(),
            measure_densities: Default::default(),
            measure_nps_vec: Default::default(),
            row_to_beat: Default::default(),
            chart_offset_seconds: Default::default(),
        }
    }
}
