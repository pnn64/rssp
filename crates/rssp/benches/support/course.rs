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
pub const SELECT_COUNT: usize = 64;
pub const SELECT_PARAMS: u64 = 12;
pub const BANNER_ENTRY_COUNT: usize = 258;
pub const RESOLVE_GROUP_COUNT: usize = 128;
pub const RESOLVE_FILE_COUNT: usize = 256;
pub const RESOLVE_ENTRY_COUNT: usize = RESOLVE_GROUP_COUNT + RESOLVE_FILE_COUNT;
pub const RESOLVE_SONG: &str = "Target Song";
pub const RESOLVE_TITLE: &str = "RSSP Hash Perf Fixture Benchmark";

pub fn select_input() -> Vec<u8> {
    let mut course = String::with_capacity(64 + SELECT_COUNT * 256);
    course.push_str("#COURSE:Selection Performance;\n");
    for index in 0..SELECT_COUNT {
        writeln!(
            &mut course,
            "#SONGSELECT:TITLE=Song{index:03},Alt {index:03}:GROUP=Group A,Group B:ARTIST=Artist:GENRE=Genre A,Genre B:DIFFICULTY=Medium,Hard:METER=8-12:BPMRANGE=100-200:DURATION=90-150:SORT=FewestPlays,4:GAINSECONDS=5:GAINLIVES=2:MODS=2x,noshowcourse,nodifficult;"
        )
        .expect("writing to a String should succeed");
    }
    course.into_bytes()
}

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
        course.push_str(concat!(
            "#COURSE:Performance Course;\n",
            "#REPEAT:Maybe YES after completion;\n",
            "#METER:Beginner:3:Easy:6:Medium:9:Hard:12:Challenge:15:Edit:18;\n",
        ));
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

pub struct ResolveFixture {
    root: PathBuf,
    songs_dir: PathBuf,
    expected: PathBuf,
    title_expected: PathBuf,
}

impl ResolveFixture {
    pub fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rssp-course-resolve-{}-{unique}",
            std::process::id()
        ));
        let songs_dir = root.join("Songs");
        std::fs::create_dir_all(&songs_dir).expect("benchmark root should be creatable");
        for index in 0..RESOLVE_GROUP_COUNT {
            std::fs::create_dir(songs_dir.join(format!("Group-{index:03}")))
                .expect("benchmark group should be creatable");
        }
        for index in 0..RESOLVE_FILE_COUNT {
            std::fs::write(songs_dir.join(format!("Loose-{index:03}.txt")), [])
                .expect("benchmark loose file should be writable");
        }
        let expected = songs_dir.join("Group-000").join(RESOLVE_SONG);
        std::fs::create_dir(&expected).expect("benchmark song should be creatable");
        let title_expected = songs_dir.join("Group-001").join("Alias Directory");
        std::fs::create_dir(&title_expected).expect("benchmark alias should be creatable");
        std::fs::write(title_expected.join("Alias.ssc"), SIMFILE)
            .expect("benchmark alias simfile should be writable");
        Self {
            root,
            songs_dir,
            expected,
            title_expected,
        }
    }

    pub fn songs_dir(&self) -> &Path {
        &self.songs_dir
    }

    pub fn assert_behavior(&self) {
        const CASES: [(Option<&str>, &str); 7] = [
            (None, RESOLVE_SONG),
            (None, " target song "),
            (Some("Group-000"), RESOLVE_SONG),
            (Some("group-000"), "target song"),
            (Some("Group-001"), RESOLVE_TITLE),
            (None, "Missing Song"),
            (None, " "),
        ];
        for (group, song) in CASES {
            let legacy = rssp::course::profile_resolve_song_dir(&self.songs_dir, group, song, true);
            let current =
                rssp::course::profile_resolve_song_dir(&self.songs_dir, group, song, false);
            assert_eq!(current, legacy, "song resolution must not change");
        }
        assert_eq!(
            rssp::course::profile_resolve_song_dir(&self.songs_dir, None, RESOLVE_SONG, false,)
                .as_deref(),
            Some(self.expected.as_path())
        );
        assert_eq!(
            rssp::course::profile_resolve_song_dir(
                &self.songs_dir,
                Some("Group-001"),
                RESOLVE_TITLE,
                false,
            )
            .as_deref(),
            Some(self.title_expected.as_path())
        );
    }
}

impl Drop for ResolveFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub struct BannerFixture {
    root: PathBuf,
    course_path: PathBuf,
}

impl BannerFixture {
    pub fn new() -> Self {
        const EXTS: [&str; 5] = ["PNG", "jpg", "JPEG", "bmp", "GIF"];
        let root =
            std::env::temp_dir().join(format!("rssp-course-banner-bench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("banner benchmark root should be creatable");
        let course_path = root.join("Performance Mix.crs");
        std::fs::write(&course_path, []).expect("benchmark course should be writable");
        for index in 0..128 {
            std::fs::write(
                root.join(format!(
                    "pERFORMANCE mIX-{index:03}.{}",
                    EXTS[index % EXTS.len()]
                )),
                [],
            )
            .expect("benchmark banner should be writable");
            std::fs::write(root.join(format!("Unrelated-{index:03}.txt")), [])
                .expect("unrelated benchmark file should be writable");
        }
        std::fs::create_dir(root.join("Performance Mix-directory.png"))
            .expect("benchmark directory should be creatable");
        Self { root, course_path }
    }

    pub fn course_path(&self) -> &Path {
        &self.course_path
    }

    pub fn expected_banner(&self) -> PathBuf {
        self.root.join("pERFORMANCE mIX-000.PNG")
    }
}

impl Drop for BannerFixture {
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
