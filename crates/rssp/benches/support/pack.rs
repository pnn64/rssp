use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const SONG_COUNT: usize = 64;
pub const ROOT_ENTRY_COUNT: usize = 1 + 128 + SONG_COUNT;
pub const BANNER_HINT: &str = "missing*.png";
pub const BACKGROUND_HINT: &str = "background*.jpg";

pub struct PackFixture {
    root: PathBuf,
    pack_dir: PathBuf,
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

        Self { root, pack_dir }
    }

    pub fn pack_dir(&self) -> &Path {
        &self.pack_dir
    }
}

impl Drop for PackFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
