use super::{black_box, measure};

#[path = "timing_fixtures.rs"]
mod fixtures;

fn descriptions(version: &str, kind: &str, length: usize) -> Vec<u8> {
    let mut desc = match kind {
        "plain" => vec![b'a'; length],
        "escaped" => b"\\a".repeat(length / 2),
        "cp1252" => vec![0xe9; length],
        _ => unreachable!("known fixture kind"),
    };
    desc.extend_from_slice(b" end");
    let mut data = format!("#VERSION:{version};#BPMS:0=120;").into_bytes();
    for _ in 0..4 {
        if version == "sm" {
            data.extend_from_slice(b"#NOTES:dance-single:");
            data.extend_from_slice(&desc);
            data.extend_from_slice(b":invalid:3::1000\n0100\n,\n0010\n0001;\n");
        } else {
            data.extend_from_slice(b"#NOTEDATA:;#STEPSTYPE:dance-single;#DESCRIPTION:");
            data.extend_from_slice(&desc);
            data.extend_from_slice(
                b";#DIFFICULTY:invalid;#METER:3;#NOTES:1000\n0100\n,\n0010\n0001;\n",
            );
        }
    }
    data
}

fn summary() -> rssp::SimfileSummary {
    rssp::analyze(
        b"#VERSION:0.83;#BPMS:0=120;#NOTEDATA:;#STEPSTYPE:dance-single;\
          #DIFFICULTY:Hard;#METER:8;#NOTES:1000\n0100\n0010\n0001;",
        "ssc",
        &rssp::AnalysisOptions::default(),
    )
    .expect("valid timing fixture")
}

pub fn cases(iters: usize) {
    for length in [16, 4096] {
        for version in ["0.6", "0.74", "NaN", "sm"] {
            let extension = if version == "sm" { "sm" } else { "ssc" };
            for kind in ["plain", "escaped", "cp1252"] {
                let data = descriptions(version, kind, length);
                let tag = format!("{version}_{length}_{kind}");
                measure(&format!("description/hash/{tag}"), 4, iters, || {
                    black_box(
                        rssp::compute_all_hashes(black_box(&data), extension)
                            .expect("valid hash fixture"),
                    );
                });
                measure(&format!("description/duration/{tag}"), 4, iters, || {
                    black_box(
                        rssp::duration::compute_chart_durations(
                            black_box(&data),
                            extension,
                            rssp::TimingOffsets::default(),
                        )
                        .expect("valid duration fixture"),
                    );
                });
                measure(&format!("description/peak/{tag}"), 4, iters, || {
                    black_box(
                        rssp::nps::compute_chart_peak_nps(black_box(&data), extension)
                            .expect("valid NPS fixture"),
                    );
                });
            }
        }
    }
    let mut summary = summary();
    summary.charts[0].chart_has_own_timing = false;
    for count in [0, 1, 32, 256] {
        for kind in ["unique", "repeat", "replace", "mixed", "long", "invalid"] {
            summary.normalized_labels = fixtures::labels(count, kind);
            measure(
                &format!("label_snapshot/{count}_{kind}"),
                count,
                iters,
                || {
                    black_box(rssp::report::build_timing_snapshot(
                        black_box(&summary.charts[0]),
                        black_box(&summary),
                    ));
                },
            );
        }
    }
    for count in [0, 1, 32, 512, 1024, 2048] {
        std::sync::Arc::make_mut(&mut summary.charts[0].timing_segments).bpms =
            fixtures::bpms(count);
        let mut output = Vec::with_capacity(262_144);
        measure(&format!("bpm_report/{count}"), count, iters, || {
            output.clear();
            rssp::report::write_reports(
                black_box(&summary),
                rssp::report::OutputMode::JSON,
                black_box(&mut output),
            )
            .expect("Vec write");
            black_box(&output);
        });
    }
}

pub fn verify() {
    for length in [0, 16, 4096] {
        for version in ["0.6", "0.74", "NaN", "sm"] {
            let extension = if version == "sm" { "sm" } else { "ssc" };
            for kind in ["plain", "escaped", "cp1252"] {
                let data = descriptions(version, kind, length);
                println!(
                    "description {version} {length} {kind} {:?} {:?} {:?}",
                    rssp::compute_all_hashes(&data, extension),
                    rssp::duration::compute_chart_durations(
                        &data,
                        extension,
                        rssp::TimingOffsets::default()
                    ),
                    rssp::nps::compute_chart_peak_nps(&data, extension)
                );
            }
        }
    }
    let mut summary = summary();
    summary.charts[0].chart_has_own_timing = false;
    for count in [0, 1, 32, 256] {
        for kind in ["unique", "repeat", "replace", "mixed", "long", "invalid"] {
            summary.normalized_labels = fixtures::labels(count, kind);
            let snapshot = rssp::report::build_timing_snapshot(&summary.charts[0], &summary);
            println!("labels {count} {kind} {:?}", snapshot.labels);
        }
    }
    for count in [0, 1, 32, 512, 1024, 2048] {
        std::sync::Arc::make_mut(&mut summary.charts[0].timing_segments).bpms =
            fixtures::bpms(count);
        let mut output = Vec::new();
        rssp::report::write_reports(&summary, rssp::report::OutputMode::JSON, &mut output)
            .expect("Vec write");
        println!("bpm report {count} {output:?}");
    }
}
