use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const CHANGE_COUNT: usize = 256;
const ASSET_COUNT: usize = 32;
pub const LOOKUP_COUNT: usize = 256;
pub const IMAGE_COUNT: usize = 256;
pub const NON_IMAGE_COUNT: usize = 256;
pub const MOVIE_COUNT: usize = 128;
pub const SOUND_COUNT: usize = 129;
pub const DELIMITER_FIELD_COUNT: usize = 4_096;
pub const REL_PATH_COUNT: usize = 256;
pub const REL_COMPONENT_COUNT: usize = 4_096;
pub const BG_TAG_COUNT: usize = 256;

pub fn bgchange_tags() -> Vec<u8> {
    let mut input = String::with_capacity(BG_TAG_COUNT * 40);
    for index in 0..BG_TAG_COUNT {
        writeln!(
            &mut input,
            "#BGCHANGES:{}=Background-{index:03}.png;",
            index * 4
        )
        .expect("writing to a String cannot fail");
    }
    input.into_bytes()
}

pub fn assert_bgchange_values_behavior(input: &[u8]) {
    let previous = rssp::parse::extract_bgchanges_values(input);
    let streamed: Vec<_> = rssp::parse::bgchanges_values(input).collect();
    assert_eq!(streamed, previous);
    assert_eq!(streamed.len(), BG_TAG_COUNT);
}

pub fn relative_paths() -> Vec<String> {
    (0..REL_PATH_COUNT)
        .map(|index| format!("visuals/background,layer-{:02}.PNG", index % ASSET_COUNT))
        .collect()
}

pub fn relative_component_paths() -> Vec<String> {
    (0..REL_COMPONENT_COUNT)
        .map(|index| match index & 3 {
            0 => format!("Visuals/Layer-{index:04}.png"),
            1 => format!("./Visuals/../Visuals/Layer-{index:04}.png"),
            2 => format!("Group/Song/Visuals/Layer-{index:04}.png"),
            _ => format!(" Group / Song / Visuals / Layer-{index:04}.png "),
        })
        .collect()
}

pub fn assert_rel_component_behavior(paths: &[String]) {
    for path in paths.iter().map(String::as_str).chain([
        "../Visuals/file.png",
        "a/b/c/d/e/file.png",
        "Visuals\\file.png",
        "",
    ]) {
        assert!(
            rssp::profile::relative_asset_parts_match(path),
            "relative components changed for {path:?}"
        );
    }
}

pub fn delimiter_fields() -> Vec<String> {
    (0..DELIMITER_FIELD_COUNT)
        .map(|index| {
            if index.is_multiple_of(2) {
                format!("{index:08}=Visuals/Background-Layer-{index:08}.png,tail")
            } else {
                format!("{index:08},Visuals/Background-Layer-{index:08}.png=tail")
            }
        })
        .collect()
}

pub struct AssetFixture {
    root: PathBuf,
    image_dir: PathBuf,
    lookup_dir: PathBuf,
    relative_dir: PathBuf,
    simfile: Vec<u8>,
}

impl AssetFixture {
    pub fn new() -> Self {
        Self::with_movies(MOVIE_COUNT)
    }

    pub fn with_movies(movie_count: usize) -> Self {
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

        let relative_dir = root.join("Relative");
        let relative_visuals = relative_dir.join("Visuals");
        std::fs::create_dir_all(&relative_visuals)
            .expect("relative benchmark directory should be creatable");
        for index in 0..ASSET_COUNT {
            std::fs::write(
                relative_visuals.join(format!("Background,Layer-{index:02}.png")),
                [],
            )
            .expect("relative benchmark asset should be writable");
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
        for index in 0..movie_count {
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
            relative_dir,
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

    pub fn relative_dir(&self) -> &Path {
        &self.relative_dir
    }

    pub fn lookup_name() -> &'static str {
        "asset-255.DAT"
    }

    pub fn assert_song_assets_behavior(&self) {
        for (banner, background) in [
            ("", ""),
            ("Candidate-001.png", ""),
            ("missing.png", "Candidate-002.png"),
            ("candidate-003.PNG", "CANDIDATE-004.png"),
        ] {
            let legacy = rssp::profile::song_assets_legacy(&self.image_dir, banner, background);
            let current = rssp::assets::resolve_song_assets(&self.image_dir, banner, background);
            assert_eq!(current, legacy);
        }
    }

    pub fn assert_background_behavior(&self) {
        let previous = rssp::profile::background_changes_materialized(&self.root, &self.simfile);
        let current = rssp::assets::resolve_background_changes_like_itg(&self.root, &self.simfile);
        assert_eq!(current, previous);
    }

    pub fn assert_music_behavior(&self) {
        for tag in [
            "",
            "Track-127.ogg",
            "track-126.OGG",
            "missing.ogg",
            "  Intro.ogg  ",
        ] {
            let legacy = rssp::profile::music_path_legacy(&self.root, tag);
            let current = rssp::assets::resolve_music_path_like_itg(&self.root, tag);
            assert_eq!(current, legacy, "music fallback changed for tag {tag:?}");
        }
    }

    pub fn assert_rel_path_behavior(&self) {
        let deep = "a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/file.png";
        for rel in [
            "Visuals/Background,Layer-00.png",
            "visuals/background,layer-01.PNG",
            "Visuals\\Background,Layer-02.png",
            "./Visuals/./Background,Layer-03.png",
            "Visuals/Missing/../Background,Layer-04.png",
            "../Visuals/Background,Layer-05.png",
            "Visuals/missing.png",
            "",
            deep,
        ] {
            let legacy = rssp::profile::relative_asset_path(&self.relative_dir, rel, true);
            let current = rssp::profile::relative_asset_path(&self.relative_dir, rel, false);
            assert_eq!(current, legacy, "relative lookup changed for {rel:?}");
        }
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
