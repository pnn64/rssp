use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(crate) fn lc_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) fn img_rank(ext: &str) -> Option<u8> {
    if ext.eq_ignore_ascii_case("png") {
        Some(0)
    } else if ext.eq_ignore_ascii_case("jpg") {
        Some(1)
    } else if ext.eq_ignore_ascii_case("jpeg") {
        Some(2)
    } else if ext.eq_ignore_ascii_case("gif") {
        Some(3)
    } else if ext.eq_ignore_ascii_case("bmp") {
        Some(4)
    } else {
        None
    }
}

pub(crate) fn to_slash(s: &str) -> String {
    s.chars().map(|c| if c == '\\' { '/' } else { c }).collect()
}

pub(crate) fn is_dir_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_ascii_lowercase();
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let fname = entry.file_name();
        if fname.to_string_lossy().to_ascii_lowercase() == want && entry.path().is_dir() {
            return Some(entry.path());
        }
    }
    None
}

pub(crate) fn is_file_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_ascii_lowercase();
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let fname = entry.file_name();
        if fname.to_string_lossy().to_ascii_lowercase() == want && entry.path().is_file() {
            return Some(entry.path());
        }
    }
    None
}

pub(crate) fn match_mask_ci(name: &str, mask: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let mask = mask.to_ascii_lowercase();
    let Some(first) = mask.find('*') else {
        return name == mask;
    };
    let Some(second) = mask[first + 1..].find('*').map(|i| i + first + 1) else {
        let (a, b) = (&mask[..first], &mask[first + 1..]);
        return name.starts_with(a) && name.ends_with(b) && name.len() >= a.len() + b.len();
    };
    let a = &mask[..first];
    let b = &mask[first + 1..second];
    let c = &mask[second + 1..];
    if !name.starts_with(a) || !name.ends_with(c) || name.len() < a.len() + b.len() + c.len() {
        return false;
    }
    let mid = &name[a.len()..name.len() - c.len()];
    mid.contains(b)
}

pub(crate) fn list_img_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|e| img_rank(e).is_some())
        })
        .collect()
}

fn resolve_rel_ci(base: &Path, rel: &str) -> Option<PathBuf> {
    let rel = to_slash(rel);
    let mut parts: Vec<&str> = Vec::new();
    for part in rel.split('/') {
        let part = part.trim();
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.pop().is_none() {
                return None;
            }
            continue;
        }
        parts.push(part);
    }
    let (file, dirs) = parts.split_last()?;
    let mut dir = base.to_path_buf();
    for seg in dirs {
        dir = is_dir_ci(&dir, seg).or_else(|| {
            let p = dir.join(seg);
            p.is_dir().then_some(p)
        })?;
    }
    is_file_ci(&dir, file).or_else(|| {
        let p = dir.join(file);
        p.is_file().then_some(p)
    })
}

fn resolve_asset(song_dir: &Path, tag: &str) -> Option<PathBuf> {
    let tag = tag.trim();
    if tag.is_empty() {
        return None;
    }
    let direct = song_dir.join(tag);
    if direct.is_file() {
        return Some(direct);
    }
    if !tag.contains(['/', '\\']) {
        return is_file_ci(song_dir, tag);
    }
    resolve_rel_ci(song_dir, tag)
}

fn file_stem_lc(path: &Path) -> Option<String> {
    Some(path.file_stem()?.to_string_lossy().to_ascii_lowercase())
}

fn find_hint(
    files: &[PathBuf],
    starts_with: &[&str],
    contains: &[&str],
    ends_with: &[&str],
) -> Option<PathBuf> {
    for path in files {
        let Some(stem) = file_stem_lc(path) else {
            continue;
        };
        if starts_with.iter().any(|s| stem.starts_with(s)) {
            return Some(path.clone());
        }
        if ends_with.iter().any(|s| stem.ends_with(s)) {
            return Some(path.clone());
        }
        if contains.iter().any(|s| stem.contains(s)) {
            return Some(path.clone());
        }
    }
    None
}

fn png_dims(mut f: fs::File) -> Option<(u32, u32)> {
    let mut header = [0u8; 24];
    f.read_exact(&mut header).ok()?;
    if &header[0..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(header[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(header[20..24].try_into().ok()?);
    Some((w, h))
}

fn gif_dims(mut f: fs::File) -> Option<(u32, u32)> {
    let mut header = [0u8; 10];
    f.read_exact(&mut header).ok()?;
    if &header[0..3] != b"GIF" {
        return None;
    }
    let w = u16::from_le_bytes(header[6..8].try_into().ok()?) as u32;
    let h = u16::from_le_bytes(header[8..10].try_into().ok()?) as u32;
    Some((w, h))
}

fn bmp_dims(mut f: fs::File) -> Option<(u32, u32)> {
    let mut header = [0u8; 26];
    f.read_exact(&mut header).ok()?;
    if &header[0..2] != b"BM" {
        return None;
    }
    let w = i32::from_le_bytes(header[18..22].try_into().ok()?);
    let h = i32::from_le_bytes(header[22..26].try_into().ok()?);
    Some((w.unsigned_abs(), h.unsigned_abs()))
}

fn jpg_sof(marker: u8) -> bool {
    matches!(
        marker,
        0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF
    )
}

fn jpg_dims(mut f: fs::File) -> Option<(u32, u32)> {
    let mut buf = [0u8; 2];
    f.read_exact(&mut buf).ok()?;
    if buf != [0xFF, 0xD8] {
        return None;
    }
    loop {
        let mut b = [0u8; 1];
        f.read_exact(&mut b).ok()?;
        if b[0] != 0xFF {
            continue;
        }
        f.read_exact(&mut b).ok()?;
        while b[0] == 0xFF {
            f.read_exact(&mut b).ok()?;
        }
        let marker = b[0];
        if marker == 0xD9 || marker == 0xDA {
            return None;
        }
        if (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        let mut len_bytes = [0u8; 2];
        f.read_exact(&mut len_bytes).ok()?;
        let len = u16::from_be_bytes(len_bytes) as usize;
        if len < 2 {
            return None;
        }
        if jpg_sof(marker) {
            let mut sof = [0u8; 5];
            f.read_exact(&mut sof).ok()?;
            let h = u16::from_be_bytes(sof[1..3].try_into().ok()?) as u32;
            let w = u16::from_be_bytes(sof[3..5].try_into().ok()?) as u32;
            return Some((w, h));
        }
        io::copy(&mut f.by_ref().take((len - 2) as u64), &mut io::sink()).ok()?;
    }
}

fn img_dims(path: &Path) -> Option<(u32, u32)> {
    let ext = path.extension()?.to_str()?;
    let f = fs::File::open(path).ok()?;

    if ext.eq_ignore_ascii_case("png") {
        png_dims(f)
    } else if ext.eq_ignore_ascii_case("gif") {
        gif_dims(f)
    } else if ext.eq_ignore_ascii_case("bmp") {
        bmp_dims(f)
    } else if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") {
        jpg_dims(f)
    } else {
        None
    }
}

pub fn resolve_song_assets(
    song_dir: &Path,
    banner_tag: &str,
    background_tag: &str,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let mut banner = resolve_asset(song_dir, banner_tag);
    let mut background = resolve_asset(song_dir, background_tag);

    if banner.is_some() && background.is_some() {
        return (banner, background);
    }

    let mut imgs = list_img_files(song_dir);
    imgs.sort_by_cached_key(|p| lc_name(p));

    if banner.is_none() {
        banner = find_hint(&imgs, &[], &["banner"], &["bn"]);
    }
    if background.is_none() {
        background = find_hint(&imgs, &[], &["background"], &["bg"]);
    }

    if banner.is_some() && background.is_some() {
        return (banner, background);
    }

    for img in &imgs {
        if background.as_ref().is_some_and(|p| p == img) {
            continue;
        }
        if banner.as_ref().is_some_and(|p| p == img) {
            continue;
        }
        let Some((w, h)) = img_dims(img) else {
            continue;
        };
        if background.is_none() && w >= 320 && h >= 240 {
            background = Some(img.clone());
            continue;
        }
        if banner.is_none() && (100..=320).contains(&w) && (50..=240).contains(&h) {
            banner = Some(img.clone());
            continue;
        }
        if banner.is_none() && w > 200 && h > 0 && (w as f32 / h as f32) > 2.0 {
            banner = Some(img.clone());
        }
    }

    (banner, background)
}
