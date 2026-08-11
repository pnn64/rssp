use std::fmt::Write as _;

pub const CHART_COUNT: usize = 256;

pub fn fixture(version: &str) -> String {
    let mut out = String::with_capacity(160 + CHART_COUNT * 180);
    writeln!(
        &mut out,
        concat!(
            "#VERSION:{};\n",
            "#TITLE:  Metadata Performance  ;\n",
            "#SUBTITLE:  Allocation fixture  ;\n",
            "#ARTIST:  RSSP  ;\n",
            "#GENRE:  Benchmark  ;\n",
            "#OFFSET:0;\n",
            "#BPMS:0=120;"
        ),
        version
    )
    .expect("writing to a String should succeed");

    for index in 0..CHART_COUNT {
        writeln!(
            &mut out,
            concat!(
                "#NOTEDATA:;\n",
                "#STEPSTYPE:dance-single;\n",
                "#DESCRIPTION:Description {index};\n",
                "#CHARTNAME:Chart Name {index};\n",
                "#DIFFICULTY:Hard;\n",
                "#METER:{meter};\n",
                "#CREDIT:BR+;\n",
                "#NOTES:\n",
                "1000\n",
                ";"
            ),
            index = index,
            meter = 8 + index % 12,
        )
        .expect("writing to a String should succeed");
    }
    out
}

pub fn options() -> rssp::AnalysisOptions {
    rssp::AnalysisOptions {
        compute_pattern_counts: false,
        compute_tech_counts: false,
        ..rssp::AnalysisOptions::default()
    }
}
