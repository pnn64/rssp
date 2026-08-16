use std::fmt::Write as _;

pub const CHART_COUNT: usize = 256;

pub fn fixture() -> Vec<u8> {
    let mut data = String::with_capacity(CHART_COUNT * 180);
    data.push_str("#VERSION:0.83;\n#OFFSET:0;\n#BPMS:0=120;\n");
    for index in 0..CHART_COUNT {
        write!(
            &mut data,
            concat!(
                "#NOTEDATA:;\n",
                "#STEPSTYPE:dance-single;\n",
                "#DESCRIPTION:display-{index};\n",
                "#DIFFICULTY:Challenge;\n",
                "#METER:10;\n",
                "#DISPLAYBPM: 120:240 ;\n",
                "#NOTES:\n1000\n0000\n0000\n0000\n;\n"
            ),
            index = index,
        )
        .expect("writing to a String cannot fail");
    }
    data.into_bytes()
}

pub fn compute(data: &[u8], legacy: bool) -> Vec<rssp::bpm::ChartBpmSnapshot> {
    rssp::bpm::chart_bpm_snapshots_for_bench(data, "ssc", legacy)
        .expect("display BPM fixture should parse")
}

pub fn assert_behavior(data: &[u8]) {
    let legacy = compute(data, true);
    let current = compute(data, false);
    assert_eq!(current, legacy);
    assert_eq!(current.len(), CHART_COUNT);
    assert!(current.iter().all(|snapshot| {
        snapshot.display_bpm == "120 - 240"
            && snapshot.display_bpm_min == 120.0
            && snapshot.display_bpm_max == 240.0
    }));

    let mut edge = b"#VERSION:0.83;#BPMS:0=120;#NOTEDATA:;#STEPSTYPE:dance-single;#DESCRIPTION:edge;#DIFFICULTY:Challenge;#METER:10;#DISPLAYBPM:".to_vec();
    edge.extend_from_slice(b"\xA0120:240\xA0");
    edge.extend_from_slice(b";#NOTES:\n1000\n0000\n0000\n0000\n;");
    let legacy = compute(&edge, true);
    let current = compute(&edge, false);
    assert_eq!(current, legacy);
    assert_eq!(current[0].display_bpm, "120 - 240");
}
