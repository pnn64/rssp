use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const SONG_COUNT: usize = 64;
pub const SONG_ENTRY_COUNT: usize = 5;
pub const LOOSE_ENTRY_COUNT: usize = 256;
pub const PARENT_IMG_COUNT: usize = 3;
pub const SONGS_ROOT_ENTRY_COUNT: usize = 1 + LOOSE_ENTRY_COUNT + PARENT_IMG_COUNT;
pub const ROOT_ENTRY_COUNT: usize = 1 + 128 + SONG_COUNT;
pub const TREE_ENTRY_COUNT: usize =
    SONGS_ROOT_ENTRY_COUNT + ROOT_ENTRY_COUNT + SONG_COUNT * SONG_ENTRY_COUNT;
pub const BANNER_HINT: &str = "missing*.png";
pub const BACKGROUND_HINT: &str = "background*.jpg";
pub const HINT_IMAGE_COUNT: usize = 256;
pub const HINT_OTHER_COUNT: usize = 256;
pub const HINT_ENTRY_COUNT: usize = HINT_IMAGE_COUNT + HINT_OTHER_COUNT;
pub const SUBDIR_HINT: &str = "Images/banner*.png";
pub const HINT_NORM_BATCH: usize = 4_096;
pub const HINT_NORM_INPUT: &str = "  Images/banner*.png  ";

pub fn assert_hint_norm_behavior() {
    for hint in [
        "",
        "   ",
        "banner*.png",
        " Images/banner*.png ",
        r"Images\banner*.png",
        r"Images\\nested\banner*.png",
        "å›¾åƒ/banner*.png",
    ] {
        let legacy = rssp::pack::profile_normalized_img_hint(hint, true);
        let borrowed = rssp::pack::profile_normalized_img_hint(hint, false);
        assert_eq!(borrowed.as_deref(), legacy.as_deref());
    }
    assert!(matches!(
        rssp::pack::profile_normalized_img_hint(HINT_NORM_INPUT, false),
        Some(std::borrow::Cow::Borrowed("Images/banner*.png"))
    ));
}

pub struct PackFixture {
    root: PathBuf,
    pack_dir: PathBuf,
    song_dir: PathBuf,
}

impl PackFixture {
    pub fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("rssp-pack-bench-{}-{unique}", std::process::id()));
        let pack_dir = root.join("Performance Pack");
        std::fs::create_dir_all(&pack_dir).expect("benchmark pack should be creatable");
        for index in 0..LOOSE_ENTRY_COUNT {
            std::fs::write(root.join(format!("Loose-{index:03}.dat")), [])
                .expect("benchmark loose root file should be writable");
        }
        for name in [
            "Performance Pack.gif",
            "PERFORMANCE PACK.jpeg",
            "performance pack.jpg",
        ] {
            std::fs::write(root.join(name), [])
                .expect("benchmark parent pack image should be writable");
        }

        let mut pack_ini = String::new();
        writeln!(
            &mut pack_ini,
            "[Group]\nVersion=1\nBanner={BANNER_HINT}\nBackground={BACKGROUND_HINT}"
        )
        .expect("writing to a String should succeed");
        std::fs::write(pack_dir.join("Pack.ini"), pack_ini)
            .expect("benchmark Pack.ini should be writable");

        for index in 0..64 {
            std::fs::write(pack_dir.join(format!("image{index:03}.png")), [])
                .expect("benchmark image should be writable");
            std::fs::write(pack_dir.join(format!("background{index:03}.jpg")), [])
                .expect("benchmark background should be writable");
        }

        for index in 0..SONG_COUNT {
            let song_dir = pack_dir.join(format!("Song{index:03}"));
            std::fs::create_dir(&song_dir).expect("benchmark song directory should be creatable");
            for name in [
                "chart.ssc",
                "Backup.SSC",
                "legacy.sm",
                "Older.SM",
                "audio.ogg",
            ] {
                std::fs::write(song_dir.join(name), [])
                    .expect("benchmark song asset should be writable");
            }
        }

        let song_dir = pack_dir.join("Song000");
        Self {
            root,
            pack_dir,
            song_dir,
        }
    }

    pub fn pack_dir(&self) -> &Path {
        &self.pack_dir
    }

    pub fn tree_root(&self) -> &Path {
        &self.root
    }

    pub fn song_dir(&self) -> &Path {
        &self.song_dir
    }

    pub fn assert_song_behavior(&self) {
        for opt in [
            rssp::pack::ScanOpt::default(),
            rssp::pack::ScanOpt {
                dup: rssp::pack::DupPolicy::Error,
            },
        ] {
            let old = rssp::profile::scan_song_dir_full_paths(&self.song_dir, opt);
            let new = rssp::pack::scan_song_dir(&self.song_dir, opt);
            assert_song_result(new, old);
            let previous = rssp::profile::scan_song_dir_joined_paths(&self.song_dir, opt);
            let new = rssp::pack::scan_song_dir(&self.song_dir, opt);
            assert_song_result(new, previous);
        }
    }

    pub fn assert_tree_behavior(&self) {
        for opt in [
            rssp::pack::ScanOpt::default(),
            rssp::pack::ScanOpt {
                dup: rssp::pack::DupPolicy::Error,
            },
        ] {
            let old = rssp::profile::find_simfiles_legacy(&self.root, opt);
            let new = rssp::pack::find_simfiles(&self.root, opt);
            assert_eq!(new, old, "recursive simfile discovery changed");
        }
    }

    pub fn assert_songs_behavior(&self) {
        let old = rssp::profile::scan_songs_dir_legacy(&self.root, rssp::pack::ScanOpt::default())
            .expect("legacy Songs root should scan");
        let new = rssp::pack::scan_songs_dir(&self.root, rssp::pack::ScanOpt::default())
            .expect("filtered Songs root should scan");
        assert_eq!(new.len(), old.len(), "pack count changed");
        for (new, old) in new.iter().zip(&old) {
            assert_pack_scan(new, old);
        }
    }

    pub fn assert_parent_img_behavior(&self) {
        for group_name in ["Performance Pack", "performance pack", "Missing Pack"] {
            let old = rssp::profile::pack_parent_img_legacy(&self.pack_dir, group_name);
            let new = rssp::profile::pack_parent_img(&self.pack_dir, group_name);
            assert_eq!(new, old, "parent pack image changed");
        }
        assert_eq!(
            rssp::profile::pack_parent_img(&self.pack_dir, "Performance Pack"),
            Some(self.root.join("performance pack.jpg")),
        );
    }

    pub fn assert_root_behavior(&self) {
        for (banner, background) in [
            (BANNER_HINT, BACKGROUND_HINT),
            ("image*.png", "missing*.jpg"),
            ("", ""),
        ] {
            let old = rssp::profile::pack_root_full_paths(
                &self.pack_dir,
                rssp::pack::ScanOpt::default(),
                banner,
                background,
            )
            .expect("full-path pack root should scan");
            let new = rssp::profile::pack_root(
                &self.pack_dir,
                rssp::pack::ScanOpt::default(),
                banner,
                background,
            )
            .expect("cached-type pack root should scan");
            assert_eq!(new.0, old.0, "pack banner changed");
            assert_eq!(new.1, old.1, "pack background changed");
            assert_eq!(new.2.len(), old.2.len(), "pack songs changed");
            for (new_song, old_song) in new.2.iter().zip(&old.2) {
                assert_eq!(new_song.dir, old_song.dir);
                assert_eq!(new_song.simfile, old_song.simfile);
                assert_eq!(new_song.extension, old_song.extension);
            }
        }
    }
}

fn assert_pack_scan(new: &rssp::pack::PackScan, old: &rssp::pack::PackScan) {
    assert_eq!(new.dir, old.dir);
    assert_eq!(new.group_name, old.group_name);
    assert_eq!(new.display_title, old.display_title);
    assert_eq!(new.sort_title, old.sort_title);
    assert_eq!(new.translit_title, old.translit_title);
    assert_eq!(new.series, old.series);
    assert_eq!(new.year, old.year);
    assert_eq!(new.version, old.version);
    assert_eq!(new.has_pack_ini, old.has_pack_ini);
    assert_eq!(new.sync_pref, old.sync_pref);
    assert_eq!(new.banner_path, old.banner_path);
    assert_eq!(new.background_path, old.background_path);
    assert_eq!(new.songs.len(), old.songs.len());
    for (new, old) in new.songs.iter().zip(&old.songs) {
        assert_eq!(new.dir, old.dir);
        assert_eq!(new.simfile, old.simfile);
        assert_eq!(new.extension, old.extension);
    }
}

fn assert_song_result(
    new: Result<Option<rssp::pack::SongScan>, rssp::pack::ScanError>,
    old: Result<Option<rssp::pack::SongScan>, rssp::pack::ScanError>,
) {
    match (new, old) {
        (Ok(Some(new)), Ok(Some(old))) => {
            assert_eq!(new.dir, old.dir);
            assert_eq!(new.simfile, old.simfile);
            assert_eq!(new.extension, old.extension);
        }
        (Ok(None), Ok(None)) => {}
        (
            Err(rssp::pack::ScanError::DuplicateSimfile {
                ext: new_ext,
                paths: new_paths,
            }),
            Err(rssp::pack::ScanError::DuplicateSimfile {
                ext: old_ext,
                paths: old_paths,
            }),
        ) => {
            assert_eq!(new_ext, old_ext);
            assert_eq!(new_paths, old_paths);
        }
        (new, old) => panic!("song scan changed: new={new:?} old={old:?}"),
    }
}

impl Drop for PackFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub struct ImageHintFixture {
    root: PathBuf,
    pack_dir: PathBuf,
}

impl ImageHintFixture {
    pub fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rssp-pack-image-bench-{}-{unique}",
            std::process::id()
        ));
        let pack_dir = root.join("Image Pack");
        let images = pack_dir.join("Images");
        std::fs::create_dir_all(&images).expect("benchmark image directory should be creatable");
        for index in 0..HINT_IMAGE_COUNT {
            std::fs::write(images.join(format!("Banner-{index:03}.PNG")), [])
                .expect("benchmark hint image should be writable");
        }
        for index in 0..HINT_OTHER_COUNT {
            std::fs::write(images.join(format!("Other-{index:03}.dat")), [])
                .expect("benchmark non-image should be writable");
        }
        Self { root, pack_dir }
    }

    pub fn pack_dir(&self) -> &Path {
        &self.pack_dir
    }

    pub fn assert_behavior(&self) {
        for hint in [SUBDIR_HINT, "images/BANNER-255.png", "Images/missing*.png"] {
            let old = rssp::profile::pack_subdir_img_legacy(&self.pack_dir, hint);
            let new = rssp::profile::pack_subdir_img(&self.pack_dir, hint);
            assert_eq!(new, old, "subdirectory pack image changed");
        }
        assert_eq!(
            rssp::profile::pack_subdir_img(&self.pack_dir, SUBDIR_HINT),
            Some(self.pack_dir.join("Images").join("Banner-000.PNG")),
        );
    }
}

impl Drop for ImageHintFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
