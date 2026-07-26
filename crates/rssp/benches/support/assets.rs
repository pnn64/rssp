use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const CHANGE_COUNT: usize = 256;
const ASSET_COUNT: usize = 32;

pub struct AssetFixture {
    root: PathBuf,
    simfile: Vec<u8>,
}

impl AssetFixture {
    pub fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("rssp-assets-bench-{}-{unique}", std::process::id()));
        let visuals = root.join("Visuals");
        std::fs::create_dir_all(&visuals).expect("benchmark asset directory should be creatable");

        for index in 0..ASSET_COUNT {
            std::fs::write(visuals.join(format!("Background,Layer-{index:02}.png")), [])
                .expect("benchmark background should be writable");
        }

        let mut simfile = String::with_capacity(CHANGE_COUNT * 48);
        simfile.push_str("#BGCHANGES:");
        for index in 0..CHANGE_COUNT {
            if index != 0 {
                simfile.push(',');
            }
            write!(
                &mut simfile,
                "{}=Visuals/Background,Layer-{:02}.png",
                index * 4,
                index % ASSET_COUNT
            )
            .expect("writing to a String should succeed");
        }
        simfile.push_str(";\n");

        Self {
            root,
            simfile: simfile.into_bytes(),
        }
    }

    pub fn song_dir(&self) -> &Path {
        &self.root
    }

    pub fn simfile(&self) -> &[u8] {
        &self.simfile
    }
}

impl Drop for AssetFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
