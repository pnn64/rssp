use super::{black_box, measure};
use std::path::PathBuf;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rssp-course-bench-{}", std::process::id()));
        std::fs::create_dir(&root).expect("unique benchmark root");
        for (name, meter) in [("A", "8"), ("B", "9")] {
            let song = root.join("Songs").join("Group").join(name);
            std::fs::create_dir_all(&song).expect("fixture directory");
            std::fs::write(song.join("test.ssc"), format!("#VERSION:0.83;#TITLE:{name};#BPMS:0=120;#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:{meter};#NOTES:1000\n0100\n0010\n0001;")).expect("fixture simfile");
        }
        Self(root)
    }

    fn course(&self, count: usize, explicit: bool) -> PathBuf {
        let mut text = String::from("#COURSE:Meter benchmark;");
        if explicit {
            text.push_str("#METER:Medium:17;");
        }
        for index in 0..count {
            text.push_str(if index % 2 == 0 {
                "#SONG:Group/A:Hard:;"
            } else {
                "#SONG:Group/B:Hard:;"
            });
        }
        let path = self.0.join("test.crs");
        std::fs::write(&path, text).expect("fixture course");
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only this process's unique, freshly created fixture directory is removed.
        std::fs::remove_dir_all(&self.0).expect("remove benchmark fixture");
    }
}

fn analyze(fixture: &Fixture, path: &std::path::Path) -> rssp::CourseSummary {
    rssp::course::analyze_crs_path(
        path,
        Some(&fixture.0.join("Songs")),
        "dance-single",
        "Medium",
        rssp::AnalysisOptions {
            compute_tech_counts: false,
            compute_pattern_counts: false,
            ..Default::default()
        },
    )
    .expect("valid course fixture")
}

pub fn cases(iters: usize) {
    if std::env::var("RSSP_HOT_FILTER")
        .is_ok_and(|filter| !"course_load/".contains(&filter) && !filter.contains("course_load/"))
    {
        return;
    }
    let fixture = Fixture::new();
    for count in [1, 8, 32, 256] {
        for explicit in [false, true] {
            let path = fixture.course(count, explicit);
            // File creation is setup; the production load boundary intentionally includes I/O.
            measure(
                &format!("course_load/{count}_{explicit}"),
                count,
                iters,
                || {
                    black_box(analyze(&fixture, black_box(&path)));
                },
            );
        }
    }
}

pub fn verify() {
    let fixture = Fixture::new();
    for count in [1, 8, 32, 256] {
        for explicit in [false, true] {
            let path = fixture.course(count, explicit);
            let mut summary = analyze(&fixture, &path);
            summary.total_elapsed = std::time::Duration::ZERO;
            summary.chart.elapsed = std::time::Duration::ZERO;
            let mut output = Vec::new();
            rssp::report::write_course_reports(
                &summary,
                rssp::report::OutputMode::JSON,
                &mut output,
            )
            .expect("Vec write");
            println!(
                "course-load {count} {explicit} {}",
                String::from_utf8(output).expect("JSON is UTF-8")
            );
        }
    }
}
