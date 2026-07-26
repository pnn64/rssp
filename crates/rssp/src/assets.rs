use std::borrow::Cow;
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::parse::{decode_bytes, extract_bgchanges_values, unescape_tag};

const RANDOM_BACKGROUND_FILE: &str = "-random-";
const NO_SONG_BG_FILE: &str = "-nosongbg-";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackgroundChangeTarget {
    File(PathBuf),
    NoSongBg,
    Random,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedBackgroundChange {
    pub start_beat: f32,
    pub target: BackgroundChangeTarget,
}

pub(crate) fn lc_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) fn is_mac_resource_fork(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("._"))
}

pub(crate) fn name_eq_ci(actual: &OsStr, expected: &str) -> bool {
    actual.to_string_lossy().eq_ignore_ascii_case(expected)
}

pub(crate) const fn img_rank(ext: &str) -> Option<u8> {
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
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let fname = entry.file_name();
        if !fname.to_string_lossy().starts_with("._") && name_eq_ci(&fname, name) {
            let path = entry.path();
            if path.is_dir() {
                return Some(path);
            }
        }
    }
    None
}

pub(crate) fn is_file_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let fname = entry.file_name();
        if !fname.to_string_lossy().starts_with("._") && name_eq_ci(&fname, name) {
            let path = entry.path();
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

pub(crate) fn match_mask_ci(name: &str, mask: &str) -> bool {
    let Some(first) = mask.find('*') else {
        return name.eq_ignore_ascii_case(mask);
    };
    let Some(second) = mask[first + 1..].find('*').map(|i| i + first + 1) else {
        let (a, b) = (&mask[..first], &mask[first + 1..]);
        return name.len() >= a.len() + b.len()
            && name
                .as_bytes()
                .get(..a.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(a.as_bytes()))
            && name
                .as_bytes()
                .get(name.len() - b.len()..)
                .is_some_and(|tail| tail.eq_ignore_ascii_case(b.as_bytes()));
    };
    let a = &mask[..first];
    let b = &mask[first + 1..second];
    let c = &mask[second + 1..];
    if name.len() < a.len() + b.len() + c.len()
        || !name
            .as_bytes()
            .get(..a.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(a.as_bytes()))
        || !name
            .as_bytes()
            .get(name.len() - c.len()..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(c.as_bytes()))
    {
        return false;
    }
    let mid = &name.as_bytes()[a.len()..name.len() - c.len()];
    b.is_empty()
        || mid
            .windows(b.len())
            .any(|window| window.eq_ignore_ascii_case(b.as_bytes()))
}

fn list_image_candidates(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(extension) = Path::new(&name)
            .extension()
            .and_then(|extension| extension.to_str())
        else {
            continue;
        };
        if name.to_string_lossy().starts_with("._") || img_rank(extension).is_none() {
            continue;
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        candidates.push(path);
    }
    candidates.sort_by(|left, right| cmp_name_ci(left, right));
    candidates
}

fn image_hint_matches(path: &Path, contains: &[u8], suffix: &[u8]) -> bool {
    let Some(stem) = path.file_stem() else {
        return false;
    };
    let stem = stem.to_string_lossy();
    let stem = stem.as_bytes();
    stem.get(stem.len().saturating_sub(suffix.len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
        || stem
            .windows(contains.len())
            .any(|window| window.eq_ignore_ascii_case(contains))
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
            parts.pop()?;
            continue;
        }
        parts.push(part);
    }
    let (file, dirs) = parts.split_last()?;
    let mut dir = base.to_path_buf();
    for seg in dirs {
        dir = is_dir_ci(&dir, seg).or_else(|| {
            let p = dir.join(seg);
            (!is_mac_resource_fork(&p) && p.is_dir()).then_some(p)
        })?;
    }
    is_file_ci(&dir, file).or_else(|| {
        let p = dir.join(file);
        (!is_mac_resource_fork(&p) && p.is_file()).then_some(p)
    })
}

fn resolve_asset(song_dir: &Path, tag: &str) -> Option<PathBuf> {
    let tag = tag.trim();
    if tag.is_empty() {
        return None;
    }
    let direct = song_dir.join(tag);
    if !is_mac_resource_fork(&direct) && direct.is_file() {
        return Some(direct);
    }
    if !tag.contains(['/', '\\']) {
        return is_file_ci(song_dir, tag);
    }
    resolve_rel_ci(song_dir, tag)
}

const SOUND_EXTS: [&str; 1] = ["ogg"];
const MOVIE_EXTS: [&str; 11] = [
    "avi", "f4v", "flv", "mkv", "mp4", "mpeg", "mpg", "mov", "ogv", "webm", "wmv",
];

#[inline(always)]
fn is_sound_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| SOUND_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

#[inline(always)]
fn is_movie_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| MOVIE_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

pub(crate) fn cmp_name_ci(left: &Path, right: &Path) -> Ordering {
    let left = left
        .file_name()
        .map_or_else(Cow::default, |name| name.to_string_lossy());
    let right = right
        .file_name()
        .map_or_else(Cow::default, |name| name.to_string_lossy());
    let left = left.as_bytes();
    let right = right.as_bytes();
    for index in 0..left.len().min(right.len()) {
        let ordering = left[index]
            .to_ascii_lowercase()
            .cmp(&right[index].to_ascii_lowercase());
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

#[inline(always)]
fn first_two_sound_files(song_dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let Ok(entries) = fs::read_dir(song_dir) else {
        return (None, None);
    };
    let mut first: Option<PathBuf> = None;
    let mut second: Option<PathBuf> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if is_mac_resource_fork(&path) || !path.is_file() || !is_sound_ext(&path) {
            continue;
        }
        if first
            .as_ref()
            .is_none_or(|candidate| cmp_name_ci(&path, candidate) == Ordering::Less)
        {
            second = first.replace(path);
        } else if second
            .as_ref()
            .is_none_or(|candidate| cmp_name_ci(&path, candidate) == Ordering::Less)
        {
            second = Some(path);
        }
    }
    (first, second)
}

#[inline(always)]
fn only_movie_file(song_dir: &Path) -> Option<PathBuf> {
    let Ok(entries) = fs::read_dir(song_dir) else {
        return None;
    };
    let mut movie = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if is_mac_resource_fork(&path) || !path.is_file() || !is_movie_ext(&path) {
            continue;
        }
        if movie.is_some() {
            return None;
        }
        movie = Some(path);
    }
    movie
}

/// Resolves `#MUSIC` like ITGmania's Song::TidyUpData fallback behavior.
///
/// Order:
/// 1. Try the tagged path in `#MUSIC` (case-insensitive within song dir).
/// 2. If missing, pick the first sound file in the song directory.
/// 3. If the first fallback starts with `intro` and another sound exists, use the second.
#[must_use]
pub fn resolve_music_path_like_itg(song_dir: &Path, music_tag: &str) -> Option<PathBuf> {
    let tag = music_tag.trim();
    if !tag.is_empty()
        && let Some(path) = resolve_asset(song_dir, tag)
    {
        return Some(path);
    }

    let (first, second) = first_two_sound_files(song_dir);
    let first = first?;
    if second.is_some()
        && first.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .as_bytes()
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"intro"))
        })
    {
        return second;
    }
    Some(first)
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
    let w = u32::from(u16::from_le_bytes(header[6..8].try_into().ok()?));
    let h = u32::from(u16::from_le_bytes(header[8..10].try_into().ok()?));
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

const fn jpg_sof(marker: u8) -> bool {
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
            let h = u32::from(u16::from_be_bytes(sof[1..3].try_into().ok()?));
            let w = u32::from(u16::from_be_bytes(sof[3..5].try_into().ok()?));
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

#[must_use]
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

    let images = list_image_candidates(song_dir);
    if banner.is_none() || background.is_none() {
        for image in &images {
            if banner.is_none() && image_hint_matches(image, b"banner", b"bn") {
                banner = Some(image.clone());
            }
            if background.is_none() && image_hint_matches(image, b"background", b"bg") {
                background = Some(image.clone());
            }
            if banner.is_some() && background.is_some() {
                break;
            }
        }
    }

    if banner.is_some() && background.is_some() {
        return (banner, background);
    }

    for image in &images {
        if banner.is_some() && background.is_some() {
            break;
        }
        if background.as_ref().is_some_and(|path| path == image) {
            continue;
        }
        if banner.as_ref().is_some_and(|path| path == image) {
            continue;
        }
        let Some((w, h)) = img_dims(image) else {
            continue;
        };
        if background.is_none() && w >= 320 && h >= 240 {
            background = Some(image.clone());
            continue;
        }
        if banner.is_none() && (100..=320).contains(&w) && (50..=240).contains(&h) {
            banner = Some(image.clone());
            continue;
        }
        if banner.is_none() && w > 200 && h > 0 && (w as f32 / h as f32) > 2.0 {
            banner = Some(image.clone());
        }
    }

    (banner, background)
}

fn list_song_dir_rel_files(song_dir: &Path) -> Vec<String> {
    let mut dirs = vec![song_dir.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let Ok(rel) = path.strip_prefix(song_dir) else {
                continue;
            };
            files.push(to_slash(&rel.to_string_lossy()));
        }
    }
    files.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    files
}

fn strip_newlines(s: &str) -> Cow<'_, str> {
    if !s.contains('\n') {
        return Cow::Borrowed(s);
    }

    let mut out = String::with_capacity(s.len());
    for line in s.lines() {
        out.push_str(line);
    }
    Cow::Owned(out)
}

fn match_bg_file<'a>(changes: &'a str, start: usize, files: &[String]) -> Option<&'a str> {
    for file in files {
        let Some(head) = changes.get(start..start + file.len()) else {
            continue;
        };
        if !head.eq_ignore_ascii_case(file) {
            continue;
        }
        let next = start + file.len();
        if matches!(changes.as_bytes().get(next), None | Some(b'=') | Some(b',')) {
            return Some(head);
        }
    }
    None
}

fn for_each_bgchange_pair(changes: &str, files: &[String], mut handle: impl FnMut(&str, &str)) {
    let changes = strip_newlines(changes);
    if changes.is_empty() {
        return;
    }

    let changes = changes.as_ref();
    let mut start = 0usize;
    let mut pnum = 0u8;
    let mut start_beat = None;
    let mut target = None;
    while start <= changes.len() {
        let (field, delimiter) = if (pnum == 1 || pnum == 7)
            && let Some(found) = match_bg_file(changes, start, files)
        {
            start += found.len();
            let delimiter = changes.as_bytes().get(start).copied();
            if delimiter.is_some() {
                start += 1;
            }
            (found, delimiter)
        } else {
            let rem = &changes[start..];
            let eq = rem.find('=').map(|index| start + index);
            let comma = rem.find(',').map(|index| start + index);
            let end = match (eq, comma) {
                (Some(eq), Some(comma)) => eq.min(comma),
                (Some(eq), None) => eq,
                (None, Some(comma)) => comma,
                (None, None) => changes.len(),
            };
            let field = &changes[start..end];
            let delimiter = changes.as_bytes().get(end).copied();
            start = end + usize::from(delimiter.is_some());
            (field, delimiter)
        };

        match pnum {
            0 => start_beat = Some(field),
            1 => target = Some(field),
            _ => {}
        }

        match delimiter {
            Some(b'=') => pnum += 1,
            Some(b',') => {
                if let (Some(start_beat), Some(target)) = (start_beat, target) {
                    handle(start_beat, target);
                }
                start_beat = None;
                target = None;
                pnum = 0;
            }
            None => {
                if let (Some(start_beat), Some(target)) = (start_beat, target) {
                    handle(start_beat, target);
                }
                break;
            }
            Some(_) => unreachable!("background change delimiter must be '=' or ','"),
        }
    }
}

fn resolve_bgchange_target(song_dir: &Path, file1: &str) -> Option<BackgroundChangeTarget> {
    let file1 = file1.trim();
    if file1.is_empty() {
        return None;
    }
    if file1.eq_ignore_ascii_case(NO_SONG_BG_FILE) {
        return Some(BackgroundChangeTarget::NoSongBg);
    }
    if file1.eq_ignore_ascii_case(RANDOM_BACKGROUND_FILE) {
        return Some(BackgroundChangeTarget::Random);
    }
    resolve_asset(song_dir, file1).map(BackgroundChangeTarget::File)
}

fn parse_bgchange_pair(
    song_dir: &Path,
    start_beat: &str,
    target: &str,
) -> Option<ResolvedBackgroundChange> {
    let start_beat = start_beat.trim().parse::<f32>().unwrap_or(0.0);
    let target = resolve_bgchange_target(song_dir, target)?;
    Some(ResolvedBackgroundChange { start_beat, target })
}

#[inline(always)]
fn upsert_bgchange(out: &mut Vec<ResolvedBackgroundChange>, change: ResolvedBackgroundChange) {
    if let Some(slot) = out
        .iter_mut()
        .find(|existing| existing.start_beat == change.start_beat)
    {
        *slot = change;
    } else {
        out.push(change);
    }
}

#[must_use]
pub fn resolve_background_changes_like_itg(
    song_dir: &Path,
    simfile_data: &[u8],
) -> Vec<ResolvedBackgroundChange> {
    let files = list_song_dir_rel_files(song_dir);
    let mut out: Vec<ResolvedBackgroundChange> = Vec::new();
    let mut saw_no_song_bg = false;
    for raw in extract_bgchanges_values(simfile_data) {
        let decoded = decode_bytes(raw);
        let text = unescape_tag(decoded.as_ref());
        for_each_bgchange_pair(text.as_ref(), &files, |start_beat, target| {
            let Some(change) = parse_bgchange_pair(song_dir, start_beat, target) else {
                return;
            };
            if matches!(change.target, BackgroundChangeTarget::NoSongBg) {
                saw_no_song_bg = true;
                return;
            }
            upsert_bgchange(&mut out, change);
        });
    }
    let has_explicit_movie = out.iter().any(|change| {
        matches!(
            change.target,
            BackgroundChangeTarget::File(ref path) if is_movie_ext(path)
        )
    });
    let beat_zero_still_ix = out
        .iter()
        .enumerate()
        .filter(|(_, change)| {
            change.start_beat <= 0.0
                && matches!(
                    change.target,
                    BackgroundChangeTarget::File(ref path) if !is_movie_ext(path)
                )
        })
        .map(|(ix, _)| ix)
        .last();
    let blocks_beat_zero = out.iter().any(|change| {
        change.start_beat <= 0.0 && !matches!(change.target, BackgroundChangeTarget::File(_))
    });
    let has_any_file = out
        .iter()
        .any(|change| matches!(change.target, BackgroundChangeTarget::File(_)));
    if !has_explicit_movie && let Some(movie) = only_movie_file(song_dir) {
        if saw_no_song_bg {
            if let Some(ix) = beat_zero_still_ix {
                out[ix].target = BackgroundChangeTarget::File(movie);
            } else if !blocks_beat_zero {
                out.push(ResolvedBackgroundChange {
                    start_beat: 0.0,
                    target: BackgroundChangeTarget::File(movie),
                });
            }
        } else if !has_any_file && !blocks_beat_zero {
            out.push(ResolvedBackgroundChange {
                start_beat: 0.0,
                target: BackgroundChangeTarget::File(movie),
            });
        }
    }
    out.sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        BackgroundChangeTarget, ResolvedBackgroundChange, cmp_name_ci, for_each_bgchange_pair,
        is_dir_ci, is_file_ci, lc_name, list_image_candidates, match_bg_file,
        resolve_background_changes_like_itg, resolve_music_path_like_itg, resolve_song_assets,
        strip_newlines,
    };

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should follow the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("rssp-assets-test-{}-{unique}", std::process::id()));
            std::fs::create_dir_all(path.join("Visuals"))
                .expect("asset test directory should be creatable");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn split_bgchange_sets_materialized(changes: &str, files: &[String]) -> Vec<Vec<String>> {
        let changes = strip_newlines(changes).into_owned();
        if changes.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<Vec<String>> = Vec::new();
        let mut start = 0usize;
        let mut pnum = 0u8;
        while start <= changes.len() {
            if matches!(pnum, 1 | 7)
                && let Some(found) = match_bg_file(&changes, start, files)
            {
                out.last_mut().unwrap().push(found.to_string());
                start += found.len();
                if let Some(&delimiter) = changes.as_bytes().get(start) {
                    pnum = if delimiter == b'=' { pnum + 1 } else { 0 };
                    start += 1;
                }
                continue;
            }
            if pnum == 0 {
                out.push(Vec::new());
            }
            let remaining = &changes[start..];
            let equals = remaining.find('=').map(|index| start + index);
            let comma = remaining.find(',').map(|index| start + index);
            let Some((end, next_pnum)) = equals
                .zip(comma)
                .map(|(equals, comma)| {
                    if equals < comma {
                        (equals, pnum + 1)
                    } else {
                        (comma, 0)
                    }
                })
                .or_else(|| equals.map(|equals| (equals, pnum + 1)))
                .or_else(|| comma.map(|comma| (comma, 0)))
            else {
                out.last_mut().unwrap().push(changes[start..].to_string());
                break;
            };
            out.last_mut()
                .unwrap()
                .push(changes[start..end].to_string());
            start = end + 1;
            pnum = next_pnum;
        }
        out
    }

    fn assert_streamed_pairs_match(changes: &str, files: &[String]) {
        let expected = split_bgchange_sets_materialized(changes, files)
            .into_iter()
            .filter_map(|fields| Some((fields.first()?.clone(), fields.get(1)?.clone())))
            .collect::<Vec<_>>();
        let mut actual = Vec::new();
        for_each_bgchange_pair(changes, files, |start_beat, target| {
            actual.push((start_beat.to_string(), target.to_string()));
        });
        assert_eq!(actual, expected, "changes={changes:?}");
    }

    #[test]
    fn streamed_bgchange_pairs_match_materialized_sets() {
        let files = [
            "Visuals/Background,Layer.png",
            "Visuals/Overlay,Layer.png",
            "Visuals/A=B, C.png",
            "first.png",
        ]
        .map(str::to_string);
        let cases = [
            "",
            "0=first.png",
            "0=Visuals/Background,Layer.png=1.000=0=0=1=StretchNoLoop==",
            concat!(
                "0=first.png=1=0=0=0=0=Visuals/Overlay,Layer.png,",
                "4=Visuals/A=B, C.png"
            ),
            "0=\nVisuals/Background,Layer.png,\r\n4=first.png",
            ",0=,4,8=first.png,12=first.png=1=0=0=0=0=",
        ];

        for changes in cases {
            assert_streamed_pairs_match(changes, &files);
        }

        let alphabet = b"01=,\n\r abcXYZ/";
        let mut state = 0x7265_7373_7062_6763u64;
        for case_index in 0..2_048 {
            let len = case_index % 96;
            let mut changes = String::with_capacity(len);
            for _ in 0..len {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                changes.push(char::from(alphabet[state as usize % alphabet.len()]));
            }
            assert_streamed_pairs_match(&changes, &files);
        }
    }

    #[test]
    fn streamed_bgchanges_preserve_resolution_and_upsert_behavior() {
        let temp = TempDir::new();
        for relative in [
            "first.png",
            "replacement.png",
            "Visuals/Background,Layer.png",
        ] {
            std::fs::write(temp.0.join(relative), []).expect("asset test file should be writable");
        }
        let simfile = concat!(
            "#BGCHANGES:",
            "0=first.png,",
            "4=Visuals/Background,Layer.png,",
            "-1=-random-,",
            "0=replacement.png,",
            "8=missing.png,",
            "12=-nosongbg-;\n"
        );

        let actual = resolve_background_changes_like_itg(&temp.0, simfile.as_bytes());
        let expected = vec![
            ResolvedBackgroundChange {
                start_beat: -1.0,
                target: BackgroundChangeTarget::Random,
            },
            ResolvedBackgroundChange {
                start_beat: 0.0,
                target: BackgroundChangeTarget::File(temp.0.join("replacement.png")),
            },
            ResolvedBackgroundChange {
                start_beat: 4.0,
                target: BackgroundChangeTarget::File(temp.0.join("Visuals/Background,Layer.png")),
            },
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn allocation_free_name_comparison_matches_lowercase_keys() {
        let paths = [
            PathBuf::from(""),
            PathBuf::from("Alpha.OGG"),
            PathBuf::from("alpha.ogg"),
            PathBuf::from("INTRO-theme.ogg"),
            PathBuf::from("Track-010.ogg"),
            PathBuf::from("café.OGG"),
            PathBuf::from("二.ogg"),
        ];

        for left in &paths {
            for right in &paths {
                assert_eq!(
                    cmp_name_ci(left, right),
                    lc_name(left).cmp(&lc_name(right)),
                    "left={left:?}, right={right:?}"
                );
            }
        }
    }

    #[test]
    fn case_insensitive_lookup_and_music_fallback_preserve_selection() {
        let temp = TempDir::new();
        let album = temp.0.join("MixedCaseAlbum");
        std::fs::create_dir(&album).expect("asset test album should be creatable");
        let chart = album.join("Chart.DAT");
        std::fs::write(&chart, []).expect("asset test chart should be writable");

        assert_eq!(is_dir_ci(&temp.0, "mixedcasealbum"), Some(album.clone()));
        assert_eq!(is_file_ci(&album, "chart.dat"), Some(chart));
        assert_eq!(resolve_music_path_like_itg(&album, ""), None);

        let track_b = album.join("Track-B.ogg");
        std::fs::write(&track_b, []).expect("asset test sound should be writable");
        assert_eq!(
            resolve_music_path_like_itg(&album, ""),
            Some(track_b.clone())
        );

        std::fs::write(album.join("INTRO-theme.OGG"), [])
            .expect("asset test intro should be writable");
        assert_eq!(
            resolve_music_path_like_itg(&album, ""),
            Some(track_b.clone())
        );

        let track_a = album.join("track-A.ogg");
        std::fs::write(&track_a, []).expect("asset test sound should be writable");
        assert_eq!(resolve_music_path_like_itg(&album, ""), Some(track_a));
    }

    #[test]
    fn movie_fallback_only_applies_to_exactly_one_candidate() {
        let temp = TempDir::new();
        let movie = temp.0.join("Movie.MP4");
        std::fs::write(&movie, []).expect("asset test movie should be writable");

        assert_eq!(
            resolve_background_changes_like_itg(&temp.0, b""),
            vec![ResolvedBackgroundChange {
                start_beat: 0.0,
                target: BackgroundChangeTarget::File(movie),
            }]
        );

        std::fs::write(temp.0.join("Second.mkv"), [])
            .expect("second asset test movie should be writable");
        assert!(resolve_background_changes_like_itg(&temp.0, b"").is_empty());
    }

    fn png_header(width: u32, height: u32) -> [u8; 24] {
        let mut header = [0u8; 24];
        header[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        header[12..16].copy_from_slice(b"IHDR");
        header[16..20].copy_from_slice(&width.to_be_bytes());
        header[20..24].copy_from_slice(&height.to_be_bytes());
        header
    }

    #[test]
    fn image_catalog_and_song_asset_selection_match_materialized_order() {
        let temp = TempDir::new();
        let hints = temp.0.join("Hints");
        std::fs::create_dir(&hints).expect("hint directory should be creatable");
        for name in [
            "00-background.PNG",
            "01-Banner.png",
            "02-bn.png",
            "03-bg.png",
        ] {
            std::fs::write(hints.join(name), png_header(64, 64))
                .expect("hint image should be writable");
        }
        std::fs::write(hints.join("._ignored.png"), png_header(64, 64))
            .expect("resource fork fixture should be writable");
        std::fs::write(hints.join("not-an-image.dat"), [])
            .expect("non-image fixture should be writable");

        let mut expected_paths = std::fs::read_dir(&hints)
            .expect("hint directory should be readable")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                !super::is_mac_resource_fork(path)
                    && path.is_file()
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| super::img_rank(extension).is_some())
            })
            .collect::<Vec<_>>();
        expected_paths.sort_by_cached_key(|path| lc_name(path));
        let actual_paths = list_image_candidates(&hints);
        assert_eq!(actual_paths, expected_paths);
        assert_eq!(
            resolve_song_assets(&hints, "", ""),
            (
                Some(hints.join("01-Banner.png")),
                Some(hints.join("00-background.PNG")),
            )
        );

        let dimensions = temp.0.join("Dimensions");
        std::fs::create_dir(&dimensions).expect("dimension directory should be creatable");
        std::fs::write(dimensions.join("00.png"), png_header(640, 480))
            .expect("background dimension fixture should be writable");
        std::fs::write(dimensions.join("01.png"), png_header(300, 100))
            .expect("banner dimension fixture should be writable");
        for index in 2..16 {
            std::fs::write(
                dimensions.join(format!("{index:02}.png")),
                png_header(640, 480),
            )
            .expect("trailing dimension fixture should be writable");
        }
        assert_eq!(
            resolve_song_assets(&dimensions, "", ""),
            (
                Some(dimensions.join("01.png")),
                Some(dimensions.join("00.png")),
            )
        );
    }
}
