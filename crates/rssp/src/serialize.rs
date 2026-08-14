use std::{borrow::Cow, io};

use rssp_core::{
    parse::extension_is_ssc,
    timing::{SpeedUnit, convert_warps_and_delays_to_sm_stops},
};

pub const DEFAULT_VERSION: &[u8] = b"0.83";
pub const DEFAULT_TITLE: &[u8] = b"Untitled";
pub const DEFAULT_ARTIST: &[u8] = b"Unknown artist";
pub const DEFAULT_BPMS: &[u8] = b"0.000000=60.000000";
pub const DEFAULT_TIME_SIGNATURES: &[u8] = b"0.000000=4=4";
pub const DEFAULT_TICKCOUNTS: &[u8] = b"0.000000=4";
pub const DEFAULT_COMBOS: &[u8] = b"0.000000=1";
pub const DEFAULT_SPEEDS: &[u8] = b"0.000000=1.000000=0.000000=0";
pub const DEFAULT_SCROLLS: &[u8] = b"0.000000=1.000000";
pub const DEFAULT_LABELS: &[u8] = b"0.000000=Song Start";
pub const DEFAULT_STEPSTYPE: &[u8] = b"dance-single";
pub const DEFAULT_DIFFICULTY: &[u8] = b"Beginner";
pub const DEFAULT_METER: &[u8] = b"1";

/// Call `write_all` and, on success, return the input's size in bytes
macro_rules! write_all {
    ($out:expr, $value:expr) => {
        $out.write_all($value).map(|_| $value.len())
    };
}

#[must_use]
fn sm_escape(out: &mut dyn io::Write, bytes: &[u8]) -> io::Result<usize> {
    let mut written_bytes = 0;
    let mut bytes_iter = bytes.iter().peekable();

    while let Some(&byte) = bytes_iter.next() {
        if byte == b'/' && bytes_iter.peek().is_some_and(|&b| *b == b'/') {
            written_bytes += write_all!(out, b"\\/\\/")?;
            bytes_iter.next();
            continue;
        }
        if byte == b'\\' || byte == b':' || byte == b';' {
            written_bytes += write_all!(out, b"\\")?;
        }
        written_bytes += write_all!(out, &[byte])?;
    }

    Ok(written_bytes)
}

#[must_use]
#[inline(always)]
fn format_version(ssc_version: f32) -> String {
    format!("{:.2}", ssc_version)
}

#[must_use]
#[inline(always)]
fn format_dot6_f64(value: f64) -> String {
    format!("{:.6}", value)
}

#[must_use]
#[inline(always)]
fn format_dot6_f32(value: f32) -> String {
    format!("{:.6}", value)
}

#[derive(Default)]
enum PropValue<'a> {
    #[default]
    Empty,
    Str(&'a str),
    StrNoEscape(&'a str),
    StrNoEscapeOpt(Option<&'a str>),
    Bytes(&'a [u8]),
    NoteData(&'a [u8]),
    Version(f32),
    Number(f64),
    NumberOpt(Option<f64>),
    Bool(bool),
    TimingPairs(&'a [(f32, f32)]),
    TimingSpeeds(&'a [(f32, f32, f32, SpeedUnit)]),
    RadarValues(Option<[f32; crate::stats::RADAR_CATEGORY_COUNT]>, bool),
}

impl<'a> PropValue<'a> {
    #[must_use]
    fn serialize(&self, out: &mut dyn io::Write) -> io::Result<usize> {
        match self {
            PropValue::Empty => Ok(0),
            PropValue::Str(s) => sm_escape(out, s.as_bytes()),
            PropValue::StrNoEscape(s) => write_all!(out, s.as_bytes()),
            PropValue::StrNoEscapeOpt(opt) => match opt {
                None => Ok(0),
                Some(s) => PropValue::StrNoEscape(*s).serialize(out),
            },
            PropValue::Bytes(b) => write_all!(out, b),
            PropValue::NoteData(b) => Ok(write_all!(out, b"\n")? + write_all!(out, b)?),
            PropValue::Version(v) => write_all!(out, format_version(*v).as_bytes()),
            PropValue::Number(n) => write_all!(out, format_dot6_f64(*n).as_bytes()),
            PropValue::NumberOpt(opt) => match opt {
                None => Ok(0),
                Some(n) => PropValue::Number(*n).serialize(out),
            },
            PropValue::Bool(b) => {
                if *b {
                    write_all!(out, b"YES")
                } else {
                    write_all!(out, b"NO")
                }
            }
            PropValue::RadarValues(rv, per_player) => match rv {
                Some(values) => {
                    let mut written_bytes = 0;
                    let mut first_item = true;
                    let players = if *per_player { 2 } else { 1 };

                    for _ in 0..players {
                        for value in values {
                            if !first_item {
                                written_bytes += write_all!(out, b",")?;
                            }
                            written_bytes += write_all!(out, format_dot6_f32(*value).as_bytes())?;
                            first_item = false;
                        }
                    }

                    Ok(written_bytes)
                }
                None => Ok(0),
            },
            PropValue::TimingPairs(items) => {
                let mut written_bytes = 0;
                let mut first_item = true;

                for (beat, value) in items.iter() {
                    if !first_item {
                        written_bytes += write_all!(out, b",\n")?;
                    }
                    written_bytes += write_all!(out, format_dot6_f32(*beat).as_bytes())?;
                    written_bytes += write_all!(out, b"=")?;
                    written_bytes += write_all!(out, format_dot6_f32(*value).as_bytes())?;
                    first_item = false;
                }

                Ok(written_bytes)
            }
            PropValue::TimingSpeeds(items) => {
                let mut written_bytes = 0;
                let mut first_item = true;

                for (beat, ratio, delay, unit) in items.iter() {
                    if !first_item {
                        written_bytes += write_all!(out, b",\n")?;
                    }
                    written_bytes += write_all!(out, format_dot6_f32(*beat).as_bytes())?;
                    written_bytes += write_all!(out, b"=")?;
                    written_bytes += write_all!(out, format_dot6_f32(*ratio).as_bytes())?;
                    written_bytes += write_all!(out, b"=")?;
                    written_bytes += write_all!(out, format_dot6_f32(*delay).as_bytes())?;
                    written_bytes += write_all!(out, b"=")?;
                    written_bytes += write_all!(
                        out,
                        match unit {
                            SpeedUnit::Seconds => b"1",
                            SpeedUnit::Beats => b"0",
                        }
                    )?;
                    first_item = false;
                }

                Ok(written_bytes)
            }
        }
    }

    #[must_use]
    fn is_empty(&self) -> bool {
        match self {
            PropValue::Empty => true,
            PropValue::Str(s) => s.is_empty(),
            PropValue::StrNoEscape(s) => s.is_empty(),
            PropValue::StrNoEscapeOpt(opt) => opt.as_ref().is_none_or(|s| s.is_empty()),
            PropValue::Bytes(b) => b.is_empty(),
            PropValue::NoteData(_) => false,
            PropValue::Version(v) => !v.is_finite(),
            PropValue::Number(_) => false,
            PropValue::NumberOpt(opt) => opt.is_none(),
            PropValue::Bool(_) => false,
            PropValue::TimingPairs(items) => items.is_empty(),
            PropValue::TimingSpeeds(items) => items.is_empty(),
            PropValue::RadarValues(rv, _) => rv.is_none(),
        }
    }
}

#[derive(Default)]
struct Prop<'a> {
    key: &'a [u8],
    value: PropValue<'a>,
    default_value: Option<&'a [u8]>,
    ssc_only: bool,
    nonempty_value_only: bool,
    own_timing_only: bool,
}

impl<'a> Prop<'a> {
    #[must_use]
    #[inline(always)]
    fn new(key: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        Prop {
            key,
            value,
            ..Default::default()
        }
    }

    fn new_with_default(key: &'a [u8], default: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        let mut prop = Prop::new(key, value);
        prop.default_value = Some(default);
        prop
    }

    #[must_use]
    fn ssc_only(key: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        let mut prop = Prop::new(key, value);
        prop.ssc_only = true;
        prop
    }

    #[must_use]
    fn ssc_nonempty_only(key: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        let mut prop = Prop::ssc_only(key, value);
        prop.nonempty_value_only = true;
        prop
    }

    #[must_use]
    fn ssc_only_with_default(key: &'a [u8], default: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        let mut prop = Prop::ssc_only(key, value);
        prop.default_value = Some(default);
        prop
    }

    #[must_use]
    fn nonempty_only(key: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        let mut prop = Prop::new(key, value);
        prop.nonempty_value_only = true;
        prop
    }

    #[must_use]
    fn own_timing_only(key: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        let mut prop = Prop::new(key, value);
        prop.own_timing_only = true;
        prop
    }

    #[must_use]
    fn own_timing_only_with_default(
        key: &'a [u8],
        default: &'a [u8],
        value: PropValue<'a>,
    ) -> Prop<'a> {
        let mut prop = Prop::own_timing_only(key, value);
        prop.default_value = Some(default);
        prop
    }

    #[must_use]
    #[inline(always)]
    fn start_prop(out: &mut dyn io::Write, key: &[u8]) -> io::Result<usize> {
        let mut written_bytes = 0;
        written_bytes += write_all!(out, b"#")?;
        written_bytes += write_all!(out, key)?;
        written_bytes += write_all!(out, b":")?;
        Ok(written_bytes)
    }

    #[must_use]
    #[inline(always)]
    fn end_prop(out: &mut dyn io::Write) -> io::Result<usize> {
        Ok(write_all!(out, b";\n")?)
    }

    #[must_use]
    fn serialize(self, ssc: bool, own_timing: bool, out: &mut dyn io::Write) -> io::Result<usize> {
        if self.ssc_only && !ssc {
            Ok(0)
        } else if self.nonempty_value_only && self.value.is_empty() {
            Ok(0)
        } else if self.own_timing_only && !own_timing {
            Ok(0)
        } else {
            let value = match (self.default_value, self.value.is_empty()) {
                (Some(default), true) => PropValue::Bytes(default),
                _ => self.value,
            };
            let mut written_bytes = 0;
            written_bytes += Prop::start_prop(out, self.key)?;
            written_bytes += value.serialize(out)?;
            written_bytes += Prop::end_prop(out)?;
            Ok(written_bytes)
        }
    }
}

#[must_use]
pub fn serialize_simfile(
    summary: &crate::SimfileSummary,
    extension: &str,
    out: &mut dyn io::Write,
) -> io::Result<usize> {
    let ssc = extension_is_ssc(extension)?;
    if !ssc {
        for chart in &summary.charts {
            if chart.chart_has_own_timing {
                return io::Result::Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Can't serialize chart with its own timing to .sm",
                ));
            }
        }
    }

    let stops = if ssc {
        Cow::Borrowed(summary.global_timing_segments.stops.as_slice())
    } else {
        convert_warps_and_delays_to_sm_stops(
            &summary.global_timing_segments.bpms,
            &summary.global_timing_segments.stops,
            &summary.global_timing_segments.delays,
            &summary.global_timing_segments.warps,
        )
    };

    let mut written_bytes = 0;

    #[rustfmt::skip]
    let props = [
        Prop::ssc_nonempty_only(b"VERSION", PropValue::Version(summary.ssc_version)),
        Prop::new_with_default(b"TITLE", DEFAULT_TITLE, PropValue::Str(&summary.title_str)),
        Prop::new(b"SUBTITLE", PropValue::Str(&summary.subtitle_str)),
        Prop::new_with_default(b"ARTIST", DEFAULT_ARTIST, PropValue::Str(&summary.artist_str)),
        Prop::new(b"TITLETRANSLIT", PropValue::Str(&summary.titletranslit_str)),
        Prop::new(b"SUBTITLETRANSLIT", PropValue::Str(&summary.subtitletranslit_str)),
        Prop::new(b"ARTISTTRANSLIT", PropValue::Str(&summary.artisttranslit_str)),
        Prop::new(b"GENRE", PropValue::Str(&summary.genre_str)),
        Prop::ssc_only(b"ORIGIN", PropValue::Str(&summary.origin_str)),
        Prop::new(b"CREDIT", PropValue::Str(&summary.credit_str)),
        Prop::new(b"BANNER", PropValue::Str(&summary.banner_path)),
        Prop::new(b"BACKGROUND", PropValue::Str(&summary.background_path)),
        Prop::ssc_only(b"PREVIEWVID", PropValue::Str(&summary.previewvid_path)),
        Prop::ssc_only(b"JACKET", PropValue::Str(&summary.jacket_path)),
        Prop::ssc_only(b"CDIMAGE", PropValue::Str(&summary.cdimage_path)),
        Prop::ssc_only(b"DISCIMAGE", PropValue::Str(&summary.discimage_path)),
        Prop::new(b"LYRICSPATH", PropValue::Str(&summary.lyrics_path)),
        Prop::new(b"CDTITLE", PropValue::Str(&summary.cdtitle_path)),
        Prop::new(b"MUSIC", PropValue::Str(&summary.music_path)),
        Prop::new(b"OFFSET", PropValue::Number(summary.offset)),
        Prop::new(b"SAMPLESTART", PropValue::Number(summary.sample_start)),
        Prop::new(b"SAMPLELENGTH", PropValue::Number(summary.sample_length)),
        Prop::new(b"SELECTABLE", PropValue::Bool(summary.selectable)),
        Prop::nonempty_only(b"DISPLAYBPM", PropValue::StrNoEscape(&summary.display_bpm_str)),
        Prop::new_with_default(b"BPMS", DEFAULT_BPMS, PropValue::TimingPairs(&summary.global_timing_segments.bpms)),
        Prop::new(b"STOPS", PropValue::TimingPairs(&stops)),
        Prop::ssc_only(b"DELAYS", PropValue::TimingPairs(&summary.global_timing_segments.delays)),
        Prop::ssc_only(b"WARPS", PropValue::TimingPairs(&summary.global_timing_segments.warps)),
        Prop::ssc_only_with_default(b"TIMESIGNATURES", DEFAULT_TIME_SIGNATURES, PropValue::StrNoEscape(&summary.normalized_time_signatures)),
        Prop::ssc_only_with_default(b"TICKCOUNTS", DEFAULT_TICKCOUNTS, PropValue::StrNoEscape(&summary.normalized_tickcounts)),
        Prop::ssc_only_with_default(b"COMBOS", DEFAULT_COMBOS, PropValue::StrNoEscape(&summary.normalized_combos)),
        Prop::ssc_only_with_default(b"SPEEDS", DEFAULT_SPEEDS, PropValue::TimingSpeeds(&summary.global_timing_segments.speeds)),
        Prop::ssc_only_with_default(b"SCROLLS", DEFAULT_SCROLLS, PropValue::TimingPairs(&summary.global_timing_segments.scrolls)),
        Prop::ssc_only(b"FAKES", PropValue::TimingPairs(&summary.global_timing_segments.fakes)),
        Prop::ssc_only_with_default(b"LABELS", DEFAULT_LABELS, PropValue::StrNoEscape(&summary.normalized_labels)),
        Prop::ssc_nonempty_only(b"LASTSECONDHINT", PropValue::NumberOpt(summary.last_second_hint)),
        Prop::new(b"BGCHANGES", PropValue::StrNoEscape(&summary.normalized_bgchanges)),
        Prop::nonempty_only(b"FGCHANGES", PropValue::StrNoEscape(&summary.normalized_fgchanges)),
        Prop::new(b"KEYSOUNDS", PropValue::StrNoEscape(&summary.normalized_keysounds)),
        Prop::new(b"ATTACKS", PropValue::StrNoEscape(&summary.normalized_attacks)),
    ];

    for prop in props {
        written_bytes += prop.serialize(ssc, false, out)?;
    }

    written_bytes += write_all!(out, b"\n")?;

    for chart in &summary.charts {
        written_bytes += write_chart_comment_prefix(out, chart)?;
        written_bytes += if ssc {
            serialize_ssc_chart(out, chart)
        } else {
            serialize_sm_chart(out, chart)
        }?;
        written_bytes += write_all!(out, b"\n")?;
    }

    Ok(written_bytes)
}

#[must_use]
fn write_chart_comment_prefix(
    out: &mut dyn io::Write,
    chart: &crate::ChartSummary,
) -> io::Result<usize> {
    let mut written_bytes: usize = 0;

    written_bytes += write_all!(out, b"//---------------")?;
    match chart.step_type_str.as_ref() {
        "" => {
            written_bytes += write_all!(out, DEFAULT_STEPSTYPE)?;
        }
        s => {
            // Passively strip newlines to ensure that we never terminate the comment prematurely
            for line in s.lines() {
                written_bytes += write_all!(out, line.as_bytes())?;
            }
        }
    };
    written_bytes += write_all!(out, b" - ")?;
    // Passively strip newlines again
    for line in chart.description_str.lines() {
        written_bytes += write_all!(out, line.as_bytes())?;
    }
    written_bytes += write_all!(out, b"----------------\n")?;

    Ok(written_bytes)
}

#[must_use]
fn serialize_sm_chart(out: &mut dyn io::Write, chart: &crate::ChartSummary) -> io::Result<usize> {
    let mut written_bytes = 0;
    written_bytes += write_all!(out, b"#NOTES:\n")?;

    // Kludge: don't use `Prop#serialize` here because SM charts are special.
    // We use `Prop` anyway for the convenience of `new_with_default` and `PropValue::RadarValues`.
    #[rustfmt::skip]
    let props = [
        Prop::new_with_default(b"", DEFAULT_STEPSTYPE, PropValue::Str(&chart.step_type_str)),
        Prop::new(b"", PropValue::Str(&chart.description_str)),
        Prop::new_with_default(b"", DEFAULT_DIFFICULTY, PropValue::Str(&chart.difficulty_str)),
        Prop::new_with_default(b"", DEFAULT_METER, PropValue::Str(&chart.rating_str)),
        Prop::new(b"", PropValue::RadarValues(chart.cached_radar_values, false)),
    ];

    for prop in props {
        let value = match (prop.default_value, prop.value.is_empty()) {
            (Some(default), true) => PropValue::Bytes(default),
            _ => prop.value,
        };
        written_bytes += write_all!(out, b"     ")?;
        written_bytes += value.serialize(out)?;
        written_bytes += write_all!(out, b":\n")?;
    }

    written_bytes += write_all!(out, &chart.minimized_note_data)?;
    written_bytes += write_all!(out, b";\n")?;
    Ok(written_bytes)
}

#[must_use]
fn serialize_ssc_chart(out: &mut dyn io::Write, chart: &crate::ChartSummary) -> io::Result<usize> {
    let mut written_bytes = 0;

    #[rustfmt::skip]
    let props = [
        Prop::new(b"NOTEDATA", PropValue::Empty),
        Prop::new(b"CHARTNAME", PropValue::Str(&chart.chart_name_str)),
        Prop::new_with_default(b"STEPSTYPE", DEFAULT_STEPSTYPE, PropValue::Str(&chart.step_type_str)),
        Prop::new(b"DESCRIPTION", PropValue::Str(&chart.description_str)),
        Prop::new(b"CHARTSTYLE", PropValue::Str(&chart.chart_style_str)),
        Prop::new_with_default(b"DIFFICULTY", DEFAULT_DIFFICULTY, PropValue::Str(&chart.difficulty_str)),
        Prop::new_with_default(b"METER", DEFAULT_METER, PropValue::Str(&chart.rating_str)),
        Prop::nonempty_only(b"MUSIC", PropValue::Str(&chart.music_path)),
        Prop::new(b"RADARVALUES", PropValue::RadarValues(chart.cached_radar_values, true)),
        Prop::new(b"CREDIT", PropValue::Str(&chart.step_artist_str)),
        Prop::own_timing_only(b"OFFSET", PropValue::Number(chart.chart_offset_seconds)),
        Prop::own_timing_only_with_default(b"BPMS", DEFAULT_BPMS, PropValue::TimingPairs(&chart.timing_segments.bpms)),
        Prop::own_timing_only(b"STOPS", PropValue::TimingPairs(&chart.timing_segments.stops)),
        Prop::own_timing_only(b"DELAYS", PropValue::TimingPairs(&chart.timing_segments.delays)),
        Prop::own_timing_only(b"WARPS", PropValue::TimingPairs(&chart.timing_segments.warps)),
        Prop::own_timing_only_with_default(b"TIMESIGNATURES", DEFAULT_TIME_SIGNATURES, PropValue::StrNoEscapeOpt(chart.chart_time_signatures.as_deref())),
        Prop::own_timing_only_with_default(b"TICKCOUNTS", DEFAULT_TICKCOUNTS, PropValue::StrNoEscapeOpt(chart.chart_tickcounts.as_deref())),
        Prop::own_timing_only_with_default(b"COMBOS", DEFAULT_COMBOS, PropValue::StrNoEscapeOpt(chart.chart_combos.as_deref())),
        Prop::own_timing_only_with_default(b"SPEEDS", DEFAULT_SPEEDS, PropValue::TimingSpeeds(&chart.timing_segments.speeds)),
        Prop::own_timing_only_with_default(b"SCROLLS", DEFAULT_SCROLLS, PropValue::TimingPairs(&chart.timing_segments.scrolls)),
        Prop::own_timing_only(b"FAKES", PropValue::TimingPairs(&chart.timing_segments.fakes)),
        Prop::own_timing_only_with_default(b"LABELS", DEFAULT_LABELS, PropValue::StrNoEscapeOpt(chart.chart_labels.as_deref())),
        Prop::nonempty_only(b"ATTACKS", PropValue::StrNoEscapeOpt(chart.chart_has_own_attacks.then(|| chart.chart_attacks.as_deref()).flatten())),
        Prop::nonempty_only(b"DISPLAYBPM", PropValue::StrNoEscapeOpt(chart.chart_display_bpm.as_deref())),
        Prop::nonempty_only(b"NOTES", PropValue::NoteData(&chart.minimized_note_data)),
        Prop::nonempty_only(b"NOTES2", PropValue::Empty), // TODO: write NOTES2 instead of NOTES as needed
    ];

    for prop in props {
        written_bytes += prop.serialize(true, chart.chart_has_own_timing, out)?;
    }

    Ok(written_bytes)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rssp_core::timing::TimingSegments;
    use std::io;
    use std::sync::Arc;

    #[test]
    fn serialize_simfile_with_ssc() -> io::Result<()> {
        let mut summary = simfile_summary_with_all_fields(false, false);
        summary
            .charts
            .push(chart_summary_with_all_fields(false, false, false));
        summary
            .charts
            .push(chart_summary_with_all_fields(true, false, false));
        let mut buffer = vec![];
        {
            let mut cursor = io::Cursor::new(&mut buffer);
            super::serialize_simfile(&summary, "ssc", &mut cursor)?;
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
            #ORIGIN:Origin;\n\
            #CREDIT:Credit;\n\
            #BANNER:banner.png;\n\
            #BACKGROUND:background.png;\n\
            #PREVIEWVID:previewvid.mov;\n\
            #JACKET:jacket.png;\n\
            #CDIMAGE:cdimage.png;\n\
            #DISCIMAGE:discimage.png;\n\
            #LYRICSPATH:lyrics.lrc;\n\
            #CDTITLE:cdtitle.png;\n\
            #MUSIC:music.ogg;\n\
            #OFFSET:0.123000;\n\
            #SAMPLESTART:10.000000;\n\
            #SAMPLELENGTH:16.000000;\n\
            #SELECTABLE:YES;\n\
            #BPMS:0.000000=120.000000,\n\
            16.000000=240.000000,\n\
            48.000000=120.000000;\n\
            #STOPS:1.000000=1.250000,\n\
            1.500000=1.750000;\n\
            #DELAYS:2.000000=2.250000,\n\
            2.500000=2.750000;\n\
            #WARPS:3.000000=3.250000,\n\
            3.500000=3.750000;\n\
            #TIMESIGNATURES:0.000000=4=4,\n\
            16.000000=8=4,\n\
            48.000000=4=4;\n\
            #TICKCOUNTS:0.000000=4,\n\
            16.000000=2,\n\
            48.000000=4;\n\
            #COMBOS:0.000000=1,\n\
            16.000000=2,\n\
            48.000000=1;\n\
            #SPEEDS:0.000000=1.000000=0.000000=0,\n\
            12.000000=0.500000=4.000000=0,\n\
            48.000000=1.000000=0.000000=1;\n\
            #SCROLLS:0.000000=1.000000,\n\
            16.000000=2.000000,\n\
            48.000000=1.000000;\n\
            #FAKES:4.000000=4.250000,\n\
            4.500000=4.750000;\n\
            #LABELS:0.000000=Song Start,\n\
            16.000000=Speedup;\n\
            #BGCHANGES:99999=-nosongbg-=1.000=0=0=0 // don't automatically add -songbackground-\n\
            ;\n\
            #KEYSOUNDS:a.ogg,b.ogg.c.ogg;\n\
            #ATTACKS:;\n\
            \n\
            //---------------dance-single - Description----------------\n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #RADARVALUES:0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000,\
            0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000;\n\
            #CREDIT:Step artist;\n\
            #NOTES:\n\
            1000\n\
            0000\n\
            0100\n\
            00M0\n\
            0100\n\
            0000\n\
            0001\n\
            0000\n\
            ;\n\
            \n\
            //---------------dance-single - Description----------------\n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #RADARVALUES:0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000,\
            0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000;\n\
            #CREDIT:Step artist;\n\
            #OFFSET:0.000000;\n\
            #BPMS:0.000000=120.000000,\n\
            116.000000=240.000000,\n\
            148.000000=120.000000;\n\
            #STOPS:101.000000=1.250000,\n\
            101.500000=1.750000;\n\
            #DELAYS:102.000000=2.250000,\n\
            102.500000=2.750000;\n\
            #WARPS:103.000000=3.250000,\n\
            103.500000=3.750000;\n\
            #TIMESIGNATURES:0.000000=4=4,\n\
            116.000000=8=4,\n\
            148.000000=4=4;\n\
            #TICKCOUNTS:0.000000=4,\n\
            116.000000=2,\n\
            148.000000=4;\n\
            #COMBOS:0.000000=1,\n\
            116.000000=2,\n\
            148.000000=1;\n\
            #SPEEDS:0.000000=1.000000=0.000000=0,\n\
            112.000000=0.500000=4.000000=0,\n\
            148.000000=1.000000=0.000000=1;\n\
            #SCROLLS:0.000000=1.000000,\n\
            116.000000=2.000000,\n\
            148.000000=1.000000;\n\
            #FAKES:104.000000=4.250000,\n\
            104.500000=4.750000;\n\
            #LABELS:0.000000=Song Start,\n\
            116.000000=Speedup;\n\
            #NOTES:\n\
            1000\n\
            0000\n\
            0100\n\
            00M0\n\
            0100\n\
            0000\n\
            0001\n\
            0000\n\
            ;\n\
            \n";

        assert_eq!(expected, output);

        Ok(())
    }

    #[test]
    fn serialize_simfile_with_ssc_and_nonempty_fields() -> io::Result<()> {
        let mut summary = simfile_summary_with_all_fields(true, false);
        summary
            .charts
            .push(chart_summary_with_all_fields(false, true, false));
        summary
            .charts
            .push(chart_summary_with_all_fields(true, true, false));
        let mut buffer = vec![];
        {
            let mut cursor = io::Cursor::new(&mut buffer);
            super::serialize_simfile(&summary, "ssc", &mut cursor)?;
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
            #ORIGIN:Origin;\n\
            #CREDIT:Credit;\n\
            #BANNER:banner.png;\n\
            #BACKGROUND:background.png;\n\
            #PREVIEWVID:previewvid.mov;\n\
            #JACKET:jacket.png;\n\
            #CDIMAGE:cdimage.png;\n\
            #DISCIMAGE:discimage.png;\n\
            #LYRICSPATH:lyrics.lrc;\n\
            #CDTITLE:cdtitle.png;\n\
            #MUSIC:music.ogg;\n\
            #OFFSET:0.123000;\n\
            #SAMPLESTART:10.000000;\n\
            #SAMPLELENGTH:16.000000;\n\
            #SELECTABLE:YES;\n\
            #DISPLAYBPM:150;\n\
            #BPMS:0.000000=120.000000,\n\
            16.000000=240.000000,\n\
            48.000000=120.000000;\n\
            #STOPS:1.000000=1.250000,\n\
            1.500000=1.750000;\n\
            #DELAYS:2.000000=2.250000,\n\
            2.500000=2.750000;\n\
            #WARPS:3.000000=3.250000,\n\
            3.500000=3.750000;\n\
            #TIMESIGNATURES:0.000000=4=4,\n\
            16.000000=8=4,\n\
            48.000000=4=4;\n\
            #TICKCOUNTS:0.000000=4,\n\
            16.000000=2,\n\
            48.000000=4;\n\
            #COMBOS:0.000000=1,\n\
            16.000000=2,\n\
            48.000000=1;\n\
            #SPEEDS:0.000000=1.000000=0.000000=0,\n\
            12.000000=0.500000=4.000000=0,\n\
            48.000000=1.000000=0.000000=1;\n\
            #SCROLLS:0.000000=1.000000,\n\
            16.000000=2.000000,\n\
            48.000000=1.000000;\n\
            #FAKES:4.000000=4.250000,\n\
            4.500000=4.750000;\n\
            #LABELS:0.000000=Song Start,\n\
            16.000000=Speedup;\n\
            #LASTSECONDHINT:120.000000;\n\
            #BGCHANGES:99999=-nosongbg-=1.000=0=0=0 // don't automatically add -songbackground-\n\
            ;\n\
            #FGCHANGES:0.000000=lua=1.000000=0=0=1;\n\
            #KEYSOUNDS:a.ogg,b.ogg.c.ogg;\n\
            #ATTACKS:TIME=64.000000:LEN=2.000000:MODS=*0.5 stealth;\n\
            \n\
            //---------------dance-single - Description----------------\n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #MUSIC:chart_music.ogg;\n\
            #RADARVALUES:0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000,\
            0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000;\n\
            #CREDIT:Step artist;\n\
            #ATTACKS:TIME=164.000000:LEN=2.000000:MODS=*0.5 stealth;\n\
            #DISPLAYBPM:300;\n\
            #NOTES:\n\
            1000\n\
            0000\n\
            0100\n\
            00M0\n\
            0100\n\
            0000\n\
            0001\n\
            0000\n\
            ;\n\
            \n\
            //---------------dance-single - Description----------------\n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #MUSIC:chart_music.ogg;\n\
            #RADARVALUES:0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000,\
            0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000;\n\
            #CREDIT:Step artist;\n\
            #OFFSET:0.000000;\n\
            #BPMS:0.000000=120.000000,\n\
            116.000000=240.000000,\n\
            148.000000=120.000000;\n\
            #STOPS:101.000000=1.250000,\n\
            101.500000=1.750000;\n\
            #DELAYS:102.000000=2.250000,\n\
            102.500000=2.750000;\n\
            #WARPS:103.000000=3.250000,\n\
            103.500000=3.750000;\n\
            #TIMESIGNATURES:0.000000=4=4,\n\
            116.000000=8=4,\n\
            148.000000=4=4;\n\
            #TICKCOUNTS:0.000000=4,\n\
            116.000000=2,\n\
            148.000000=4;\n\
            #COMBOS:0.000000=1,\n\
            116.000000=2,\n\
            148.000000=1;\n\
            #SPEEDS:0.000000=1.000000=0.000000=0,\n\
            112.000000=0.500000=4.000000=0,\n\
            148.000000=1.000000=0.000000=1;\n\
            #SCROLLS:0.000000=1.000000,\n\
            116.000000=2.000000,\n\
            148.000000=1.000000;\n\
            #FAKES:104.000000=4.250000,\n\
            104.500000=4.750000;\n\
            #LABELS:0.000000=Song Start,\n\
            116.000000=Speedup;\n\
            #ATTACKS:TIME=164.000000:LEN=2.000000:MODS=*0.5 stealth;\n\
            #DISPLAYBPM:300;\n\
            #NOTES:\n\
            1000\n\
            0000\n\
            0100\n\
            00M0\n\
            0100\n\
            0000\n\
            0001\n\
            0000\n\
            ;\n\
            \n";

        assert_eq!(expected, output);

        Ok(())
    }

    #[test]
    fn serialize_simfile_with_ssc_and_trigger_defaults() -> io::Result<()> {
        let mut summary = simfile_summary_with_all_fields(false, true);
        summary
            .charts
            .push(chart_summary_with_all_fields(false, false, true));
        summary
            .charts
            .push(chart_summary_with_all_fields(true, false, true));
        let mut buffer = vec![];
        {
            let mut cursor = io::Cursor::new(&mut buffer);
            super::serialize_simfile(&summary, "ssc", &mut cursor)?;
        };

        let output = String::from_utf8(buffer).unwrap();

        let expected = "#VERSION:0.83;\n\
            #TITLE:Untitled;\n\
            #SUBTITLE:Subtitle;\n\
            #ARTIST:Unknown artist;\n\
            #TITLETRANSLIT:Title translit;\n\
            #SUBTITLETRANSLIT:Subtitle translit;\n\
            #ARTISTTRANSLIT:Artist translit;\n\
            #GENRE:Genre;\n\
            #ORIGIN:Origin;\n\
            #CREDIT:Credit;\n\
            #BANNER:banner.png;\n\
            #BACKGROUND:background.png;\n\
            #PREVIEWVID:previewvid.mov;\n\
            #JACKET:jacket.png;\n\
            #CDIMAGE:cdimage.png;\n\
            #DISCIMAGE:discimage.png;\n\
            #LYRICSPATH:lyrics.lrc;\n\
            #CDTITLE:cdtitle.png;\n\
            #MUSIC:music.ogg;\n\
            #OFFSET:0.123000;\n\
            #SAMPLESTART:10.000000;\n\
            #SAMPLELENGTH:16.000000;\n\
            #SELECTABLE:YES;\n\
            #BPMS:0.000000=60.000000;\n\
            #STOPS:;\n\
            #DELAYS:;\n\
            #WARPS:;\n\
            #TIMESIGNATURES:0.000000=4=4;\n\
            #TICKCOUNTS:0.000000=4;\n\
            #COMBOS:0.000000=1;\n\
            #SPEEDS:0.000000=1.000000=0.000000=0;\n\
            #SCROLLS:0.000000=1.000000;\n\
            #FAKES:;\n\
            #LABELS:0.000000=Song Start;\n\
            #BGCHANGES:99999=-nosongbg-=1.000=0=0=0 // don't automatically add -songbackground-\n\
            ;\n\
            #KEYSOUNDS:a.ogg,b.ogg.c.ogg;\n\
            #ATTACKS:;\n\
            \n\
            //---------------dance-single - Description----------------\n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Beginner;\n\
            #METER:1;\n\
            #RADARVALUES:0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000,\
            0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000;\n\
            #CREDIT:Step artist;\n\
            #NOTES:\n\
            1000\n\
            0000\n\
            0100\n\
            00M0\n\
            0100\n\
            0000\n\
            0001\n\
            0000\n\
            ;\n\
            \n\
            //---------------dance-single - Description----------------\n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Beginner;\n\
            #METER:1;\n\
            #RADARVALUES:0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000,\
            0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000;\n\
            #CREDIT:Step artist;\n\
            #OFFSET:0.000000;\n\
            #BPMS:0.000000=60.000000;\n\
            #STOPS:;\n\
            #DELAYS:;\n\
            #WARPS:;\n\
            #TIMESIGNATURES:0.000000=4=4;\n\
            #TICKCOUNTS:0.000000=4;\n\
            #COMBOS:0.000000=1;\n\
            #SPEEDS:0.000000=1.000000=0.000000=0;\n\
            #SCROLLS:0.000000=1.000000;\n\
            #FAKES:;\n\
            #LABELS:0.000000=Song Start;\n\
            #NOTES:\n\
            1000\n\
            0000\n\
            0100\n\
            00M0\n\
            0100\n\
            0000\n\
            0001\n\
            0000\n\
            ;\n\
            \n";

        assert_eq!(expected, output);

        Ok(())
    }

    #[test]
    fn serialize_simfile_with_sm() -> io::Result<()> {
        let mut summary = simfile_summary_with_all_fields(false, false);
        summary
            .charts
            .push(chart_summary_with_all_fields(false, false, false));

        let mut buffer = vec![];
        {
            let mut cursor = io::Cursor::new(&mut buffer);
            super::serialize_simfile(&summary, "sm", &mut cursor)?;
        };

        let output = String::from_utf8(buffer).unwrap();

        let expected = "#TITLE:Title;\n\
        #SUBTITLE:Subtitle;\n\
        #ARTIST:Artist;\n\
        #TITLETRANSLIT:Title translit;\n\
        #SUBTITLETRANSLIT:Subtitle translit;\n\
        #ARTISTTRANSLIT:Artist translit;\n\
        #GENRE:Genre;\n\
        #CREDIT:Credit;\n\
        #BANNER:banner.png;\n\
        #BACKGROUND:background.png;\n\
        #LYRICSPATH:lyrics.lrc;\n\
        #CDTITLE:cdtitle.png;\n\
        #MUSIC:music.ogg;\n\
        #OFFSET:0.123000;\n\
        #SAMPLESTART:10.000000;\n\
        #SAMPLELENGTH:16.000000;\n\
        #SELECTABLE:YES;\n\
        #BPMS:0.000000=120.000000,\n\
        16.000000=240.000000,\n\
        48.000000=120.000000;\n\
        #STOPS:1.000000=1.250000,\n\
        1.500000=1.750000,\n\
        2.000000=2.250000,\n\
        2.500000=2.750000,\n\
        3.000000=-1.625000,\n\
        3.500000=-1.875000;\n\
        #BGCHANGES:99999=-nosongbg-=1.000=0=0=0 // don't automatically add -songbackground-\n\
        ;\n\
        #KEYSOUNDS:a.ogg,b.ogg.c.ogg;\n\
        #ATTACKS:;\n\
        \n\
        //---------------dance-single - Description----------------\n\
        #NOTES:\n     dance-single:\n     Description:\n     Challenge:\n     17:\n     0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000:\n\
        1000\n\
        0000\n\
        0100\n\
        00M0\n\
        0100\n\
        0000\n\
        0001\n\
        0000\n\
        ;\n\
        \n";

        assert_eq!(expected, output);

        Ok(())
    }

    #[test]
    fn serialize_simfile_with_sm_and_nonempty_fields() -> io::Result<()> {
        let mut summary = simfile_summary_with_all_fields(true, false);
        summary
            .charts
            .push(chart_summary_with_all_fields(false, true, false));

        let mut buffer = vec![];
        {
            let mut cursor = io::Cursor::new(&mut buffer);
            super::serialize_simfile(&summary, "sm", &mut cursor)?;
        };

        let output = String::from_utf8(buffer).unwrap();

        let expected = "#TITLE:Title;\n\
        #SUBTITLE:Subtitle;\n\
        #ARTIST:Artist;\n\
        #TITLETRANSLIT:Title translit;\n\
        #SUBTITLETRANSLIT:Subtitle translit;\n\
        #ARTISTTRANSLIT:Artist translit;\n\
        #GENRE:Genre;\n\
        #CREDIT:Credit;\n\
        #BANNER:banner.png;\n\
        #BACKGROUND:background.png;\n\
        #LYRICSPATH:lyrics.lrc;\n\
        #CDTITLE:cdtitle.png;\n\
        #MUSIC:music.ogg;\n\
        #OFFSET:0.123000;\n\
        #SAMPLESTART:10.000000;\n\
        #SAMPLELENGTH:16.000000;\n\
        #SELECTABLE:YES;\n\
        #DISPLAYBPM:150;\n\
        #BPMS:0.000000=120.000000,\n\
        16.000000=240.000000,\n\
        48.000000=120.000000;\n\
        #STOPS:1.000000=1.250000,\n\
        1.500000=1.750000,\n\
        2.000000=2.250000,\n\
        2.500000=2.750000,\n\
        3.000000=-1.625000,\n\
        3.500000=-1.875000;\n\
        #BGCHANGES:99999=-nosongbg-=1.000=0=0=0 // don't automatically add -songbackground-\n\
        ;\n\
        #FGCHANGES:0.000000=lua=1.000000=0=0=1;\n\
        #KEYSOUNDS:a.ogg,b.ogg.c.ogg;\n\
        #ATTACKS:TIME=64.000000:LEN=2.000000:MODS=*0.5 stealth;\n\
        \n\
        //---------------dance-single - Description----------------\n\
        #NOTES:\n     dance-single:\n     Description:\n     Challenge:\n     17:\n     0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000:\n\
        1000\n\
        0000\n\
        0100\n\
        00M0\n\
        0100\n\
        0000\n\
        0001\n\
        0000\n\
        ;\n\
        \n";

        assert_eq!(expected, output);

        Ok(())
    }

    #[test]
    fn serialize_simfile_with_sm_and_trigger_defaults() -> io::Result<()> {
        let mut summary = simfile_summary_with_all_fields(false, true);
        summary
            .charts
            .push(chart_summary_with_all_fields(false, false, true));

        let mut buffer = vec![];
        {
            let mut cursor = io::Cursor::new(&mut buffer);
            super::serialize_simfile(&summary, "sm", &mut cursor)?;
        };

        let output = String::from_utf8(buffer).unwrap();

        let expected = "#TITLE:Untitled;\n\
        #SUBTITLE:Subtitle;\n\
        #ARTIST:Unknown artist;\n\
        #TITLETRANSLIT:Title translit;\n\
        #SUBTITLETRANSLIT:Subtitle translit;\n\
        #ARTISTTRANSLIT:Artist translit;\n\
        #GENRE:Genre;\n\
        #CREDIT:Credit;\n\
        #BANNER:banner.png;\n\
        #BACKGROUND:background.png;\n\
        #LYRICSPATH:lyrics.lrc;\n\
        #CDTITLE:cdtitle.png;\n\
        #MUSIC:music.ogg;\n\
        #OFFSET:0.123000;\n\
        #SAMPLESTART:10.000000;\n\
        #SAMPLELENGTH:16.000000;\n\
        #SELECTABLE:YES;\n\
        #BPMS:0.000000=60.000000;\n\
        #STOPS:;\n\
        #BGCHANGES:99999=-nosongbg-=1.000=0=0=0 // don't automatically add -songbackground-\n\
        ;\n\
        #KEYSOUNDS:a.ogg,b.ogg.c.ogg;\n\
        #ATTACKS:;\n\
        \n\
        //---------------dance-single - Description----------------\n\
        #NOTES:\n     dance-single:\n     Description:\n     Beginner:\n     1:\n     0.010000,0.020000,0.030000,0.040000,0.050000,0.060000,0.070000,0.080000,0.090000,0.100000,0.110000,0.120000,0.130000,0.140000:\n\
        1000\n\
        0000\n\
        0100\n\
        00M0\n\
        0100\n\
        0000\n\
        0001\n\
        0000\n\
        ;\n\
        \n";

        assert_eq!(expected, output);

        Ok(())
    }

    #[test]
    fn serialize_simfile_with_sm_and_chart_timing_returns_error() -> io::Result<()> {
        let mut summary = simfile_summary_with_all_fields(true, false);
        summary
            .charts
            .push(chart_summary_with_all_fields(true, false, false));

        let mut buffer = vec![];
        {
            let mut cursor = io::Cursor::new(&mut buffer);

            match super::serialize_simfile(&summary, "sm", &mut cursor) {
                Ok(_) => panic!("serialize_simfile returned Ok but a chart has its own timing"),
                Err(_) => {}
            }
        };

        Ok(())
    }

    fn simfile_summary_with_all_fields(
        include_nonempty: bool,
        trigger_defaults: bool,
    ) -> crate::SimfileSummary {
        crate::SimfileSummary {
            title_str: match trigger_defaults {
                false => String::from("Title"),
                true => String::from(""),
            },
            subtitle_str: String::from("Subtitle"),
            artist_str: match trigger_defaults {
                false => String::from("Artist"),
                true => String::from(""),
            },
            genre_str: String::from("Genre"),
            titletranslit_str: String::from("Title translit"),
            subtitletranslit_str: String::from("Subtitle translit"),
            artisttranslit_str: String::from("Artist translit"),
            offset: 0.123,
            normalized_bpms: Default::default(),
            normalized_stops: Default::default(),
            normalized_delays: Default::default(),
            normalized_warps: Default::default(),
            normalized_speeds: Default::default(),
            normalized_scrolls: Default::default(),
            normalized_fakes: Default::default(),
            global_timing_segments: if trigger_defaults {
                Arc::new(TimingSegments::default())
            } else {
                Arc::new(TimingSegments {
                    beat0_offset_adjust: Default::default(),
                    bpms: vec![(0.000, 120.000), (16.000, 240.000), (48.000, 120.000)],
                    stops: vec![(1.000, 1.250), (1.500, 1.750)],
                    delays: vec![(2.000, 2.250), (2.500, 2.750)],
                    warps: vec![(3.000, 3.250), (3.500, 3.750)],
                    speeds: vec![
                        (0.000, 1.000, 0.000, rssp_core::timing::SpeedUnit::Beats),
                        (12.000, 0.500, 4.000, rssp_core::timing::SpeedUnit::Beats),
                        (48.000, 1.000, 0.000, rssp_core::timing::SpeedUnit::Seconds),
                    ],
                    scrolls: vec![(0.000, 1.000), (16.000, 2.000), (48.000, 1.000)],
                    fakes: vec![(4.000, 4.250), (4.500, 4.750)],
                })
            },
            normalized_time_signatures: if trigger_defaults {
                String::from("")
            } else {
                String::from("0.000000=4=4,\n16.000000=8=4,\n48.000000=4=4")
            },
            normalized_labels: if trigger_defaults {
                String::from("")
            } else {
                String::from("0.000000=Song Start,\n16.000000=Speedup")
            },
            normalized_tickcounts: if trigger_defaults {
                String::from("")
            } else {
                String::from("0.000000=4,\n16.000000=2,\n48.000000=4")
            },
            normalized_combos: if trigger_defaults {
                String::from("")
            } else {
                String::from("0.000000=1,\n16.000000=2,\n48.000000=1")
            },
            ssc_version: 0.83,
            timing_format: rssp_core::timing::TimingFormat::Ssc,
            banner_path: String::from("banner.png"),
            background_path: String::from("background.png"),
            cdtitle_path: String::from("cdtitle.png"),
            jacket_path: String::from("jacket.png"),
            music_path: String::from("music.ogg"),
            display_bpm_str: if include_nonempty {
                String::from("150")
            } else {
                Default::default()
            },
            sample_start: 10.0,
            sample_length: 16.0,
            origin_str: String::from("Origin"),
            credit_str: String::from("Credit"),
            normalized_bgchanges: String::from(
                "99999=-nosongbg-=1.000=0=0=0 // don't automatically add -songbackground-\n",
            ),
            normalized_fgchanges: if include_nonempty {
                String::from("0.000000=lua=1.000000=0=0=1")
            } else {
                Default::default()
            },
            normalized_keysounds: String::from("a.ogg,b.ogg.c.ogg"),
            normalized_attacks: if include_nonempty {
                String::from("TIME=64.000000:LEN=2.000000:MODS=*0.5 stealth")
            } else {
                Default::default()
            },
            previewvid_path: String::from("previewvid.mov"),
            cdimage_path: String::from("cdimage.png"),
            discimage_path: String::from("discimage.png"),
            lyrics_path: String::from("lyrics.lrc"),
            selectable: true,
            last_second_hint: include_nonempty.then(|| 120.0),

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

    fn chart_summary_with_all_fields(
        has_own_timing: bool,
        include_nonempty: bool,
        trigger_defaults: bool,
    ) -> crate::ChartSummary {
        crate::ChartSummary {
            step_type_str: match trigger_defaults {
                false => String::from("dance-single"),
                true => String::from(""),
            },
            step_artist_str: String::from("Step artist"),
            description_str: String::from("Description"),
            chart_name_str: String::from("Chart name"),
            chart_style_str: String::from("Chart style"),
            difficulty_str: match trigger_defaults {
                false => String::from("Challenge"),
                true => String::from(""),
            },
            rating_str: match trigger_defaults {
                false => String::from("17"),
                true => String::from(""),
            },
            cached_radar_values: Some([
                0.010, 0.020, 0.030, 0.040, 0.050, 0.060, 0.070, 0.080, 0.090, 0.100, 0.110, 0.120,
                0.130, 0.140,
            ]),
            tech_notation_str: String::from("BR FS XO"),
            minimized_note_data: b"1000\n0000\n0100\n00M0\n0100\n0000\n0001\n0000\n".to_vec(),
            music_path: match include_nonempty {
                true => String::from("chart_music.ogg"),
                false => String::from(""),
            },

            // Timing fields
            chart_has_own_timing: has_own_timing,
            chart_time_signatures: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000000=4=4,\n116.000000=8=4,\n148.000000=4=4")),
            chart_labels: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000000=Song Start,\n116.000000=Speedup")),
            chart_tickcounts: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000000=4,\n116.000000=2,\n148.000000=4")),
            chart_combos: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000000=1,\n116.000000=2,\n148.000000=1")),
            chart_attacks: include_nonempty
                .then(|| String::from("TIME=164.000000:LEN=2.000000:MODS=*0.5 stealth")),
            chart_has_own_attacks: include_nonempty,
            chart_display_bpm: include_nonempty.then(|| String::from("300")),

            timing_segments: if has_own_timing && !trigger_defaults {
                Arc::new(TimingSegments {
                    beat0_offset_adjust: Default::default(),
                    bpms: vec![(0.000, 120.000), (116.000, 240.000), (148.000, 120.000)],
                    stops: vec![(101.000, 1.250), (101.500, 1.750)],
                    delays: vec![(102.000, 2.250), (102.500, 2.750)],
                    warps: vec![(103.000, 3.250), (103.500, 3.750)],
                    speeds: vec![
                        (0.000, 1.000, 0.000, rssp_core::timing::SpeedUnit::Beats),
                        (112.000, 0.500, 4.000, rssp_core::timing::SpeedUnit::Beats),
                        (148.000, 1.000, 0.000, rssp_core::timing::SpeedUnit::Seconds),
                    ],
                    scrolls: vec![(0.000, 1.000), (116.000, 2.000), (148.000, 1.000)],
                    fakes: vec![(104.000, 4.250), (104.500, 4.750)],
                })
            } else {
                Arc::new(TimingSegments::default())
            },

            // Unused string timing fields (TimingSegments used instead)
            chart_bpms: Default::default(),
            chart_bpms_norm: Default::default(),
            chart_stops: Default::default(),
            chart_delays: Default::default(),
            chart_warps: Default::default(),
            chart_speeds: Default::default(),
            chart_scrolls: Default::default(),
            chart_fakes: Default::default(),

            // The remaining fields are irrelevant to serialization
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
            matrix_profile: Default::default(),
        }
    }
}
