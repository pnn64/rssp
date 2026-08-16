use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const SONG_COUNT: usize = 64;
pub const SONG_ENTRY_COUNT: usize = 5;
pub const ROOT_ENTRY_COUNT: usize = 1 + 128 + SONG_COUNT;
pub const BANNER_HINT: &str = "missing*.png";
pub const BACKGROUND_HINT: &str = "background*.jpg";

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
        }
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
