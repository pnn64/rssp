use std::io;

use rssp_core::parse::extension_is_ssc;

pub(crate) const DEFAULT_VERSION: &[u8] = b"0.83";
pub(crate) const DEFAULT_TITLE: &[u8] = b"Untitled";
pub(crate) const DEFAULT_ARTIST: &[u8] = b"Unknown artist";
pub(crate) const DEFAULT_BPMS: &[u8] = b"0.000000=60.000000";
pub(crate) const DEFAULT_TIME_SIGNATURES: &[u8] = b"0.000000=4=4";
pub(crate) const DEFAULT_TICKCOUNTS: &[u8] = b"0.000000=4";
pub(crate) const DEFAULT_COMBOS: &[u8] = b"0.000000=1";
pub(crate) const DEFAULT_SPEEDS: &[u8] = b"0.000000=1.000000=0.000000=0";
pub(crate) const DEFAULT_SCROLLS: &[u8] = b"0.000000=1.000000";
pub(crate) const DEFAULT_LABELS: &[u8] = b"0.000000=Song Start";
pub(crate) const DEFAULT_STEPSTYPE: &[u8] = b"dance-single";
pub(crate) const DEFAULT_DIFFICULTY: &[u8] = b"Beginner";
pub(crate) const DEFAULT_METER: &[u8] = b"1";

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
fn format_dot3_f32(value: f32) -> String {
    format!("{:.3}", value)
}

#[must_use]
#[inline(always)]
fn format_radar_values(radar_values: Option<[f32; crate::stats::RADAR_CATEGORY_COUNT]>) -> String {
    match radar_values {
        Some(radar_values) => radar_values
            .iter()
            .map(|&f| format_dot3_f32(f))
            .collect::<Vec<String>>()
            .join(","),
        None => String::from(""),
    }
}

enum ListItem<'a> {
    Float(f64),
    Int(i32),
    Str(&'a str),
}

#[derive(Default)]
enum PropValue<'a> {
    #[default]
    Empty,
    Str(&'a str),
    StrOpt(Option<&'a str>),
    Bytes(&'a [u8]),
    NoteData(&'a [u8]),
    Version(f32),
    Number(f64),
    NumberOpt(Option<f64>),
    Bool(bool),
    NormalizedList(&'a str),
    NormalizedListOpt(Option<&'a str>),
    // TODO: decide if we actually need these.
    // Currently only charts store timing fields like this and they're redundant with the normalized forms.
    List(&'a [[Option<ListItem<'a>>; 4]]),
    Pairs(&'a [(f64, f64)]),
    TimeSignatures(&'a [(f64, i32, i32)]),
    Labels(&'a [(f64, String)]),
    TickCounts(&'a [(f64, i32)]),
    Combos(&'a [(f64, i32, i32)]),
    Speeds(&'a [(f64, f64, f64, i32)]),
    RadarValues(Option<[f32; crate::stats::RADAR_CATEGORY_COUNT]>),
}

impl<'a> PropValue<'a> {
    #[must_use]
    fn serialize(&self, out: &mut dyn io::Write) -> io::Result<usize> {
        use PropValue::*;

        match self {
            Empty => Ok(0),
            Str(s) => sm_escape(out, s.as_bytes()),
            StrOpt(opt) => match opt {
                None => Ok(0),
                Some(s) => Str(s).serialize(out),
            },
            Bytes(b) => write_all!(out, b),
            NoteData(b) => Ok(write_all!(out, b"\n")? + write_all!(out, b)?),
            Version(v) => write_all!(out, format_version(*v).as_bytes()),
            Number(n) => write_all!(out, format_dot6_f64(*n).as_bytes()),
            NumberOpt(opt) => match opt {
                None => Ok(0),
                Some(n) => Number(*n).serialize(out),
            },
            Bool(b) => {
                if *b {
                    write_all!(out, b"YES")
                } else {
                    write_all!(out, b"NO")
                }
            }
            NormalizedList(items_str) => {
                let items = items_str.as_bytes();
                let mut written_bytes = 0;
                let mut start = 0;

                while let Some(offset) = items[start..].iter().position(|&b| b == b',') {
                    let comma = start + offset;
                    written_bytes += write_all!(out, &items[start..=comma])?;
                    written_bytes += write_all!(out, b"\n")?;
                    start = comma + 1;
                }

                written_bytes += write_all!(out, &items[start..])?;
                Ok(written_bytes)
            }
            NormalizedListOpt(opt_items_str) => match opt_items_str {
                None => Ok(0),
                Some(items_str) => NormalizedList(items_str).serialize(out),
            },
            List(items) => {
                let mut written_bytes = 0;
                let mut first_row = true;

                for row in *items {
                    if !first_row {
                        written_bytes += write_all!(out, b",\n")?;
                    }
                    let mut first_item = true;
                    for item in row {
                        if !first_item {
                            written_bytes += write_all!(out, b"=")?;
                        }
                        written_bytes += match item {
                            Some(ListItem::Float(f)) => {
                                write_all!(out, format_dot6_f64(*f).as_bytes())?
                            }
                            Some(ListItem::Int(i)) => write_all!(out, i.to_string().as_bytes())?,
                            Some(ListItem::Str(s)) => sm_escape(out, s.as_bytes())?,
                            None => break,
                        };
                        first_item = false;
                    }
                    first_row = false;
                }

                Ok(written_bytes)
            }
            Pairs(rows) => List(
                rows.iter()
                    .map(|row| {
                        [
                            Some(ListItem::Float(row.0)),
                            Some(ListItem::Float(row.1)),
                            None,
                            None,
                        ]
                    })
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .serialize(out),
            TimeSignatures(rows) => List(
                rows.iter()
                    .map(|row| {
                        [
                            Some(ListItem::Float(row.0)),
                            Some(ListItem::Int(row.1)),
                            Some(ListItem::Int(row.2)),
                            None,
                        ]
                    })
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .serialize(out),

            Labels(rows) => List(
                rows.iter()
                    .map(|row| {
                        [
                            Some(ListItem::Float(row.0)),
                            Some(ListItem::Str(&row.1)),
                            None,
                            None,
                        ]
                    })
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .serialize(out),
            TickCounts(rows) => List(
                rows.iter()
                    .map(|row| {
                        [
                            Some(ListItem::Float(row.0)),
                            Some(ListItem::Int(row.1)),
                            None,
                            None,
                        ]
                    })
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .serialize(out),
            Combos(rows) => List(
                rows.iter()
                    .map(|row| {
                        [
                            Some(ListItem::Float(row.0)),
                            Some(ListItem::Int(row.1)),
                            None,
                            None,
                        ]
                    })
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .serialize(out),
            Speeds(rows) => List(
                rows.iter()
                    .map(|row| {
                        [
                            Some(ListItem::Float(row.0)),
                            Some(ListItem::Float(row.1)),
                            Some(ListItem::Float(row.2)),
                            Some(ListItem::Int(row.3)),
                        ]
                    })
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .serialize(out),
            RadarValues(rv) => match rv {
                Some(values) => {
                    let mut written_bytes = 0;
                    let mut first_item = true;

                    for value in values {
                        if !first_item {
                            written_bytes += write_all!(out, b",")?;
                        }
                        written_bytes += write_all!(out, format_dot3_f32(*value).as_bytes())?;
                        first_item = false;
                    }

                    Ok(written_bytes)
                }
                None => Ok(0),
            },
        }
    }

    #[must_use]
    fn is_empty(&self) -> bool {
        use PropValue::*;
        match self {
            Empty => true,
            Str(s) => s.is_empty(),
            StrOpt(opt) => opt.as_ref().is_none_or(|s| s.is_empty()),
            Bytes(b) => b.is_empty(),
            NoteData(_) => false,
            Version(_) => false,
            Number(_) => false,
            NumberOpt(opt) => opt.is_none(),
            Bool(_) => false,
            NormalizedList(s) => s.is_empty(),
            NormalizedListOpt(opt) => opt.as_ref().is_none_or(|s| s.is_empty()),
            List(rows) => rows.is_empty(),
            Pairs(rows) => rows.is_empty(),
            TimeSignatures(rows) => rows.is_empty(),
            Labels(rows) => rows.is_empty(),
            TickCounts(rows) => rows.is_empty(),
            Combos(rows) => rows.is_empty(),
            Speeds(rows) => rows.is_empty(),
            RadarValues(rv) => rv.is_none(),
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

    #[must_use]
    #[inline(always)]
    fn new_with_default(key: &'a [u8], default: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        Prop {
            key,
            value,
            default_value: Some(default),
            ..Default::default()
        }
    }

    #[must_use]
    #[inline(always)]
    fn ssc_only(key: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        Prop {
            key,
            value,
            ssc_only: true,
            ..Default::default()
        }
    }

    #[must_use]
    #[inline(always)]
    fn ssc_nonempty_only(key: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        Prop {
            key,
            value,
            ssc_only: true,
            nonempty_value_only: true,
            ..Default::default()
        }
    }

    #[must_use]
    #[inline(always)]
    fn ssc_only_with_default(key: &'a [u8], default: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        Prop {
            key,
            value,
            default_value: Some(default),
            ssc_only: true,
            ..Default::default()
        }
    }

    #[must_use]
    #[inline(always)]
    fn nonempty_only(key: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        Prop {
            key,
            value,
            nonempty_value_only: true,
            ..Default::default()
        }
    }

    #[must_use]
    #[inline(always)]
    fn own_timing_only(key: &'a [u8], value: PropValue<'a>) -> Prop<'a> {
        Prop {
            key,
            value,
            own_timing_only: true,
            ..Default::default()
        }
    }

    #[must_use]
    #[inline(always)]
    fn own_timing_only_with_default(
        key: &'a [u8],
        default: &'a [u8],
        value: PropValue<'a>,
    ) -> Prop<'a> {
        Prop {
            key,
            value,
            default_value: Some(default),
            own_timing_only: true,
            ..Default::default()
        }
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

    let mut written_bytes = 0;

    #[rustfmt::skip]
    let props = [
        Prop::ssc_only_with_default(b"VERSION", DEFAULT_VERSION, PropValue::Version(summary.ssc_version)),
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
        Prop::nonempty_only(b"DISPLAYBPM", PropValue::Str(&summary.display_bpm_str)),
        Prop::new_with_default(b"BPMS", DEFAULT_BPMS, PropValue::NormalizedList(&summary.normalized_bpms)),
        Prop::new(b"STOPS", PropValue::NormalizedList(&summary.normalized_stops)),
        Prop::ssc_only(b"DELAYS", PropValue::NormalizedList(&summary.normalized_delays)),
        Prop::ssc_only(b"WARPS", PropValue::NormalizedList(&summary.normalized_warps)),
        Prop::ssc_only_with_default(b"TIMESIGNATURES", DEFAULT_TIME_SIGNATURES, PropValue::NormalizedList(&summary.normalized_time_signatures)),
        Prop::ssc_only_with_default(b"TICKCOUNTS", DEFAULT_TICKCOUNTS, PropValue::NormalizedList(&summary.normalized_tickcounts)),
        Prop::ssc_only_with_default(b"COMBOS", DEFAULT_COMBOS, PropValue::NormalizedList(&summary.normalized_combos)),
        Prop::ssc_only_with_default(b"SPEEDS", DEFAULT_SPEEDS, PropValue::NormalizedList(&summary.normalized_speeds)),
        Prop::ssc_only_with_default(b"SCROLLS", DEFAULT_SCROLLS, PropValue::NormalizedList(&summary.normalized_scrolls)),
        Prop::ssc_only(b"FAKES", PropValue::NormalizedList(&summary.normalized_fakes)),
        Prop::ssc_only_with_default(b"LABELS", DEFAULT_LABELS, PropValue::NormalizedList(&summary.normalized_labels)),
        Prop::ssc_nonempty_only(b"LASTSECONDHINT", PropValue::NumberOpt(None)), // TODO: add SimfileSummary.last_second_hint
        Prop::new(b"BGCHANGES", PropValue::NormalizedList(&summary.normalized_bgchanges)),
        Prop::nonempty_only(b"FGCHANGES", PropValue::NormalizedList(&summary.normalized_fgchanges)),
        Prop::new(b"KEYSOUNDS", PropValue::NormalizedList(&summary.normalized_keysounds)),
        Prop::new(b"ATTACKS", PropValue::NormalizedList(&summary.normalized_attacks)),
    ];

    for prop in props {
        written_bytes += prop.serialize(ssc, false, out)?;
    }

    written_bytes += write_all!(out, b"\n")?;

    for chart in &summary.charts {
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
#[inline(always)]
fn write_sm_chart_field(value: &str, out: &mut dyn io::Write) -> io::Result<usize> {
    let mut written_bytes = 0;
    written_bytes += write_all!(out, b"     ")?;
    written_bytes += out.write(value.as_bytes())?;
    written_bytes += write_all!(out, b":\n")?;
    Ok(written_bytes)
}

#[must_use]
fn serialize_sm_chart(out: &mut dyn io::Write, chart: &crate::ChartSummary) -> io::Result<usize> {
    let mut written_bytes = 0;
    written_bytes += write_all!(out, b"#NOTES:\n")?;

    // Kludge: don't use `Prop#serialize` here because SM charts are special.
    // We use `Prop` anyway for the sake of `new_with_default`.
    #[rustfmt::skip]
    let props = [
        Prop::new_with_default(b"", DEFAULT_STEPSTYPE, PropValue::Str(&chart.step_type_str)),
        Prop::new(b"", PropValue::Str(&chart.description_str)),
        Prop::new_with_default(b"", DEFAULT_DIFFICULTY, PropValue::Str(&chart.difficulty_str)),
        Prop::new_with_default(b"", DEFAULT_METER, PropValue::Str(&chart.rating_str)),
        Prop::new(b"", PropValue::RadarValues(chart.cached_radar_values)),
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
        Prop::nonempty_only(b"MUSIC", PropValue::Str(&chart.music_path)), // TODO
        Prop::new(b"RADARVALUES", PropValue::RadarValues(chart.cached_radar_values)),
        Prop::new(b"CREDIT", PropValue::Str(&chart.step_artist_str)),
        Prop::own_timing_only(b"OFFSET", PropValue::Number(chart.chart_offset_seconds)),
        Prop::own_timing_only_with_default(b"BPMS", DEFAULT_BPMS, PropValue::NormalizedListOpt(chart.chart_bpms.as_deref())),
        Prop::own_timing_only(b"STOPS", PropValue::NormalizedListOpt(chart.chart_stops.as_deref())),
        Prop::own_timing_only(b"DELAYS", PropValue::NormalizedListOpt(chart.chart_delays.as_deref())),
        Prop::own_timing_only(b"WARPS", PropValue::NormalizedListOpt(chart.chart_warps.as_deref())),
        Prop::own_timing_only_with_default(b"TIMESIGNATURES", DEFAULT_TIME_SIGNATURES, PropValue::NormalizedListOpt(chart.chart_time_signatures.as_deref())),
        Prop::own_timing_only_with_default(b"TICKCOUNTS", DEFAULT_TICKCOUNTS, PropValue::NormalizedListOpt(chart.chart_tickcounts.as_deref())),
        Prop::own_timing_only_with_default(b"COMBOS", DEFAULT_COMBOS, PropValue::NormalizedListOpt(chart.chart_combos.as_deref())),
        Prop::own_timing_only_with_default(b"SPEEDS", DEFAULT_SPEEDS, PropValue::NormalizedListOpt(chart.chart_speeds.as_deref())),
        Prop::own_timing_only_with_default(b"SCROLLS", DEFAULT_SCROLLS, PropValue::NormalizedListOpt(chart.chart_scrolls.as_deref())),
        Prop::own_timing_only(b"FAKES", PropValue::NormalizedListOpt(chart.chart_fakes.as_deref())),
        Prop::own_timing_only_with_default(b"LABELS", DEFAULT_LABELS, PropValue::NormalizedListOpt(chart.chart_labels.as_deref())),
        Prop::nonempty_only(b"ATTACKS", PropValue::NormalizedListOpt(chart.chart_attacks.as_deref())),
        Prop::nonempty_only(b"DISPLAYBPM", PropValue::StrOpt(chart.chart_display_bpm.as_deref())),
        Prop::nonempty_only(b"NOTES", PropValue::NoteData(&chart.minimized_note_data)),
        Prop::nonempty_only(b"NOTES2", PropValue::Empty), // TODO
    ];

    for prop in props {
        written_bytes += prop.serialize(true, chart.chart_has_own_timing, out)?;
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
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #RADARVALUES:0.010,0.020,0.030,0.040,0.050,0.060,0.070,0.080,0.090,0.100,0.110,0.120,0.130,0.140;\n\
            #CREDIT:Step artist;\n\
            #NOTES:\n\
            0000\n\
            0000\n\
            0000\n\
            0000\n\
            ;\n\
            \n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #RADARVALUES:0.010,0.020,0.030,0.040,0.050,0.060,0.070,0.080,0.090,0.100,0.110,0.120,0.130,0.140;\n\
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
            #NOTES:\n\
            0000\n\
            0000\n\
            0000\n\
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
            #LASTSECONDHINT:120.000000;\n\
            #BGCHANGES:;\n\
            #FGCHANGES:;\n\
            #KEYSOUNDS:;\n\
            #ATTACKS:;\n\
            \n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #MUSIC:chart_music.ogg;\n\
            #RADARVALUES:0.010,0.020,0.030,0.040,0.050,0.060,0.070,0.080,0.090,0.100,0.110,0.120,0.130,0.140;\n\
            #CREDIT:Step artist;\n\
            #ATTACKS:;\n\
            #DISPLAYBPM:300;\n\
            #NOTES:\n\
            0000\n\
            0000\n\
            0000\n\
            0000\n\
            ;\n\
            \n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Challenge;\n\
            #METER:17;\n\
            #MUSIC:chart_music.ogg;\n\
            #RADARVALUES:0.010,0.020,0.030,0.040,0.050,0.060,0.070,0.080,0.090,0.100,0.110,0.120,0.130,0.140;\n\
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
            #ATTACKS:;\n\
            #DISPLAYBPM:300;\n\
            #NOTES:\n\
            0000\n\
            0000\n\
            0000\n\
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
            #STOPS:1.000=1.250,\n\
            1.500=1.750;\n\
            #DELAYS:2.000=2.250,\n\
            2.500=2.750;\n\
            #WARPS:3.000=3.250,\n\
            3.500=3.750;\n\
            #TIMESIGNATURES:0.000000=4=4;\n\
            #TICKCOUNTS:0.000000=4;\n\
            #COMBOS:0.000000=1;\n\
            #SPEEDS:0.000000=1.000000=0.000000=0;\n\
            #SCROLLS:0.000000=1.000000;\n\
            #FAKES:4.000=4.250,\n\
            4.500=4.750;\n\
            #LABELS:0.000000=Song Start;\n\
            #BGCHANGES:;\n\
            #KEYSOUNDS:;\n\
            #ATTACKS:;\n\
            \n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Beginner;\n\
            #METER:1;\n\
            #RADARVALUES:0.010,0.020,0.030,0.040,0.050,0.060,0.070,0.080,0.090,0.100,0.110,0.120,0.130,0.140;\n\
            #CREDIT:Step artist;\n\
            #NOTES:\n\
            0000\n\
            0000\n\
            0000\n\
            0000\n\
            ;\n\
            \n\
            #NOTEDATA:;\n\
            #CHARTNAME:Chart name;\n\
            #STEPSTYPE:dance-single;\n\
            #DESCRIPTION:Description;\n\
            #CHARTSTYLE:Chart style;\n\
            #DIFFICULTY:Beginner;\n\
            #METER:1;\n\
            #RADARVALUES:0.010,0.020,0.030,0.040,0.050,0.060,0.070,0.080,0.090,0.100,0.110,0.120,0.130,0.140;\n\
            #CREDIT:Step artist;\n\
            #OFFSET:0.000000;\n\
            #BPMS:0.000000=60.000000;\n\
            #STOPS:1.000=1.250,\n\
            1.500=1.750;\n\
            #DELAYS:2.000=2.250,\n\
            2.500=2.750;\n\
            #WARPS:3.000=3.250,\n\
            3.500=3.750;\n\
            #TIMESIGNATURES:0.000000=4=4;\n\
            #TICKCOUNTS:0.000000=4;\n\
            #COMBOS:0.000000=1;\n\
            #SPEEDS:0.000000=1.000000=0.000000=0;\n\
            #SCROLLS:0.000000=1.000000;\n\
            #FAKES:4.000=4.250,\n\
            4.500=4.750;\n\
            #LABELS:0.000000=Song Start;\n\
            #NOTES:\n\
            0000\n\
            0000\n\
            0000\n\
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
        #BPMS:0.000=120.000,\n\
        16.000=240.000,\n\
        48.000=120.000;\n\
        #STOPS:1.000=1.250,\n\
        1.500=1.750;\n\
        #BGCHANGES:;\n\
        #KEYSOUNDS:;\n\
        #ATTACKS:;\n\
        \n\
        #NOTES:\n     dance-single:\n     Description:\n     Challenge:\n     17:\n     0.010,0.020,0.030,0.040,0.050,0.060,0.070,0.080,0.090,0.100,0.110,0.120,0.130,0.140:\n\
        0000\n\
        0000\n\
        0000\n\
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
        #BPMS:0.000=120.000,\n\
        16.000=240.000,\n\
        48.000=120.000;\n\
        #STOPS:1.000=1.250,\n\
        1.500=1.750;\n\
        #BGCHANGES:;\n\
        #FGCHANGES:;\n\
        #KEYSOUNDS:;\n\
        #ATTACKS:;\n\
        \n\
        #NOTES:\n     dance-single:\n     Description:\n     Challenge:\n     17:\n     0.010,0.020,0.030,0.040,0.050,0.060,0.070,0.080,0.090,0.100,0.110,0.120,0.130,0.140:\n\
        0000\n\
        0000\n\
        0000\n\
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
        #STOPS:1.000=1.250,\n\
        1.500=1.750;\n\
        #BGCHANGES:;\n\
        #KEYSOUNDS:;\n\
        #ATTACKS:;\n\
        \n\
        #NOTES:\n     dance-single:\n     Description:\n     Beginner:\n     1:\n     0.010,0.020,0.030,0.040,0.050,0.060,0.070,0.080,0.090,0.100,0.110,0.120,0.130,0.140:\n\
        0000\n\
        0000\n\
        0000\n\
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
            normalized_bpms: match trigger_defaults {
                false => String::from("0.000=120.000,16.000=240.000,48.000=120.000"),
                true => String::from(""),
            },
            normalized_stops: String::from("1.000=1.250,1.500=1.750"),
            normalized_delays: String::from("2.000=2.250,2.500=2.750"),
            normalized_warps: String::from("3.000=3.250,3.500=3.750"),
            normalized_speeds: match trigger_defaults {
                false => {
                    String::from("0.000=1.000=0.000=0,12.000=0.500=4.000=0,48.000=1.000=0.000=1")
                }
                true => String::from(""),
            },
            normalized_scrolls: match trigger_defaults {
                false => String::from("0.000=1.000,16.000=2.000,48.000=1.000"),
                true => String::from(""),
            },
            normalized_fakes: String::from("4.000=4.250,4.500=4.750"),
            normalized_time_signatures: match trigger_defaults {
                false => String::from("0.000=4=4,16.000=8=4,48.000=4=4"),
                true => String::from(""),
            },
            normalized_labels: match trigger_defaults {
                false => String::from("0.000=Song Start,16.000=Speedup"),
                true => String::from(""),
            },
            normalized_tickcounts: match trigger_defaults {
                false => String::from("0.000=4,16.000=2,48.000=4"),
                true => String::from(""),
            },
            normalized_combos: match trigger_defaults {
                false => String::from("0.000=1,16.000=2,48.000=1"),
                true => String::from(""),
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
            normalized_bgchanges: Default::default(), // TODO
            normalized_fgchanges: if include_nonempty {
                Default::default() // TODO
            } else {
                Default::default()
            },
            normalized_keysounds: Default::default(), // TODO
            normalized_attacks: Default::default(),   // TODO
            previewvid_path: String::from("previewvid.mov"),
            cdimage_path: String::from("cdimage.png"),
            discimage_path: String::from("discimage.png"),
            lyrics_path: String::from("lyrics.lrc"),
            selectable: true,
            // last_second_hint: include_nonempty.then(|| 120.0), // TODO

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
            minimized_note_data: b"0000\n0000\n0000\n0000\n".to_vec(), // TODO
            music_path: match include_nonempty {
                true => String::from("chart_music.ogg"),
                false => String::from(""),
            },

            // Timing fields
            chart_has_own_timing: has_own_timing,
            chart_bpms: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000=120.000,16.000=240.000,48.000=120.000")),
            chart_stops: has_own_timing.then(|| String::from("1.000=1.250,1.500=1.750")),
            chart_delays: has_own_timing.then(|| String::from("2.000=2.250,2.500=2.750")),
            chart_warps: has_own_timing.then(|| String::from("3.000=3.250,3.500=3.750")),
            chart_speeds: (has_own_timing && !trigger_defaults).then(|| {
                String::from("0.000=1.000=0.000=0,12.000=0.500=4.000=0,48.000=1.000=0.000=1")
            }),
            chart_scrolls: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000=1.000,16.000=2.000,48.000=1.000")),
            chart_fakes: has_own_timing.then(|| String::from("4.000=4.250,4.500=4.750")),
            chart_time_signatures: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000=4=4,16.000=8=4,48.000=4=4")),
            chart_labels: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000=Song Start,16.000=Speedup")),
            chart_tickcounts: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000=4,16.000=2,48.000=4")),
            chart_combos: (has_own_timing && !trigger_defaults)
                .then(|| String::from("0.000=1,16.000=2,48.000=1")),
            chart_attacks: include_nonempty.then(|| Default::default()), // TODO
            chart_display_bpm: include_nonempty.then(|| String::from("300")),

            // These are currently unused in favor of the `chart_*` fields above
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
        }
    }
}
