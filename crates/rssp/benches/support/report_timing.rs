use std::fmt::Write as _;

pub const SEGMENT_COUNT: usize = 512;

pub fn fixture() -> String {
    fn push_pairs(out: &mut String, count: usize, mut value: impl FnMut(usize) -> f64) {
        for index in 0..count {
            if index != 0 {
                out.push(',');
            }
            write!(out, "{}={}", index * 4, value(index)).unwrap();
        }
        out.push_str(";\n");
    }

    let mut fixture = String::with_capacity(SEGMENT_COUNT * 180);
    fixture.push_str("#VERSION:0.83;\n#OFFSET:-0.125;\n#BPMS:");
    push_pairs(&mut fixture, SEGMENT_COUNT, |index| {
        90.0 + (index % 211) as f64
    });
    fixture.push_str("#STOPS:");
    push_pairs(&mut fixture, SEGMENT_COUNT, |index| {
        0.01 + (index % 17) as f64 / 100.0
    });
    fixture.push_str("#DELAYS:");
    push_pairs(&mut fixture, SEGMENT_COUNT, |index| {
        0.02 + (index % 13) as f64 / 100.0
    });
    fixture.push_str("#WARPS:");
    push_pairs(&mut fixture, SEGMENT_COUNT, |index| {
        0.5 + (index % 7) as f64
    });

    fixture.push_str("#SPEEDS:");
    for index in 0..SEGMENT_COUNT {
        if index != 0 {
            fixture.push(',');
        }
        write!(
            &mut fixture,
            "{}={}=0.25={}",
            index * 4,
            1.25 + (index % 9) as f64 / 10.0,
            index & 1
        )
        .unwrap();
    }
    fixture.push_str(";\n#SCROLLS:");
    push_pairs(&mut fixture, SEGMENT_COUNT, |index| {
        0.75 + (index % 11) as f64 / 10.0
    });
    fixture.push_str("#FAKES:");
    push_pairs(&mut fixture, SEGMENT_COUNT, |index| {
        0.25 + (index % 5) as f64
    });

    fixture.push_str(concat!(
        "#TIMESIGNATURES:0=4=4,64=3=4,128=7=8;\n",
        "#LABELS:0=Song Start,64=Middle,128=Finale;\n",
        "#TICKCOUNTS:0=4,64=8,128=12;\n",
        "#COMBOS:0=1=1,64=2=3,128=4=5;\n",
        "#NOTEDATA:;\n",
        "#STEPSTYPE:dance-single;\n",
        "#DESCRIPTION:report benchmark;\n",
        "#DIFFICULTY:Challenge;\n",
        "#METER:10;\n",
        "#CREDIT:;\n",
        "#NOTES:\n",
        "1000\n0100\n0010\n0001\n",
        ";\n"
    ));
    fixture
}

pub fn options() -> rssp::AnalysisOptions {
    rssp::AnalysisOptions {
        mono_threshold: 6,
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..rssp::AnalysisOptions::default()
    }
}
