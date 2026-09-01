use std::fmt::Write as _;

pub const CHART_COUNT: usize = 128;

pub fn fixture() -> String {
    let mut bpms = String::with_capacity(24 * 16);
    let mut stops = String::with_capacity(8 * 16);
    let mut delays = String::with_capacity(8 * 16);
    let mut warps = String::with_capacity(4 * 12);
    let mut speeds = String::with_capacity(8 * 24);
    let mut scrolls = String::with_capacity(8 * 16);
    let mut fakes = String::with_capacity(4 * 12);
    for idx in 0..24 {
        if idx != 0 {
            bpms.push(',');
        }
        write!(&mut bpms, "{}={}", idx * 4, 120 + idx % 7)
            .expect("writing benchmark BPMs to a String cannot fail");
    }
    for idx in 0..8 {
        if idx != 0 {
            stops.push(',');
            delays.push(',');
            speeds.push(',');
            scrolls.push(',');
        }
        write!(&mut stops, "{}=0.125", idx * 8 + 2)
            .expect("writing benchmark stops to a String cannot fail");
        write!(&mut delays, "{}=0.0625", idx * 8 + 3)
            .expect("writing benchmark delays to a String cannot fail");
        write!(&mut speeds, "{}=1.25=2=0", idx * 8)
            .expect("writing benchmark speeds to a String cannot fail");
        write!(&mut scrolls, "{}=0.75", idx * 8)
            .expect("writing benchmark scrolls to a String cannot fail");
    }
    for idx in 0..4 {
        if idx != 0 {
            warps.push(',');
            fakes.push(',');
        }
        write!(&mut warps, "{}=1", idx * 16 + 8)
            .expect("writing benchmark warps to a String cannot fail");
        write!(&mut fakes, "{}=1", idx * 16 + 12)
            .expect("writing benchmark fakes to a String cannot fail");
    }

    let mut data = String::with_capacity(CHART_COUNT * 1_024);
    data.push_str("#VERSION:0.83;\n#TITLE:Timing Cache;\n#BPMS:0=120;\n");
    for idx in 0..CHART_COUNT {
        write!(
            &mut data,
            concat!(
                "#NOTEDATA:;\n",
                "#STEPSTYPE:dance-single;\n",
                "#DESCRIPTION:repeated-{idx};\n",
                "#DIFFICULTY:Challenge;\n",
                "#METER:12;\n",
                "#CREDIT:;\n",
                "#OFFSET:0.125;\n",
                "#BPMS:{bpms};\n",
                "#STOPS:{stops};\n",
                "#DELAYS:{delays};\n",
                "#WARPS:{warps};\n",
                "#SPEEDS:{speeds};\n",
                "#SCROLLS:{scrolls};\n",
                "#FAKES:{fakes};\n",
                "#NOTES:\n1000\n0100\n0010\n0001\n;\n"
            ),
            idx = idx,
            bpms = bpms,
            stops = stops,
            delays = delays,
            warps = warps,
            speeds = speeds,
            scrolls = scrolls,
            fakes = fakes,
        )
        .expect("writing repeated timing fixture to a String cannot fail");
    }
    data
}

pub fn options() -> rssp::AnalysisOptions {
    rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..rssp::AnalysisOptions::default()
    }
}

pub fn compute(
    data: &[u8],
    options: &rssp::AnalysisOptions,
    scratch: &mut rssp::AnalysisScratch,
    cache: bool,
) -> rssp::SimfileSummary {
    rssp::profile::analyze_timing_cache(data, "ssc", options, scratch, cache)
        .expect("repeated timing fixture should analyze")
}

pub fn assert_behavior(data: &[u8], options: &rssp::AnalysisOptions) {
    let uncached = compute(data, options, &mut rssp::AnalysisScratch::default(), false);
    let cached = compute(data, options, &mut rssp::AnalysisScratch::default(), true);
    let mut uncached_json = Vec::new();
    let mut cached_json = Vec::new();
    rssp::report::write_json_all(&uncached, &mut uncached_json)
        .expect("uncached summary should serialize");
    rssp::report::write_json_all(&cached, &mut cached_json)
        .expect("cached summary should serialize");
    assert_eq!(cached_json, uncached_json);
    assert_eq!(cached.charts.len(), CHART_COUNT);
}
