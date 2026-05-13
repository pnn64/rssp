use rssp::{AnalysisOptions, analyze};

#[test]
fn analyze_parses_genre_into_summary() {
    let simfile = br#"
#TITLE:Genre Test;
#ARTIST:Test Artist;
#GENRE:  Drum\:Bass  ;
#BPMS:0.000=120.000;
#NOTES:
     dance-single:
     :
     Easy:
     1:
     0.000,0.000,0.000,0.000,0.000:
0000
0000
0000
0000
;
"#;

    let summary =
        analyze(simfile, "sm", &AnalysisOptions::default()).expect("simfile should parse");

    assert_eq!(summary.genre_str, "Drum:Bass");
}
