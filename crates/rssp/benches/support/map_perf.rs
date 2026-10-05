use super::{black_box, measure};

#[path = "map_fixtures.rs"]
mod fixtures;

pub fn cases(iters: usize) {
    let mut scratch = rssp::AnalysisScratch::default();
    let options = rssp::AnalysisOptions::default();
    for count in [0, 1, 128, 4096] {
        let data = fixtures::blank_file(count);
        measure(&format!("blank_load/{count}_analyze"), 2, iters, || {
            black_box(
                rssp::analyze_with_scratch(black_box(&data), "ssc", &options, &mut scratch)
                    .expect("valid blank timing fixture"),
            );
        });
        measure(&format!("blank_load/{count}_duration"), 2, iters, || {
            black_box(
                rssp::compute_chart_durations(
                    black_box(&data),
                    "ssc",
                    rssp::TimingOffsets::default(),
                )
                .expect("valid blank timing fixture"),
            );
        });
        measure(&format!("blank_load/{count}_peak"), 2, iters, || {
            black_box(
                rssp::nps::compute_chart_peak_nps(black_box(&data), "ssc")
                    .expect("valid blank timing fixture"),
            );
        });
        measure(&format!("blank_load/{count}_snapshot"), 2, iters, || {
            black_box(
                rssp::bpm::chart_bpm_snapshots(black_box(&data), "ssc")
                    .expect("valid blank timing fixture"),
            );
        });
    }
    for count in [0, 1, 32, 4096] {
        for kind in ["clean", "last"] {
            let raw = fixtures::map(count, kind);
            for (ext, local) in [("sm", false), ("ssc", false), ("ssc", true)] {
                let data = fixtures::simfile(&raw, "", ext, local);
                let source = if local { "local" } else { "global" };
                measure(
                    &format!("snapshot_load/{count}_{kind}_{ext}_{source}"),
                    count,
                    iters,
                    || {
                        black_box(
                            rssp::bpm::chart_bpm_snapshots(black_box(&data), ext)
                                .expect("valid snapshot fixture"),
                        );
                    },
                );
                measure(
                    &format!("raw_duration/{count}_{kind}_{ext}_{source}"),
                    count,
                    iters,
                    || {
                        black_box(
                            rssp::compute_chart_durations(
                                black_box(&data),
                                ext,
                                rssp::TimingOffsets::default(),
                            )
                            .expect("valid timing fixture"),
                        );
                    },
                );
            }
        }
    }
}

pub fn verify() {
    for count in [0, 1, 128, 4096] {
        let data = fixtures::blank_file(count);
        let summary = rssp::analyze(&data, "ssc", &rssp::AnalysisOptions::default())
            .expect("valid blank timing fixture");
        let mut json = Vec::new();
        rssp::report::write_reports(&summary, rssp::report::OutputMode::JSON, &mut json)
            .expect("Vec write");
        println!(
            "blank-load {count} {}",
            String::from_utf8(json).expect("JSON is UTF-8")
        );
        println!(
            "blank-snapshot {count} {:?}",
            rssp::bpm::chart_bpm_snapshots(&data, "ssc")
        );
        for chart in rssp::compute_chart_durations(&data, "ssc", rssp::TimingOffsets::default())
            .expect("valid blank timing fixture")
        {
            println!(
                "blank-duration {count} {}",
                chart.duration_seconds.to_bits()
            );
        }
        for chart in
            rssp::nps::compute_chart_peak_nps(&data, "ssc").expect("valid blank timing fixture")
        {
            println!("blank-peak {count} {}", chart.peak_nps.to_bits());
        }
    }
    for count in [0, 1, 32, 4096] {
        for kind in ["clean", "first", "middle", "last"] {
            let raw = fixtures::map(count, kind);
            println!(
                "raw-map {count} {kind} {:?}",
                rssp::bpm::clean_timing_map_cow(&raw)
            );
            for (ext, local) in [("sm", false), ("ssc", false), ("ssc", true)] {
                let data = fixtures::simfile(&raw, "2=-0.5,6=0.125", ext, local);
                let source = if local { "local" } else { "global" };
                println!(
                    "snapshot-load {count} {kind} {ext} {source} {:?}",
                    rssp::bpm::chart_bpm_snapshots(&data, ext)
                );
                for chart in
                    rssp::compute_chart_durations(&data, ext, rssp::TimingOffsets::default())
                        .expect("valid timing fixture")
                {
                    println!(
                        "raw-duration {count} {kind} {ext} {source} {}",
                        chart.duration_seconds.to_bits()
                    );
                }
            }
        }
    }
}
