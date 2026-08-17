use std::fmt::Write as _;

pub const CHART_COUNT: usize = 128;
pub const TYPICAL_CHART_COUNT: usize = 10;

pub fn fixture() -> Vec<u8> {
    fixture_with_charts(CHART_COUNT)
}

pub fn fixture_with_charts(chart_count: usize) -> Vec<u8> {
    let mut data = String::with_capacity(chart_count * 700);
    data.push_str(concat!(
        "#TITLE:Dispatch fixture;\n",
        "#SUBTITLE:All tags;\n",
        "#ARTIST:Parser;\n",
        "#GENRE:Benchmark;\n",
        "#TITLETRANSLIT:Dispatch;\n",
        "#SUBTITLETRANSLIT:Tags;\n",
        "#ARTISTTRANSLIT:Parser;\n",
        "#VERSION:0.83;\n",
        "#OFFSET:-0.125;\n",
        "#BPMS:0=120,64=180;\n",
        "#STOPS:32=0.125;\n",
        "#DELAYS:48=0.0625;\n",
        "#TIMESIGNATURES:0=4=4;\n",
        "#TICKCOUNTS:0=4;\n",
        "#BANNER:banner.png;\n",
        "#BACKGROUND:background.png;\n",
        "#CDTITLE:cdtitle.png;\n",
        "#JACKET:jacket.png;\n",
        "#MUSIC:song.ogg;\n",
        "#SAMPLESTART:12.5;\n",
        "#SAMPLELENGTH:15;\n",
        "#DISPLAYBPM:120:180;\n",
        "#SELECTABLE:YES;\n",
        "#LYRICSPATH:lyrics.lrc;\n",
        "#CREDIT:Benchmark Author;\n",
        "#ATTACKS:TIME=1:LEN=2:MODS=mirror;\n",
        "#ATTACKS:TIME=4:LEN=1:MODS=reverse;\n",
        "#BGCHANGES:0=background.png=1.0=0=0=0;\n",
        "#FGCHANGES:0=foreground.png=1.0=0=0=0;\n",
        "#KEYSOUNDS:kick.wav,snare.wav;\n",
        "#ORIGIN:rssp;\n",
        "#PREVIEWVID:preview.mp4;\n",
        "#CDIMAGE:cd.png;\n",
        "#DISCIMAGE:disc.png;\n",
        "#FAKES:96=4;\n",
        "#WARPS:128=8;\n",
        "#SPEEDS:0=1=0=0;\n",
        "#SCROLLS:0=1;\n",
        "#LABELS:0=Song Start;\n",
        "#COMBOS:0=1;\n",
        "#LASTSECONDHINT:240;\n",
        "#UNKNOWNHEADER:ignored;\n",
    ));
    for index in 0..chart_count {
        write!(
            &mut data,
            concat!(
                "#NOTEDATA:;\n",
                "#STEPSTYPE:dance-single;\n",
                "#CHARTNAME:chart-{index};\n",
                "#CHARTSTYLE:pad;\n",
                "#DESCRIPTION:dispatch;\n",
                "#CREDIT:Author;\n",
                "#DIFFICULTY:Challenge;\n",
                "#METER:12;\n",
                "#MUSIC:chart.ogg;\n",
                "#ATTACKS:TIME=1:LEN=2:MODS=mirror;\n",
                "#ATTACKS:TIME=4:LEN=1:MODS=reverse;\n",
                "#BPMS:0=120,4=180;\n",
                "#STOPS:2=0.125;\n",
                "#FREEZES:3=0.25;\n",
                "#DELAYS:6=0.0625;\n",
                "#WARPS:8=1;\n",
                "#SPEEDS:0=1=0=0;\n",
                "#SCROLLS:0=1;\n",
                "#FAKES:12=1;\n",
                "#OFFSET:-0.01;\n",
                "#DISPLAYBPM:120:180;\n",
                "#TIMESIGNATURES:0=4=4;\n",
                "#LABELS:0=Start;\n",
                "#TICKCOUNTS:0=4;\n",
                "#COMBOS:0=1;\n",
                "#RADARVALUES:1,2,3,4,5;\n",
                "#UNKNOWNCHART:ignored;\n",
                "#NOTES:\n1000\n0100\n0010\n0001\n;\n",
            ),
            index = index,
        )
        .expect("writing to a String cannot fail");
    }
    data.into_bytes()
}

pub fn sm_fixture(chart_count: usize) -> Vec<u8> {
    let mut data = String::with_capacity(chart_count * 120);
    data.push_str("#TITLE:SM reserve fixture;\n#ARTIST:Parser;\n#BPMS:0=120;\n");
    for index in 0..chart_count {
        write!(
            &mut data,
            concat!(
                "#NOTES:\n",
                "dance-single:\n",
                "chart-{index}:\n",
                "Challenge:\n",
                "12:\n",
                "1,2,3,4,5:\n",
                "1000\n0100\n0010\n0001\n;\n",
            ),
            index = index,
        )
        .expect("writing to a String cannot fail");
    }
    data.into_bytes()
}

pub fn parse<'a>(data: &'a [u8], ext: &str, legacy: bool) -> rssp::parse::ParsedSimfileData<'a> {
    rssp::parse::extract_sections_for_bench(data, ext, legacy)
        .expect("dispatch fixture should parse")
}

pub fn parse_reserved<'a>(
    data: &'a [u8],
    ext: &str,
    legacy: bool,
) -> rssp::parse::ParsedSimfileData<'a> {
    rssp::parse::extract_sections_reserve_for_bench(data, ext, legacy)
        .expect("chart reserve fixture should parse")
}

pub fn parse_append<'a>(
    data: &'a [u8],
    ext: &str,
    legacy: bool,
) -> rssp::parse::ParsedSimfileData<'a> {
    rssp::parse::extract_sections_append_for_bench(data, ext, legacy)
        .expect("attack append fixture should parse")
}

fn global_fields<'a>(parsed: &'a rssp::parse::ParsedSimfileData<'a>) -> [Option<&'a [u8]>; 40] {
    [
        parsed.title,
        parsed.subtitle,
        parsed.artist,
        parsed.genre,
        parsed.title_translit,
        parsed.subtitle_translit,
        parsed.artist_translit,
        parsed.version,
        parsed.offset,
        parsed.origin,
        parsed.credit,
        parsed.attacks.as_deref(),
        parsed.bpms,
        parsed.stops,
        parsed.delays,
        parsed.warps,
        parsed.speeds,
        parsed.scrolls,
        parsed.fakes,
        parsed.time_signatures,
        parsed.labels,
        parsed.tickcounts,
        parsed.combos,
        parsed.banner,
        parsed.background,
        parsed.cdtitle,
        parsed.jacket,
        parsed.music,
        parsed.sample_start,
        parsed.sample_length,
        parsed.display_bpm,
        parsed.selectable,
        parsed.lyricspath,
        parsed.previewvid,
        parsed.cdimage,
        parsed.discimage,
        parsed.bgchanges,
        parsed.fgchanges,
        parsed.keysounds,
        parsed.last_second_hint,
    ]
}

fn chart_fields<'a>(entry: &'a rssp::parse::ParsedChartEntry<'a>) -> [Option<&'a [u8]>; 19] {
    [
        entry.chart_name,
        entry.chart_style,
        Some(entry.note_data),
        entry.chart_music.as_deref(),
        entry.chart_attacks.as_deref(),
        entry.chart_bpms.as_deref(),
        entry.chart_stops.as_deref(),
        entry.chart_delays.as_deref(),
        entry.chart_warps.as_deref(),
        entry.chart_speeds.as_deref(),
        entry.chart_scrolls.as_deref(),
        entry.chart_fakes.as_deref(),
        entry.chart_offset.as_deref(),
        entry.chart_display_bpm.as_deref(),
        entry.chart_time_signatures.as_deref(),
        entry.chart_labels.as_deref(),
        entry.chart_tickcounts.as_deref(),
        entry.chart_combos.as_deref(),
        entry.chart_radar_values.as_deref(),
    ]
}

fn assert_parsed_eq(
    current: &rssp::parse::ParsedSimfileData<'_>,
    legacy: &rssp::parse::ParsedSimfileData<'_>,
) {
    assert_eq!(global_fields(&current), global_fields(&legacy));
    assert_eq!(current.notes_list.len(), legacy.notes_list.len());
    for (current, legacy) in current.notes_list.iter().zip(&legacy.notes_list) {
        assert_eq!(current.field_count, legacy.field_count);
        assert_eq!(current.fields, legacy.fields);
        assert_eq!(chart_fields(current), chart_fields(legacy));
    }
}

fn assert_same(data: &[u8], ext: &str) {
    let legacy = parse(data, ext, true);
    let current = parse(data, ext, false);
    assert_parsed_eq(&current, &legacy);
}

#[allow(dead_code)]
pub fn assert_pair(data: &[u8], ext: &str) {
    assert_same(data, ext);
}

pub fn assert_reserve_pair(data: &[u8], ext: &str) {
    let legacy = parse_reserved(data, ext, true);
    let current = parse_reserved(data, ext, false);
    assert_parsed_eq(&current, &legacy);
}

pub fn assert_reserve_behavior() {
    for chart_count in [1, TYPICAL_CHART_COUNT, CHART_COUNT] {
        let data = fixture_with_charts(chart_count);
        assert_reserve_pair(&data, "ssc");
        assert_eq!(
            parse_reserved(&data, "ssc", false).notes_list.len(),
            chart_count
        );
    }
    let sm = sm_fixture(TYPICAL_CHART_COUNT);
    assert_reserve_pair(&sm, "sm");
    assert_eq!(
        parse_reserved(&sm, "sm", false).notes_list.len(),
        TYPICAL_CHART_COUNT
    );
}

pub fn assert_append_behavior(data: &[u8], ext: &str) {
    let legacy = parse_append(data, ext, true);
    let current = parse_append(data, ext, false);
    assert_parsed_eq(&current, &legacy);
    assert!(matches!(current.attacks, Some(std::borrow::Cow::Owned(_))));
    assert!(
        current
            .notes_list
            .iter()
            .all(|entry| matches!(entry.chart_attacks, Some(std::borrow::Cow::Owned(_))))
    );

    for case in [
        b"#TITLE:No attacks;".as_slice(),
        b"#ATTACKS:TIME=1:LEN=2:MODS=mirror;".as_slice(),
        b"#ATTACKS:first;\n#ATTACKS:second;\n#ATTACKS:third;".as_slice(),
    ] {
        let legacy = parse_append(case, ext, true);
        let current = parse_append(case, ext, false);
        assert_parsed_eq(&current, &legacy);
    }

    let single = parse_append(b"#ATTACKS:single;", ext, false);
    assert!(matches!(
        single.attacks,
        Some(std::borrow::Cow::Borrowed(_))
    ));
    let triple = parse_append(
        b"#ATTACKS:first;\n#ATTACKS:second;\n#ATTACKS:third;",
        ext,
        false,
    );
    assert_eq!(
        triple.attacks.as_deref(),
        Some(b"first:second:third".as_slice())
    );
}

pub fn assert_behavior(data: &[u8]) {
    assert_same(data, "ssc");
    assert_eq!(parse(data, "ssc", false).notes_list.len(), CHART_COUNT);
    assert_same(
        concat!(
            "#tItLe:Mixed Case;\n",
            "#aRtIsT:Parser;\n",
            "#fReEzEs:0=0.25;\n",
            "#oRiGiN:ignored for sm;\n",
            "#unknown:ignored;\n",
            "#nOtEs:\n",
            "dance-single:\n",
            "description:\n",
            "Hard:\n",
            "9:\n",
            "0,0,0,0,0:\n",
            "1000\n0100\n0010\n0001\n;\n",
        )
        .as_bytes(),
        "sm",
    );
}
