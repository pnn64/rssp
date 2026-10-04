// Run explicitly with --ignored; fixtures and buffers are prepared before timing.
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::time::Instant;

#[path = "timing_fixtures.rs"]
mod fixtures;

fn measure(name: &str, items: usize, mut run: impl FnMut()) {
    if std::env::var("RSSP_REPORT_FILTER").is_ok_and(|filter| !name.contains(&filter)) {
        return;
    }
    let iters = std::env::var("RSSP_REPORT_ITERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(4000);
    for _ in 0..4 {
        run();
    }
    let mut times = [0.0; 7];
    for time in &mut times {
        let start = Instant::now();
        for _ in 0..iters {
            run();
        }
        *time = start.elapsed().as_nanos() as f64 / iters as f64;
    }
    times.sort_unstable_by(f64::total_cmp);
    println!(
        "report_leaf/{name}: ns={:.0} items/s={:.0}",
        times[3],
        items as f64 * 1e9 / times[3]
    );
}

#[test]
#[ignore = "explicit report throughput benchmark"]
fn report_hotpath() {
    let mut writer = BufWriter::with_capacity(8192, Vec::with_capacity(16384));
    for count in [0, 1, 32, 256] {
        for kind in ["unique", "repeat", "replace", "mixed", "long", "invalid"] {
            let text = fixtures::labels(count, kind);
            measure(&format!("labels/{count}_{kind}"), count, || {
                black_box(super::parse_labels(black_box(Some(&text))));
            });
        }
    }
    let mut output = Vec::with_capacity(131_072);
    for count in [0, 1, 32, 512, 1024, 2048] {
        let bpms = fixtures::bpms(count);
        for buffered in [false, true] {
            measure(&format!("native_bpms/{count}_{buffered}"), count, || {
                if buffered {
                    writer.get_mut().clear();
                    super::write_json_native_bpms(black_box(&mut writer), black_box(&bpms))
                        .expect("Vec write");
                    writer.flush().expect("Vec flush");
                    black_box(writer.get_ref());
                } else {
                    output.clear();
                    super::write_json_native_bpms(black_box(&mut output), black_box(&bpms))
                        .expect("Vec write");
                    black_box(&output);
                }
            });
        }
    }
    for key in [
        "title",
        "sn_detailed_breakdown",
        "equally_spaced_per_measure",
    ] {
        for number in [false, true] {
            measure(&format!("object/{key}_{number}"), 1, || {
                writer.get_mut().clear();
                let mut object =
                    super::JsonObjectWriter::new(black_box(&mut writer), 0).expect("Vec write");
                if number {
                    object
                        .field_u32(black_box(key), black_box(7))
                        .expect("Vec write");
                } else {
                    object
                        .field_string(black_box(key), black_box("data"))
                        .expect("Vec write");
                }
                object.finish().expect("Vec write");
                writer.flush().expect("Vec flush");
                black_box(writer.get_ref());
            });
        }
    }
    for indent in [0, 2, 8, 16, 64, 129] {
        measure(&format!("indent/{indent}"), indent, || {
            writer.get_mut().clear();
            super::write_indent(black_box(&mut writer), black_box(indent)).expect("Vec write");
            writer.flush().expect("Vec flush");
            black_box(writer.get_ref());
        });
    }
    for length in [16, 4096] {
        for kind in ["clean", "early", "late", "dense", "comma", "comma_quote"] {
            let mut value = "a".repeat(length);
            match kind {
                "early" => value.replace_range(0..1, "\""),
                "late" => value.replace_range(length - 1..length, "\""),
                "dense" => value = "\\\"\n,".repeat(length / 4),
                "comma" => value.replace_range(length - 1..length, ","),
                "comma_quote" => {
                    value.replace_range(length / 2..=length / 2, ",");
                    value.replace_range(length - 1..length, "\"");
                }
                _ => {}
            }
            measure(&format!("json/{length}_{kind}"), length, || {
                writer.get_mut().clear();
                super::write_json_string(black_box(&mut writer), black_box(&value))
                    .expect("Vec write");
                writer.flush().expect("Vec flush");
                black_box(writer.get_ref());
            });
            measure(&format!("csv/{length}_{kind}"), length, || {
                writer.get_mut().clear();
                let mut row = super::CsvRow::new(black_box(&mut writer));
                super::push_str(&mut row, black_box(&value));
                row.finish().expect("Vec write");
                writer.flush().expect("Vec flush");
                black_box(writer.get_ref());
            });
        }
    }
}
