use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const CHANGE_COUNT: usize = 256;
const ASSET_COUNT: usize = 32;
pub const LOOKUP_COUNT: usize = 256;
pub const IMAGE_COUNT: usize = 256;
pub const NON_IMAGE_COUNT: usize = 256;
pub const MOVIE_COUNT: usize = 128;
pub const SOUND_COUNT: usize = 129;

pub struct AssetFixture {
    root: PathBuf,
    image_dir: PathBuf,
    lookup_dir: PathBuf,
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

        let lookup_dir = root.join("Lookup");
        std::fs::create_dir(&lookup_dir).expect("benchmark lookup directory should be creatable");
        for index in 0..LOOKUP_COUNT {
            std::fs::write(lookup_dir.join(format!("Asset-{index:03}.dat")), [])
                .expect("benchmark lookup file should be writable");
        }

        std::fs::write(root.join("Intro.ogg"), []).expect("benchmark intro should be writable");
        for index in 0..SOUND_COUNT - 1 {
            std::fs::write(root.join(format!("Track-{index:03}.ogg")), [])
                .expect("benchmark sound should be writable");
        }
        for index in 0..MOVIE_COUNT {
            std::fs::write(root.join(format!("Movie-{index:03}.mp4")), [])
                .expect("benchmark movie should be writable");
        }

        let image_dir = root.join("Images");
        std::fs::create_dir(&image_dir).expect("benchmark image directory should be creatable");
        for index in 0..IMAGE_COUNT {
            let (width, height) = if index == 1 { (300, 100) } else { (640, 480) };
            std::fs::write(
                image_dir.join(format!("Candidate-{index:03}.png")),
                png_header(width, height),
            )
            .expect("benchmark image should be writable");
        }
        for index in 0..NON_IMAGE_COUNT {
            std::fs::write(image_dir.join(format!("Other-{index:03}.dat")), [])
                .expect("benchmark non-image should be writable");
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
            image_dir,
            lookup_dir,
            simfile: simfile.into_bytes(),
        }
    }

    pub fn song_dir(&self) -> &Path {
        &self.root
    }

    pub fn simfile(&self) -> &[u8] {
        &self.simfile
    }

    pub fn lookup_dir(&self) -> &Path {
        &self.lookup_dir
    }

    pub fn image_dir(&self) -> &Path {
        &self.image_dir
    }

    pub fn lookup_name() -> &'static str {
        "asset-255.DAT"
    }
}

fn png_header(width: u32, height: u32) -> [u8; 24] {
    let mut header = [0u8; 24];
    header[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    header[12..16].copy_from_slice(b"IHDR");
    header[16..20].copy_from_slice(&width.to_be_bytes());
    header[20..24].copy_from_slice(&height.to_be_bytes());
    header
}

impl Drop for AssetFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
