use std::fmt::Write as _;

pub const CHART_COUNT: usize = 256;

pub fn fixture() -> String {
    let mut bpms = String::with_capacity(24 * 16);
    let mut stops = String::with_capacity(8 * 16);
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
        }
        write!(&mut stops, "{}=0.125", idx * 8 + 2)
            .expect("writing benchmark stops to a String cannot fail");
    }

    let mut data = String::with_capacity(CHART_COUNT * (bpms.len() + stops.len() + 192));
    data.push_str("#VERSION:0.83;\n#OFFSET:0;\n#BPMS:0=120;\n");
    for idx in 0..CHART_COUNT {
        write!(
            &mut data,
            concat!(
                "#NOTEDATA:;\n",
                "#STEPSTYPE:dance-single;\n",
                "#DESCRIPTION:repeated-{idx};\n",
                "#DIFFICULTY:Challenge;\n",
                "#METER:12;\n",
                "#OFFSET:0.125;\n",
                "#BPMS:{bpms};\n",
                "#STOPS:{stops};\n",
                "#NOTES:\n0000\n0000\n0000\n1000\n;\n"
            ),
            idx = idx,
            bpms = bpms,
            stops = stops,
        )
        .expect("writing repeated timing fixture to a String cannot fail");
    }
    data
}

pub fn compute(data: &[u8], cache: bool) -> Vec<rssp::ChartDuration> {
    rssp::duration::chart_durations_cache_for_bench(
        data,
        "ssc",
        rssp::TimingOffsets::default(),
        cache,
    )
    .expect("repeated timing fixture should parse")
}

pub fn assert_behavior(data: &[u8]) {
    let uncached = compute(data, false);
    let cached = compute(data, true);
    assert_eq!(cached.len(), CHART_COUNT);
    assert_eq!(cached.len(), uncached.len());
    for (actual, expected) in cached.iter().zip(uncached) {
        assert_eq!(actual.step_type, expected.step_type);
        assert_eq!(actual.difficulty, expected.difficulty);
        assert_eq!(actual.duration_seconds, expected.duration_seconds);
    }
}
