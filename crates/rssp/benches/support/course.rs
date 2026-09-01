use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const SIMFILE: &[u8] = include_bytes!("../fixtures/hash_fixture.ssc");
pub const SONG_COUNT: usize = 64;
pub const REPEATED_SONGS: usize = 4;
pub const PARSE_TYPICAL_COUNT: usize = 10;
pub const PARSE_LARGE_COUNT: usize = 256;
pub const COURSE_HASH_COUNT: usize = SONG_COUNT;
pub const TYPICAL_HASH_UNIQUES: usize = 4;
pub const HASH_DEDUP_COUNT: usize = 4_096;
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
pub const STEP_NORM_BATCH: usize = 512;
pub const STEP_NORM_CASES: [&str; 8] = [
    "dance-single",
    " dance-single ",
    "pump-double",
    "lights-cabinet",
    "DANCE_SINGLE",
    "pump_Double",
    "DANCE-SOLO",
    "非ASCII-single",
];
pub const TITLE_MATCH_BATCH: usize = 1_024;
pub const TITLE_MATCH_INPUT: &[u8] =
    b"#TITLE:Performance Song;#SUBTITLE:Benchmark Mix;#ARTIST:RSSP;";
pub const TITLE_MATCH_EXPECTED: &str = "Performance Song Benchmark Mix";

pub fn parse_input(entry_count: usize) -> Vec<u8> {
    let mut course = String::with_capacity(256 + entry_count * 40);
    course.push_str(concat!(
        "#COURSE:Parse Performance;\n",
        "#COURSETRANSLIT:Parse Perf;\n",
        "#SCRIPTER:Benchmark;\n",
        "#DESCRIPTION:Entry reserve fixture;\n",
        "#BANNER:banner.png;\n",
        "#BACKGROUND:background.png;\n",
        "#REPEAT:YES;\n",
        "#LIVES:4;\n",
        "#METER:Beginner:3:Easy:6:Medium:9:Hard:12:Challenge:15:Edit:18;\n",
    ));
    for index in 0..entry_count {
        writeln!(
            &mut course,
            "#SONG:Group/Song{index:03}:Challenge:1.5x,mirror;"
        )
        .expect("writing to a String should succeed");
    }
    course.into_bytes()
}

pub fn assert_same_course(left: &rssp::course::CourseFile, right: &rssp::course::CourseFile) {
    assert_eq!(left.name, right.name);
    assert_eq!(left.name_translit, right.name_translit);
    assert_eq!(left.scripter, right.scripter);
    assert_eq!(left.description, right.description);
    assert_eq!(left.banner, right.banner);
    assert_eq!(left.background, right.background);
    assert_eq!(left.repeat, right.repeat);
    assert_eq!(left.lives, right.lives);
    assert_eq!(left.meters, right.meters);
    assert_eq!(left.entries, right.entries);
}

pub fn parse_reserved(data: &[u8], legacy: bool) -> rssp::course::CourseFile {
    rssp::course::profile_parse_crs_reserve(data, legacy).expect("course fixture should parse")
}

pub fn assert_parse_reserve_behavior() {
    for entry_count in [0, 1, PARSE_TYPICAL_COUNT, PARSE_LARGE_COUNT] {
        let input = parse_input(entry_count);
        let legacy = parse_reserved(&input, true);
        let current = parse_reserved(&input, false);
        assert_same_course(&current, &legacy);
        assert_eq!(current.entries.len(), entry_count);
    }
}

pub fn hash_values() -> Vec<String> {
    (0..HASH_DEDUP_COUNT)
        .map(|index| format!("{:016x}", index % 3_072))
        .collect()
}

pub fn course_hash_values() -> Vec<String> {
    (0..COURSE_HASH_COUNT)
        .map(|index| format!("{:016x}", index % 48))
        .collect()
}

pub fn typical_hash_values() -> Vec<String> {
    (0..COURSE_HASH_COUNT)
        .map(|index| format!("{:016x}", index % TYPICAL_HASH_UNIQUES))
        .collect()
}

pub fn assert_hash_dedup_behavior(values: &[String]) {
    let expected = rssp::course::profile_dedup_hashes(values, false);
    assert_eq!(expected, rssp::course::profile_dedup_hashes(values, true));
    assert_eq!(
        expected,
        rssp::course::profile_dedup_hashes_reserved(values)
    );
    assert_eq!(
        expected,
        rssp::course::profile_dedup_hashes_adaptive(values)
    );
    let edges = [
        "".to_string(),
        "0123456789abcdef".to_string(),
        "0123456789abcdef".to_string(),
        "short".to_string(),
        "short".to_string(),
        "é234567890abcdef".to_string(),
    ];
    assert_eq!(
        rssp::course::profile_dedup_hashes(&edges, false),
        rssp::course::profile_dedup_hashes(&edges, true)
    );
    assert_eq!(
        rssp::course::profile_dedup_hashes_reserved(&edges),
        rssp::course::profile_dedup_hashes(&edges, false)
    );
    assert_eq!(
        rssp::course::profile_dedup_hashes_adaptive(&edges),
        rssp::course::profile_dedup_hashes(&edges, false)
    );
}

pub fn assert_step_norm_behavior() {
    for raw in STEP_NORM_CASES {
        let legacy = rssp::course::profile_normalize_stepstype(raw, true);
        let current = rssp::course::profile_normalize_stepstype(raw, false);
        assert_eq!(
            current, legacy,
            "step type normalization changed for {raw:?}"
        );
    }
    assert!(matches!(
        rssp::course::profile_normalize_stepstype(" dance-single ", false),
        std::borrow::Cow::Borrowed("dance-single")
    ));
    assert!(matches!(
        rssp::course::profile_normalize_stepstype("DANCE_SINGLE", false),
        std::borrow::Cow::Owned(_)
    ));
}

pub fn assert_title_match_behavior() {
    const CASES: [(&[u8], &str, &str); 10] = [
        (b"#TITLE:Song;", "ssc", "song"),
        (b"#TITLE:Song;#SUBTITLE:Mix;", "sm", "SONG MIX"),
        (
            b"#TITLE:Native;#TITLETRANSLIT:Latin;#SUBTITLE:Sub;#SUBTITLETRANSLIT:Alt;",
            "ssc",
            "latin alt",
        ),
        (b"#TITLE:  Spaced  ;#SUBTITLE: Mix ;", "ssc", "Spaced Mix"),
        (b"#ARTIST:None;", "ssc", ""),
        (
            b"#TITLE:Colon\\:Song;#SUBTITLE:Mix;",
            "ssc",
            "Colon:Song Mix",
        ),
        (b"#TITLE:Caf\xe9;", "sm", "Café"),
        (b"#TITLE:Line\nBreak;", "ssc", "LineBreak"),
        (b"#TITLE:Song;", "ssc", "Other"),
        (b"#TITLE:Song;", "invalid", "Song"),
    ];
    for (data, ext, expected) in CASES {
        let legacy = rssp::course::profile_simfile_title_eq(data, ext, expected, true);
        let current = rssp::course::profile_simfile_title_eq(data, ext, expected, false);
        assert_eq!(current, legacy, "title matching changed for {expected:?}");
    }
    assert_eq!(
        rssp::course::profile_simfile_title_eq(
            TITLE_MATCH_INPUT,
            "ssc",
            TITLE_MATCH_EXPECTED,
            false,
        ),
        Some(true)
    );
}

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
        Self::with_unique(SONG_COUNT)
    }

    pub fn repeated() -> Self {
        Self::with_unique(REPEATED_SONGS)
    }

    fn with_unique(unique_songs: usize) -> Self {
        assert!(unique_songs > 0 && unique_songs <= SONG_COUNT);
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
        for index in 0..unique_songs {
            let song = format!("Song{index:03}");
            let song_dir = group_dir.join(&song);
            std::fs::create_dir(&song_dir).expect("song directory should be creatable");
            std::fs::write(song_dir.join(format!("{song}.ssc")), SIMFILE)
                .expect("benchmark simfile should be writable");
        }
        for index in 0..SONG_COUNT {
            let song = format!("Song{:03}", index % unique_songs);
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

    pub fn assert_song_cache(&self) {
        let analyze = |song_key_cache| {
            rssp::course::profile_analyze_crs(
                &self.course_path,
                Some(&self.songs_dir),
                "dance-single",
                "Medium",
                fast_options(),
                song_key_cache,
            )
            .expect("course cache fixture should analyze")
        };
        let path_cached = analyze(false);
        let song_cached = analyze(true);
        let (mut expected, mut actual) = (Vec::new(), Vec::new());
        rssp::report::write_course_reports(
            &path_cached,
            rssp::report::OutputMode::JSON,
            &mut expected,
        )
        .expect("path-key cache summary should serialize");
        rssp::report::write_course_reports(
            &song_cached,
            rssp::report::OutputMode::JSON,
            &mut actual,
        )
        .expect("song-key cache summary should serialize");
        assert_eq!(actual, expected);
    }

    pub fn assert_group_cache(&self) {
        let analyze = |group_cache| {
            rssp::course::profile_analyze_groups(
                &self.course_path,
                Some(&self.songs_dir),
                "dance-single",
                "Medium",
                fast_options(),
                group_cache,
            )
            .expect("course group cache fixture should analyze")
        };
        let uncached = analyze(false);
        let cached = analyze(true);
        let (mut expected, mut actual) = (Vec::new(), Vec::new());
        rssp::report::write_course_reports(
            &uncached,
            rssp::report::OutputMode::JSON,
            &mut expected,
        )
        .expect("uncached group summary should serialize");
        rssp::report::write_course_reports(&cached, rssp::report::OutputMode::JSON, &mut actual)
            .expect("cached group summary should serialize");
        assert_eq!(actual, expected);
    }

    pub fn assert_group_catalog(&self) {
        let analyze = |group_catalog| {
            rssp::course::profile_analyze_catalog(
                &self.course_path,
                Some(&self.songs_dir),
                "dance-single",
                "Medium",
                fast_options(),
                group_catalog,
            )
            .expect("course group catalog fixture should analyze")
        };
        let uncached = analyze(false);
        let cached = analyze(true);
        let (mut expected, mut actual) = (Vec::new(), Vec::new());
        rssp::report::write_course_reports(
            &uncached,
            rssp::report::OutputMode::JSON,
            &mut expected,
        )
        .expect("uncataloged group summary should serialize");
        rssp::report::write_course_reports(&cached, rssp::report::OutputMode::JSON, &mut actual)
            .expect("cataloged group summary should serialize");
        assert_eq!(actual, expected);
    }

    pub fn assert_catalog_dirs(&self) {
        let analyze = |trust_catalog| {
            rssp::course::profile_catalog_dirs(
                &self.course_path,
                Some(&self.songs_dir),
                "dance-single",
                "Medium",
                fast_options(),
                trust_catalog,
            )
            .expect("course catalog directory fixture should analyze")
        };
        let rechecked = analyze(false);
        let trusted = analyze(true);
        let (mut expected, mut actual) = (Vec::new(), Vec::new());
        rssp::report::write_course_reports(
            &rechecked,
            rssp::report::OutputMode::JSON,
            &mut expected,
        )
        .expect("rechecked catalog summary should serialize");
        rssp::report::write_course_reports(&trusted, rssp::report::OutputMode::JSON, &mut actual)
            .expect("trusted catalog summary should serialize");
        assert_eq!(actual, expected);
    }

    pub fn assert_nps_capacity(&self) {
        let analyze = |prealloc_nps| {
            rssp::course::profile_course_nps(
                &self.course_path,
                Some(&self.songs_dir),
                "dance-single",
                "Medium",
                fast_options(),
                prealloc_nps,
            )
            .expect("course NPS capacity fixture should analyze")
        };
        let growing = analyze(false);
        let preallocated = analyze(true);
        let (mut expected, mut actual) = (Vec::new(), Vec::new());
        rssp::report::write_course_reports(&growing, rssp::report::OutputMode::JSON, &mut expected)
            .expect("growing NPS summary should serialize");
        rssp::report::write_course_reports(
            &preallocated,
            rssp::report::OutputMode::JSON,
            &mut actual,
        )
        .expect("preallocated NPS summary should serialize");
        assert_eq!(actual, expected);
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

    pub fn assert_behavior(&self) {
        let assert_same = |tag: &str| {
            let legacy = rssp::course::profile_course_banner(&self.course_path, tag, true);
            let full_paths = rssp::course::profile_course_banner_full_paths(&self.course_path, tag);
            let current = rssp::course::profile_course_banner(&self.course_path, tag, false);
            assert_eq!(full_paths, legacy, "one-scan banner result changed");
            assert_eq!(current, full_paths, "filename banner result changed");
        };
        for tag in ["", " pERFORMANCE mIX-001.jpg ", "Missing.png"] {
            assert_same(tag);
        }
        let expected = self.expected_banner();
        assert_same(&expected.to_string_lossy());
        assert_eq!(
            rssp::course::profile_course_banner(&self.course_path, "", false),
            Some(expected)
        );
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
