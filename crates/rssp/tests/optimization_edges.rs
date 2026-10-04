#![expect(
    clippy::float_cmp,
    reason = "regressions must preserve exact numeric outputs"
)]

use rssp::bpm::{chart_bpm_snapshots, compute_tier_bpm};
use rssp::nps::compute_chart_peak_nps;

#[test]
fn peak_aux_overrides() {
    for version in ["0.6", "0.83"] {
        for tag in ["SPEEDS:0=2=1=0", "SCROLLS:0=0.5", "FAKES:0=4", "SPEEDS:"] {
            let data = format!(
                "#VERSION:{version};\n#BPMS:0=120;\n#SPEEDS:0=3=2=1;\n\
                 #SCROLLS:0=0.5;\n#FAKES:0=8;\n#NOTEDATA:;\n\
                 #STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:8;\n\
                 #{tag};\n#NOTES:\n1000\n0100\n0010\n0001\n;\n"
            );
            let charts = compute_chart_peak_nps(data.as_bytes(), "ssc").expect("valid SSC");
            assert_eq!(charts.len(), 1);
            assert_eq!(charts[0].step_type, "dance-single");
            assert_eq!(charts[0].difficulty, "Hard");
            // Modern local timing suppresses the song BPM, even for a blank tag.
            assert_eq!(
                charts[0].peak_nps,
                if version == "0.83" { 1.0 } else { 2.0 }
            );
        }
    }
}

#[test]
fn peak_time_segments() {
    for (tags, peak) in [
        ("#BPMS:0=120;", 2.0),
        ("#BPMS:0=120;\n#STOPS:2=2;", 1.0),
        ("#BPMS:0=120;\n#DELAYS:2=2;", 1.0),
        ("#BPMS:0=120;\n#WARPS:1=1;", 4.0 / 1.5),
    ] {
        let data = format!(
            "#VERSION:0.83;\n{tags}\n#SPEEDS:0=2=4=0;\n#SCROLLS:0=0;\n\
             #FAKES:0=4;\n#NOTEDATA:;\n#STEPSTYPE:pump-single;\n\
             #DIFFICULTY:Hard;\n#METER:8;\n#NOTES:\n10000\n01000\n00100\n00010\n;\n"
        );
        let charts = compute_chart_peak_nps(data.as_bytes(), "ssc").expect("valid SSC");
        assert_eq!(charts.len(), 1);
        assert_eq!(charts[0].peak_nps, peak);
    }
}

#[test]
fn snapshot_tag_fallbacks() {
    for (version, bpm_tag, hash, formatted, range) in [
        (
            "0.83",
            "0=240",
            "0.000=240.000",
            "0.000000=240.000000",
            (240.0, 240.0),
        ),
        (
            "0.83",
            " \u{1}0=240,, ",
            "0.000=240.000",
            "0.000000=240.000000",
            (240.0, 240.0),
        ),
        (
            "0.83",
            "\u{1} , ,",
            "0.000=120.000,4.000=180.000",
            "0.000000=120.000000,4.000000=180.000000",
            (120.0, 180.0),
        ),
        (
            "0.6",
            "0=240",
            "0.000=240.000",
            "0.000000=120.000000,4.000000=180.000000",
            (120.0, 180.0),
        ),
    ] {
        let data = format!(
            "#VERSION:{version};\n#BPMS: \u{1}0=120,,4=180, ;\n#NOTEDATA:;\n\
             #STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:10;\n\
             #BPMS:{bpm_tag};\n#DISPLAYBPM:150:300;\n#NOTES:\n1000\n;\n"
        );
        let charts = chart_bpm_snapshots(data.as_bytes(), "ssc").expect("valid SSC");
        assert_eq!(charts.len(), 1);
        let chart = &charts[0];
        assert_eq!(chart.step_type, "dance-single");
        assert_eq!(chart.difficulty, "Challenge");
        assert_eq!(chart.hash_bpms, hash);
        assert_eq!(chart.bpms_formatted, formatted);
        assert_eq!((chart.bpm_min, chart.bpm_max), range);
        assert_eq!(chart.display_bpm, "150 - 300");
        assert_eq!(
            (chart.display_bpm_min, chart.display_bpm_max),
            (150.0, 300.0)
        );
    }
}

#[test]
fn tier_run_boundary() {
    let map = [(0.0, 137.125)];
    for (densities, expected) in [
        (&[32, 128, 32][..], 137.125),
        (&[32, 128, 32, 32][..], 1097.0),
        (&[24, 31, 24, 24, 0, 32, 128, 32][..], 265.679_687_5),
        (&[19, 19, 19, 20, 20, 20, 20][..], 171.40625),
        (&[16, 16, 16, 16, 32, 128, 32, 32, 0][..], 1097.0),
    ] {
        assert_eq!(compute_tier_bpm(densities, &map, 4.0), expected);
    }
}

#[test]
fn fixed_tier_matches() {
    let densities: Vec<_> = (0..512)
        .map(|i| [0, 16, 19, 16, 19, 20, 23, 20, 23, 32, usize::MAX, 32, 256][i % 13])
        .collect();
    for bpm in [
        f64::from_bits(1),
        0.000_001,
        137.125,
        9999.999,
        10_000.0,
        -120.0,
        0.0,
    ] {
        let fixed = compute_tier_bpm(&densities, &[(0.0, bpm)], 4.0);
        let variable = compute_tier_bpm(&densities, &[(0.0, bpm), (4096.0, bpm)], 4.0);
        assert_eq!(fixed.to_bits(), variable.to_bits());
    }
    for bpm in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            compute_tier_bpm(&densities, &[(0.0, bpm)], 4.0).to_bits(),
            bpm.to_bits()
        );
    }
}

#[test]
fn density_preserves_counts() {
    for lanes in [4, 5, 8, 10] {
        let mut data = Vec::new();
        for (rows, spacing) in [(64, 16), (9, 2), (32, 1)] {
            for i in 0..rows {
                let start = data.len();
                data.resize(start + lanes, b'0');
                if i % spacing == 0 {
                    data[start] = b"124"[i % 3];
                    data[start + lanes - 1] = b'1'; // Jumps still count once.
                }
                data.extend_from_slice(b"\r\n");
            }
            data.extend_from_slice(b",\n");
        }
        data.extend_from_slice(b"// comment\n\t0000\n;\n1111111111\n");
        assert_eq!(rssp::stats::measure_densities(&data, lanes), [4, 5, 32, 0]);
    }
    for (data, expected) in [
        (&b""[..], &[0][..]),
        (&b",\n,\n;"[..], &[0, 0, 0][..]),
        (&b"  1111\n0000\n"[..], &[1][..]),
        (
            &b"M000\nF000\nL000\n3000\n2000\n4000\n1000\n;"[..],
            &[3][..],
        ),
    ] {
        assert_eq!(rssp::stats::measure_densities(data, 4), expected);
    }
    assert_eq!(rssp::stats::measure_densities(b"00001\n1000\n;", 99), [1]);
}

#[test]
fn duration_cache_keys() {
    let data = concat!(
        "#VERSION:0.83;\n#OFFSET:0.5;\n#BPMS:0=120;\n",
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Easy;\n#METER:4;\n",
        "#BPMS: \u{1}0=120, ;\n#STOPS: 2=0.5, ;\n#DELAYS: 1=0.25, ;\n#WARPS: 1=0.5, ;\n",
        "#NOTES:\n1000\n0100\n0010\n0001\n;\n",
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Medium;\n#METER:4;\n",
        "#BPMS: \u{1}0=120, ;\n#STOPS: 2=0.5, ;\n#DELAYS: 1=0.25, ;\n#WARPS: 1=0.5, ;\n",
        "#NOTES:\n1000\n0100\n0010\n0001\n;\n",
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:4;\n",
        "#OFFSET:1;\n#BPMS: \u{1}0=120, ;\n#STOPS: 2=0.5, ;\n#DELAYS: 1=0.25, ;\n#WARPS: 1=0.5, ;\n",
        "#NOTES:\n1000\n0100\n0010\n0001\n;\n",
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:4;\n",
        "#BPMS:0=240;\n#NOTES:\n1000\n0100\n0010\n0001\n;\n",
    );
    let charts =
        rssp::compute_chart_durations(data.as_bytes(), "ssc", rssp::TimingOffsets::default())
            .expect("valid SSC");
    assert_eq!(charts.len(), 4);
    for (chart, (difficulty, duration)) in charts.iter().zip([
        ("Easy", 1.5),
        ("Medium", 1.5),
        ("Hard", 1.0),
        ("Challenge", 0.25),
    ]) {
        assert_eq!(chart.step_type, "dance-single");
        assert_eq!(chart.difficulty, difficulty);
        assert_eq!(chart.duration_seconds, duration);
    }
}

#[test]
fn snapshot_aux_maps() {
    for version in ["0.6", "0.83"] {
        for tag in [
            "SPEEDS: \u{1}0=2=1=0, ",
            "SCROLLS:0=-1",
            "FAKES:0=4",
            "DELAYS:0=4",
            "WARPS:0=4",
            "SPEEDS:",
        ] {
            let data = format!(
                "#VERSION:{version};\n#BPMS:0=120,4=180;\n#SPEEDS:0=2=1=0;\n\
                 #SCROLLS:0=-1;\n#FAKES:0=4;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n\
                 #DIFFICULTY:Hard;\n#METER:8;\n#{tag};\n#NOTES:\n1000\n;\n"
            );
            let charts = chart_bpm_snapshots(data.as_bytes(), "ssc").expect("valid SSC");
            assert_eq!(charts.len(), 1);
            assert_eq!(charts[0].hash_bpms, "0.000=120.000,4.000=180.000");
            assert_eq!(
                charts[0].bpms_formatted,
                "0.000000=120.000000,4.000000=180.000000"
            );
            assert_eq!((charts[0].bpm_min, charts[0].bpm_max), (120.0, 180.0));
            assert_eq!(charts[0].display_bpm, "120 - 180");
        }
    }
}
