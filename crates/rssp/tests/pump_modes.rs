use rssp::{AnalysisOptions, analyze};

const PUMP_SIMFILE: &[u8] = br"
#TITLE:Pump Modes;
#ARTIST:RSSP;
#BPMS:0.000=120.000;
#NOTES:
     pump-single:
     :
     Easy:
     3:
     0.000,0.000,0.000,0.000,0.000:
10001
00100
00000
00000
;
#NOTES:
     pump-double:
     :
     Hard:
     8:
     0.000,0.000,0.000,0.000,0.000:
1000000001
0000100000
0000000000
0000000000
;
";

#[test]
fn analyze_keeps_pump_single_and_double_columns() {
    let summary = analyze(PUMP_SIMFILE, "sm", &AnalysisOptions::default())
        .expect("Pump simfile should parse");
    assert_eq!(summary.charts.len(), 2);

    let single = &summary.charts[0];
    assert_eq!(single.step_type_str, "pump-single");
    assert_eq!(single.stats.total_arrows, 3);
    assert_eq!(single.stats.total_steps, 2);
    assert!(single.minimized_note_data.starts_with(b"10001\n00100"));

    let double = &summary.charts[1];
    assert_eq!(double.step_type_str, "pump-double");
    assert_eq!(double.stats.total_arrows, 3);
    assert_eq!(double.stats.total_steps, 2);
    assert!(
        double
            .minimized_note_data
            .starts_with(b"1000000001\n0000100000")
    );
    assert_ne!(single.short_hash, double.short_hash);
}
