pub const MEASURE_COUNT: usize = 1_024;
const ROWS_PER_MEASURE: usize = 48;

pub fn fixture() -> Vec<u8> {
    let mut fixture = Vec::with_capacity(MEASURE_COUNT * ROWS_PER_MEASURE * 5);
    fixture.extend_from_slice(
        concat!(
            "#VERSION:0.83;\n",
            "#BPMS:0=180;\n",
            "#NOTEDATA:;\n",
            "#STEPSTYPE:dance-single;\n",
            "#DESCRIPTION:NPS report benchmark;\n",
            "#DIFFICULTY:Challenge;\n",
            "#METER:12;\n",
            "#NOTES:\n",
        )
        .as_bytes(),
    );

    for measure in 0..MEASURE_COUNT {
        for row in 0..ROWS_PER_MEASURE {
            let lane = (measure + row) & 3;
            let has_note = measure % 2 == 0 || row % 5 != 0;
            for column in 0..4 {
                fixture.push(if has_note && column == lane {
                    b'1'
                } else {
                    b'0'
                });
            }
            fixture.push(b'\n');
        }
        if measure + 1 != MEASURE_COUNT {
            fixture.extend_from_slice(b",\n");
        }
    }
    fixture.extend_from_slice(b";\n");
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
