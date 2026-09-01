pub const LAST_BEAT_BATCH: usize = 128;
pub const MEASURE_COUNT: usize = 64;
pub const ROW_COUNT: usize = 16;
pub const DENSE_ROW_COUNT: usize = 96;

pub fn chart(measures: usize, rows: usize) -> Vec<u8> {
    chart_for_lanes(4, measures, rows)
}

fn chart_for_lanes(lanes: usize, measures: usize, rows: usize) -> Vec<u8> {
    let mut chart = Vec::with_capacity(measures * (rows * (lanes + 1) + 2));
    for measure in 0..measures {
        for row in 0..rows {
            for lane in 0..lanes {
                let byte = if row == (measure * 3 + 1) % rows && lane == measure % lanes {
                    b'1'
                } else if row == (measure * 5 + 3) % rows && lane == (measure + 1) % lanes {
                    b'M'
                } else {
                    b'0'
                };
                chart.push(byte);
            }
            chart.push(b'\n');
        }
        chart.extend_from_slice(if measure + 1 == measures {
            b";\n"
        } else {
            b",\n"
        });
    }
    chart
}

fn assert_case(data: &[u8], lanes: usize) {
    let legacy = rssp::stats::chart_last_beat_for_bench(data, lanes, true);
    let stack = rssp::stats::chart_last_beat_for_bench(data, lanes, false);
    assert_eq!(stack.to_bits(), legacy.to_bits());
    let stack64 = rssp::stats::chart_last_beat_stack_for_bench(data, lanes, false);
    let stack96 = rssp::stats::chart_last_beat_stack_for_bench(data, lanes, true);
    assert_eq!(stack96.to_bits(), stack64.to_bits());
}

pub fn assert_behavior() {
    assert_case(b"", 4);
    assert_case(b"// comment\n1000\n0000\n0100\n0000\n;\n", 4);
    assert_case(b"2000\n0000\n3000\n0000\n,\n0000\n;\n", 4);
    assert_case(b"  1000\r\ninvalid\r\n,\r\n  0001\r\n", 4);
    for lanes in [4, 5, 8, 10] {
        for rows in [1, 4, 16, 63, 64, 65, 96, 128] {
            assert_case(&chart_for_lanes(lanes, 8, rows), lanes);
        }
    }
}
