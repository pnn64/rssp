use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const SIMFILE: &[u8] = include_bytes!("../fixtures/hash_fixture.ssc");
pub const SONG_COUNT: usize = 64;
pub const MOD_COUNT: u64 = 9;
pub const MODS: &str =
    " 1.5x, reverse, mirror, noholds, nomines, sudden, showcourse, nodifficult, award2 ";
pub const SELECT_MOD_COUNT: u64 = 8;
pub const SELECT_MODS: &[u8] =
    b" 1.5x, reverse, mirror, noholds, nomines, sudden, noshowcourse, nodifficult ";

pub struct CourseFixture {
    root: PathBuf,
    course_path: PathBuf,
    songs_dir: PathBuf,
}

impl CourseFixture {
    pub fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("rssp-course-bench-{}-{unique}", std::process::id()));
        let songs_dir = root.join("Songs");
        let group_dir = songs_dir.join("Group");
        std::fs::create_dir_all(&group_dir).expect("benchmark root should be creatable");

        let mut course = String::with_capacity(64 + SONG_COUNT * 48);
        course.push_str("#COURSE:Performance Course;\n");
        for index in 0..SONG_COUNT {
            let song = format!("Song{index:03}");
            let song_dir = group_dir.join(&song);
            std::fs::create_dir(&song_dir).expect("song directory should be creatable");
            std::fs::write(song_dir.join(format!("{song}.ssc")), SIMFILE)
                .expect("benchmark simfile should be writable");
            writeln!(&mut course, "#SONG:Group/{song}:Challenge:;")
                .expect("writing to a String should succeed");
        }

        let course_path = root.join("performance.crs");
        std::fs::write(&course_path, course).expect("benchmark course should be writable");
        Self {
            root,
            course_path,
            songs_dir,
        }
    }

    pub fn course_path(&self) -> &Path {
        &self.course_path
    }

    pub fn songs_dir(&self) -> &Path {
        &self.songs_dir
    }
}

impl Drop for CourseFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[allow(dead_code)]
pub fn fast_options() -> rssp::AnalysisOptions {
    rssp::AnalysisOptions {
        compute_pattern_counts: false,
        compute_tech_counts: false,
        ..rssp::AnalysisOptions::default()
    }
}

pub fn clone_heavy_options() -> rssp::AnalysisOptions {
    const DIRECTIONS: [u8; 4] = *b"LDUR";
    let custom_patterns = (0..256)
        .map(|mut value| {
            let mut bytes = [b'L'; 8];
            for byte in &mut bytes {
                *byte = DIRECTIONS[value & 3];
                value >>= 2;
            }
            String::from_utf8(bytes.to_vec()).expect("directions are valid UTF-8")
        })
        .collect();
    rssp::AnalysisOptions {
        custom_patterns,
        compute_pattern_counts: false,
        compute_tech_counts: false,
        ..rssp::AnalysisOptions::default()
    }
}
