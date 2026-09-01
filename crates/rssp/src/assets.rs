use std::borrow::Cow;
use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

use memchr::memchr2;

use crate::parse::{bgchanges_values, decode_bytes, unescape_tag};

const RANDOM_BACKGROUND_FILE: &str = "-random-";
const NO_SONG_BG_FILE: &str = "-nosongbg-";
const BG_BEAT_FILTER_WORDS: usize = 1_024;
const BG_BEAT_FILTER_MASK: usize = BG_BEAT_FILTER_WORDS * u64::BITS as usize - 1;
type BgBeatFilter = [u64; BG_BEAT_FILTER_WORDS];

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

pub(crate) fn to_slash(s: &str) -> Cow<'_, str> {
    if s.contains('\\') {
        Cow::Owned(s.replace('\\', "/"))
    } else {
        Cow::Borrowed(s)
    }
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

pub(crate) fn cmp_ascii_ci(left: &[u8], right: &[u8]) -> Ordering {
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

pub(crate) fn cmp_os_ci(left: &OsStr, right: &OsStr) -> Ordering {
    let left = left.to_string_lossy();
    let right = right.to_string_lossy();
    cmp_ascii_ci(left.as_bytes(), right.as_bytes())
}

pub(crate) fn entry_is_file(entry: &fs::DirEntry) -> bool {
    match entry.file_type() {
        Ok(file_type) => file_type.is_file() || (file_type.is_symlink() && entry.path().is_file()),
        Err(_) => entry.path().is_file(),
    }
}

pub(crate) fn entry_is_dir(entry: &fs::DirEntry) -> bool {
    match entry.file_type() {
        Ok(file_type) => file_type.is_dir() || (file_type.is_symlink() && entry.path().is_dir()),
        Err(_) => entry.path().is_dir(),
    }
}

fn list_image_names(dir: &Path) -> Vec<OsString> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut candidates = Vec::with_capacity(32);
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
        if !entry_is_file(&entry) {
            continue;
        }
        candidates.push(name);
    }
    candidates.sort_by(|left, right| cmp_os_ci(left, right));
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

// Inline the common file-plus-three-directories case; deeper paths keep the
// exact collected fallback instead of imposing an artificial depth limit.
const INLINE_REL_COMPONENTS: usize = 4;

fn collect_rel_parts(rel: &str) -> Option<Vec<&str>> {
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
    (!parts.is_empty()).then_some(parts)
}

fn fill_rel_parts<'a>(rel: &'a str, parts: &mut [&'a str; INLINE_REL_COMPONENTS]) -> Option<usize> {
    let mut len = 0usize;
    for part in rel.split('/').map(str::trim) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            len = len.checked_sub(1)?;
        } else {
            if len == INLINE_REL_COMPONENTS {
                return None;
            }
            parts[len] = part;
            len += 1;
        }
    }
    Some(len)
}

fn resolve_rel_parts(base: &Path, parts: &[&str]) -> Option<PathBuf> {
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

fn resolve_rel_ci_deep(base: &Path, rel: &str) -> Option<PathBuf> {
    resolve_rel_parts(base, &collect_rel_parts(rel)?)
}

fn resolve_rel_ci(base: &Path, rel: &str) -> Option<PathBuf> {
    let rel = to_slash(rel);
    let mut parts = [""; INLINE_REL_COMPONENTS];
    let Some(len) = fill_rel_parts(rel.as_ref(), &mut parts) else {
        return resolve_rel_ci_deep(base, rel.as_ref());
    };
    resolve_rel_parts(base, &parts[..len])
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

#[inline(always)]
fn first_two_sound_files(song_dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let Ok(entries) = fs::read_dir(song_dir) else {
        return (None, None);
    };
    let mut first: Option<OsString> = None;
    let mut second: Option<OsString> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let path = Path::new(&name);
        if is_mac_resource_fork(path) || !is_sound_ext(path) || !entry_is_file(&entry) {
            continue;
        }
        if first
            .as_ref()
            .is_none_or(|candidate| cmp_os_ci(&name, candidate) == Ordering::Less)
        {
            second = first.replace(name);
        } else if second
            .as_ref()
            .is_none_or(|candidate| cmp_os_ci(&name, candidate) == Ordering::Less)
        {
            second = Some(name);
        }
    }
    (
        first.map(|name| song_dir.join(name)),
        second.map(|name| song_dir.join(name)),
    )
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

    pick_music_fallback(first_two_sound_files(song_dir))
}

fn pick_music_fallback((first, second): (Option<PathBuf>, Option<PathBuf>)) -> Option<PathBuf> {
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

    let images = list_image_names(song_dir);
    if banner.is_none() || background.is_none() {
        for name in &images {
            let image = Path::new(name);
            if banner.is_none() && image_hint_matches(image, b"banner", b"bn") {
                banner = Some(song_dir.join(name));
            }
            if background.is_none() && image_hint_matches(image, b"background", b"bg") {
                background = Some(song_dir.join(name));
            }
            if banner.is_some() && background.is_some() {
                break;
            }
        }
    }

    if banner.is_some() && background.is_some() {
        return (banner, background);
    }

    for name in &images {
        if banner.is_some() && background.is_some() {
            break;
        }
        if background
            .as_deref()
            .is_some_and(|path| root_asset_eq(path, song_dir, name))
        {
            continue;
        }
        if banner
            .as_deref()
            .is_some_and(|path| root_asset_eq(path, song_dir, name))
        {
            continue;
        }
        let image = song_dir.join(name);
        let Some((w, h)) = img_dims(&image) else {
            continue;
        };
        if background.is_none() && w >= 320 && h >= 240 {
            background = Some(image);
            continue;
        }
        if banner.is_none() && (100..=320).contains(&w) && (50..=240).contains(&h) {
            banner = Some(image);
            continue;
        }
        if banner.is_none() && w > 200 && h > 0 && (w as f32 / h as f32) > 2.0 {
            banner = Some(image);
        }
    }

    (banner, background)
}

fn root_asset_eq(path: &Path, song_dir: &Path, name: &OsStr) -> bool {
    path.parent() == Some(song_dir) && path.file_name() == Some(name)
}

struct BgFileCatalog {
    files: Vec<String>,
    bucket_ranges: [(usize, usize); 256],
}

const INLINE_BG_RESOLUTION_FILES: usize = 2_048;
const BG_RESOLUTION_STATUSES_PER_WORD: usize = u64::BITS as usize / 2;
const INLINE_BG_RESOLUTION_WORDS: usize =
    INLINE_BG_RESOLUTION_FILES / BG_RESOLUTION_STATUSES_PER_WORD;

struct BgResolutionStatus {
    inline: [u64; INLINE_BG_RESOLUTION_WORDS],
    overflow: Option<Box<[u64]>>,
}

impl BgResolutionStatus {
    fn new(file_count: usize) -> Self {
        let overflow = (file_count > INLINE_BG_RESOLUTION_FILES).then(|| {
            vec![0; file_count.div_ceil(BG_RESOLUTION_STATUSES_PER_WORD)].into_boxed_slice()
        });
        Self {
            inline: [0; INLINE_BG_RESOLUTION_WORDS],
            overflow,
        }
    }

    fn get(&self, file_index: usize) -> u8 {
        let words = self.overflow.as_deref().unwrap_or(&self.inline);
        let shift = (file_index % BG_RESOLUTION_STATUSES_PER_WORD) * 2;
        ((words[file_index / BG_RESOLUTION_STATUSES_PER_WORD] >> shift) & 0b11) as u8
    }

    fn set(&mut self, file_index: usize, status: u8) {
        let words = self.overflow.as_deref_mut().unwrap_or(&mut self.inline);
        let shift = (file_index % BG_RESOLUTION_STATUSES_PER_WORD) * 2;
        let word = &mut words[file_index / BG_RESOLUTION_STATUSES_PER_WORD];
        *word = (*word & !(0b11 << shift)) | (u64::from(status) << shift);
    }
}

impl BgFileCatalog {
    fn from_files(mut files: Vec<String>) -> Self {
        sort_bg_files(&mut files);
        Self::from_sorted(files)
    }

    fn from_sorted(files: Vec<String>) -> Self {
        let mut bucket_ranges = [(0, 0); 256];
        for (index, file) in files.iter().enumerate() {
            let range = &mut bucket_ranges[bg_file_bucket(file)];
            if range.0 == range.1 {
                range.0 = index;
            }
            range.1 = index + 1;
        }

        Self {
            files,
            bucket_ranges,
        }
    }
}

fn cmp_bg_file(left: &String, right: &String) -> std::cmp::Ordering {
    bg_file_bucket(left)
        .cmp(&bg_file_bucket(right))
        .then_with(|| right.len().cmp(&left.len()))
        .then_with(|| left.cmp(right))
}

fn sort_bg_files(files: &mut [String]) {
    files.sort_unstable_by(cmp_bg_file);
}

#[inline(always)]
fn bg_file_bucket(file: &str) -> usize {
    file.as_bytes()
        .first()
        .copied()
        .map_or(0, |byte| byte.to_ascii_lowercase() as usize)
}

fn list_song_dir_rel_files(song_dir: &Path) -> (BgFileCatalog, Option<PathBuf>) {
    let mut dirs = vec![song_dir.to_path_buf()];
    let mut files = Vec::new();
    let mut only_movie = None;
    let mut movies_ambiguous = false;
    while let Some(dir) = dirs.pop() {
        let is_root = dir == song_dir;
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_type = entry.file_type().ok();
            let path = entry.path();
            let is_dir = file_type.map_or_else(
                || path.is_dir(),
                |kind| kind.is_dir() || (kind.is_symlink() && path.is_dir()),
            );
            if is_dir {
                dirs.push(path);
                continue;
            }
            let Ok(rel) = path.strip_prefix(song_dir) else {
                continue;
            };
            files.push(to_slash(&rel.to_string_lossy()).into_owned());
            if is_root
                && !movies_ambiguous
                && !is_mac_resource_fork(&path)
                && is_movie_ext(&path)
                && file_type.map_or_else(
                    || path.is_file(),
                    |kind| kind.is_file() || (kind.is_symlink() && path.is_file()),
                )
            {
                if only_movie.is_some() {
                    only_movie = None;
                    movies_ambiguous = true;
                } else {
                    only_movie = Some(path);
                }
            }
        }
    }
    (BgFileCatalog::from_files(files), only_movie)
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

fn match_bg_file(
    changes: &str,
    start: usize,
    files: &BgFileCatalog,
) -> Option<(usize, Option<usize>)> {
    let first = *changes.as_bytes().get(start)?;
    let (bucket_start, bucket_end) = files.bucket_ranges[first.to_ascii_lowercase() as usize];
    let mut fallback = None;
    let mut fallback_is_ambiguous = false;
    for file_index in bucket_start..bucket_end {
        let file = &files.files[file_index];
        if fallback.is_some_and(|(file_len, _)| file.len() < file_len) {
            break;
        }
        let Some(head) = changes.get(start..start + file.len()) else {
            continue;
        };
        if !head.eq_ignore_ascii_case(file) {
            continue;
        }
        let next = start + file.len();
        if matches!(changes.as_bytes().get(next), None | Some(b'=' | b',')) {
            if head == file {
                return Some((file.len(), Some(file_index)));
            }
            if fallback.is_some() {
                fallback_is_ambiguous = true;
            } else {
                fallback = Some((file.len(), file_index));
            }
        }
    }
    fallback
        .map(|(file_len, file_index)| (file_len, (!fallback_is_ambiguous).then_some(file_index)))
}

fn find_bg_delimiter(rem: &str) -> Option<usize> {
    memchr2(b'=', b',', rem.as_bytes())
}

fn for_each_bgchange_pair_with(
    changes: &str,
    files: &BgFileCatalog,
    mut handle: impl FnMut(&str, &str, Option<usize>),
) {
    let changes = strip_newlines(changes);
    if changes.is_empty() {
        return;
    }

    let changes = changes.as_ref();
    let mut start = 0usize;
    let mut pnum = 0u8;
    let mut start_beat = None;
    let mut target = None;
    let mut target_file_index = None;
    while start <= changes.len() {
        let (field, delimiter, file_index) = if (pnum == 1 || pnum == 7)
            && let Some((file_len, file_index)) = match_bg_file(changes, start, files)
        {
            let found = &changes[start..start + file_len];
            start += file_len;
            let delimiter = changes.as_bytes().get(start).copied();
            if delimiter.is_some() {
                start += 1;
            }
            (found, delimiter, file_index)
        } else {
            let rem = &changes[start..];
            let end = start + find_bg_delimiter(rem).unwrap_or(rem.len());
            let field = &changes[start..end];
            let delimiter = changes.as_bytes().get(end).copied();
            start = end + usize::from(delimiter.is_some());
            (field, delimiter, None)
        };

        match pnum {
            0 => start_beat = Some(field),
            1 => {
                target = Some(field);
                target_file_index = file_index;
            }
            _ => {}
        }

        match delimiter {
            Some(b'=') => pnum += 1,
            Some(b',') => {
                if let (Some(start_beat), Some(target)) = (start_beat, target) {
                    handle(start_beat, target, target_file_index);
                }
                start_beat = None;
                target = None;
                target_file_index = None;
                pnum = 0;
            }
            None => {
                if let (Some(start_beat), Some(target)) = (start_beat, target) {
                    handle(start_beat, target, target_file_index);
                }
                break;
            }
            Some(_) => unreachable!("background change delimiter must be '=' or ','"),
        }
    }
}
fn join_rel(base: &Path, relative: &str) -> PathBuf {
    let mut joined = PathBuf::with_capacity(
        base.as_os_str()
            .len()
            .saturating_add(relative.len())
            .saturating_add(1),
    );
    joined.push(base);
    joined.push(relative);
    joined
}

fn resolve_bgchange_target(
    song_dir: &Path,
    target_name: &str,
    file_index: Option<usize>,
    files: &BgFileCatalog,
    resolution_status: &mut BgResolutionStatus,
) -> Option<BackgroundChangeTarget> {
    let target_name = target_name.trim();
    if target_name.is_empty() {
        return None;
    }
    if target_name.eq_ignore_ascii_case(NO_SONG_BG_FILE) {
        return Some(BackgroundChangeTarget::NoSongBg);
    }
    if target_name.eq_ignore_ascii_case(RANDOM_BACKGROUND_FILE) {
        return Some(BackgroundChangeTarget::Random);
    }
    if let Some(file_index) = file_index {
        let relative = &files.files[file_index];
        return match resolution_status.get(file_index) {
            1 => Some(BackgroundChangeTarget::File(join_rel(song_dir, relative))),
            2 => None,
            _ => {
                let path = join_rel(song_dir, relative);
                if !is_mac_resource_fork(&path) && path.is_file() {
                    resolution_status.set(file_index, 1);
                    Some(BackgroundChangeTarget::File(path))
                } else {
                    resolution_status.set(file_index, 2);
                    None
                }
            }
        };
    }
    resolve_asset(song_dir, target_name).map(BackgroundChangeTarget::File)
}

fn parse_bgchange_pair(
    song_dir: &Path,
    start_beat: &str,
    target: &str,
    file_index: Option<usize>,
    files: &BgFileCatalog,
    resolution_status: &mut BgResolutionStatus,
) -> Option<ResolvedBackgroundChange> {
    let start_beat = start_beat.trim().parse::<f32>().unwrap_or(0.0);
    let target = resolve_bgchange_target(song_dir, target, file_index, files, resolution_status)?;
    Some(ResolvedBackgroundChange { start_beat, target })
}

#[inline(always)]
fn upsert_bgchange(
    out: &mut Vec<ResolvedBackgroundChange>,
    change: ResolvedBackgroundChange,
    beats_ordered: &mut bool,
    beat_filter: &mut Option<BgBeatFilter>,
) {
    if *beats_ordered && let Some(last) = out.last_mut() {
        if last.start_beat == change.start_beat {
            *last = change;
            return;
        }
        if last.start_beat < change.start_beat {
            out.push(change);
            return;
        }
        *beats_ordered = false;
    }
    if let Some(last) = out.last_mut()
        && last.start_beat == change.start_beat
    {
        *last = change;
        return;
    }
    if beat_filter.is_none() {
        let mut filter = [0u64; BG_BEAT_FILTER_WORDS];
        for existing in out.iter() {
            mark_bg_beat(&mut filter, existing.start_beat);
        }
        *beat_filter = Some(filter);
    }
    let maybe_seen = beat_filter
        .as_mut()
        .is_some_and(|filter| mark_bg_beat(filter, change.start_beat));
    if !maybe_seen {
        out.push(change);
        return;
    }
    if let Some(slot) = out
        .iter_mut()
        .find(|existing| existing.start_beat == change.start_beat)
    {
        *slot = change;
    } else {
        out.push(change);
    }
}

// False positives only trigger the exact fallback scan; they cannot change output.
fn mark_bg_beat(filter: &mut BgBeatFilter, beat: f32) -> bool {
    if beat.is_nan() {
        return false;
    }
    let key = if beat == 0.0 { 0 } else { beat.to_bits() };
    let mut hash = u64::from(key);
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7FEB_352D);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x846C_A68B);
    hash ^= hash >> 16;
    let first = hash as usize & BG_BEAT_FILTER_MASK;
    let second = hash.rotate_left(29) as usize & BG_BEAT_FILTER_MASK;
    let first_mask = 1u64 << (first & 63);
    let second_mask = 1u64 << (second & 63);
    let seen = filter[first >> 6] & first_mask != 0 && filter[second >> 6] & second_mask != 0;
    filter[first >> 6] |= first_mask;
    filter[second >> 6] |= second_mask;
    seen
}

fn push_beat_zero(
    out: &mut Vec<ResolvedBackgroundChange>,
    target: BackgroundChangeTarget,
    beats_ordered: &mut bool,
) {
    if *beats_ordered
        && out
            .last()
            .is_some_and(|last| last.start_beat.total_cmp(&0.0).is_gt())
    {
        *beats_ordered = false;
    }
    out.push(ResolvedBackgroundChange {
        start_beat: 0.0,
        target,
    });
}

fn sort_bgchanges(out: &mut [ResolvedBackgroundChange], beats_ordered: bool) {
    if !beats_ordered {
        out.sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
    }
}

fn resolve_bgchanges_with<'a>(
    song_dir: &Path,
    values: impl IntoIterator<Item = &'a [u8]>,
    files: &BgFileCatalog,
    fallback_movie: impl FnOnce() -> Option<PathBuf>,
) -> Vec<ResolvedBackgroundChange> {
    let mut resolution_status = BgResolutionStatus::new(files.files.len());
    let mut out: Vec<ResolvedBackgroundChange> = Vec::new();
    let mut saw_no_song_bg = false;
    let mut beats_ordered = true;
    let mut beat_filter = None;
    for raw in values {
        let decoded = decode_bytes(raw);
        let text = unescape_tag(decoded.as_ref());
        for_each_bgchange_pair_with(text.as_ref(), files, |start_beat, target, file_index| {
            let Some(change) = parse_bgchange_pair(
                song_dir,
                start_beat,
                target,
                file_index,
                files,
                &mut resolution_status,
            ) else {
                return;
            };
            if matches!(change.target, BackgroundChangeTarget::NoSongBg) {
                saw_no_song_bg = true;
                return;
            }
            upsert_bgchange(&mut out, change, &mut beats_ordered, &mut beat_filter);
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
    if !has_explicit_movie && let Some(movie) = fallback_movie() {
        if saw_no_song_bg {
            if let Some(ix) = beat_zero_still_ix {
                out[ix].target = BackgroundChangeTarget::File(movie);
            } else if !blocks_beat_zero {
                push_beat_zero(
                    &mut out,
                    BackgroundChangeTarget::File(movie),
                    &mut beats_ordered,
                );
            }
        } else if !has_any_file && !blocks_beat_zero {
            push_beat_zero(
                &mut out,
                BackgroundChangeTarget::File(movie),
                &mut beats_ordered,
            );
        }
    }
    sort_bgchanges(&mut out, beats_ordered);
    out
}

#[must_use]
pub fn resolve_background_changes_like_itg(
    song_dir: &Path,
    simfile_data: &[u8],
) -> Vec<ResolvedBackgroundChange> {
    let (files, movie) = list_song_dir_rel_files(song_dir);
    resolve_bgchanges_with(song_dir, bgchanges_values(simfile_data), &files, || movie)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        BackgroundChangeTarget, BgFileCatalog, BgResolutionStatus, ResolvedBackgroundChange,
        is_dir_ci, is_file_ci, join_rel, match_bg_file, resolve_background_changes_like_itg,
        resolve_music_path_like_itg, resolve_song_assets,
    };

    #[test]
    fn relative_join_handles_common_paths() {
        for base in [
            Path::new(""),
            Path::new("Songs/Pack/Song"),
            Path::new("C:\\Songs\\曲"),
        ] {
            for relative in [
                "",
                "banner.png",
                "Visuals/Background,Layer.png",
                "../movie.mp4",
            ] {
                assert_eq!(join_rel(base, relative), base.join(relative));
            }
        }
    }

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
    fn indexed_bgchanges_preserve_case_insensitive_repeated_resolution() {
        let temp = TempDir::new();
        let relative = "Visuals/Mixed,Case.PNG";
        std::fs::write(temp.0.join(relative), []).expect("asset test file should be writable");
        let simfile = concat!(
            "#BGCHANGES:",
            "0=visuals/mixed,case.png,",
            "4=VISUALS/MIXED,CASE.PNG,",
            "8=Visuals/Mixed,Case.PNG;\n"
        );

        let actual = resolve_background_changes_like_itg(&temp.0, simfile.as_bytes());
        let expected_path = temp.0.join(relative);
        assert_eq!(actual.len(), 3);
        for (change, start_beat) in actual.iter().zip([0.0, 4.0, 8.0]) {
            assert_eq!(change.start_beat, start_beat);
            assert_eq!(
                change.target,
                BackgroundChangeTarget::File(expected_path.clone())
            );
        }
    }

    #[test]
    fn bg_resolution_status_preserves_inline_and_overflow_entries() {
        for file_count in [2_048, 2_049] {
            let mut status = BgResolutionStatus::new(file_count);
            let mut entries = vec![(0, 1), (31, 2), (32, 1), (2_047, 2)];
            if file_count > 2_048 {
                entries.push((2_048, 1));
            }
            for &(file_index, value) in &entries {
                status.set(file_index, value);
            }
            for (file_index, value) in entries {
                assert_eq!(status.get(file_index), value);
            }
            assert_eq!(status.get(1), 0);
        }
    }

    #[test]
    fn bg_file_catalog_only_indexes_unambiguous_or_exact_case_matches() {
        let files = BgFileCatalog::from_files(
            ["Alpha.png", "alpha.png", "Beta.png"]
                .map(str::to_string)
                .to_vec(),
        );

        assert_eq!(match_bg_file("0=ALPHA.PNG,", 2, &files), Some((9, None)));
        let (file_len, file_index) =
            match_bg_file("0=alpha.png,", 2, &files).expect("exact filename should match");
        assert_eq!(file_len, 9);
        assert_eq!(
            &files.files[file_index.expect("exact filename should retain its catalog index")],
            "alpha.png"
        );
        assert!(match_bg_file("0=missing.png,", 2, &files).is_none());
    }

    #[test]
    fn bg_file_sort_uses_length_then_name() {
        let mut files = [
            "z/short.png",
            "Alpha.png",
            "alpha.png",
            "A/longer-name.png",
            "beta.png",
            "Beta.png",
            "alpha.png",
            "Æ/visual.png",
            "",
        ]
        .map(str::to_string)
        .to_vec();
        super::sort_bg_files(&mut files);
        assert!(
            files
                .windows(2)
                .all(|pair| super::cmp_bg_file(&pair[0], &pair[1]).is_le())
        );
    }

    #[test]
    fn ordered_bgchange_upsert_matches_linear_reference() {
        fn linear_upsert(
            out: &mut Vec<ResolvedBackgroundChange>,
            change: ResolvedBackgroundChange,
        ) {
            if let Some(slot) = out
                .iter_mut()
                .find(|existing| existing.start_beat == change.start_beat)
            {
                *slot = change;
            } else {
                out.push(change);
            }
        }

        let cases = [
            vec![0.0, 4.0, 8.0, 8.0, 12.0],
            vec![8.0, 4.0, 8.0, 0.0, 4.0],
            vec![0.0, -0.0, 0.0],
            vec![f32::NAN, 1.0, f32::NAN],
        ];
        for beats in cases {
            let mut actual = Vec::new();
            let mut expected = Vec::new();
            let mut beats_ordered = true;
            for (index, start_beat) in beats.into_iter().enumerate() {
                let change = ResolvedBackgroundChange {
                    start_beat,
                    target: BackgroundChangeTarget::File(PathBuf::from(index.to_string())),
                };
                linear_upsert(&mut expected, change.clone());
                super::upsert_bgchange(&mut actual, change, &mut beats_ordered, &mut None);
            }

            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.iter().zip(&expected) {
                assert_eq!(actual.start_beat.to_bits(), expected.start_beat.to_bits());
                assert_eq!(actual.target, expected.target);
            }
        }
    }

    #[test]
    fn ascii_name_comparison_matches_lowercase_keys() {
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
                    super::cmp_os_ci(left.as_os_str(), right.as_os_str()),
                    left.to_string_lossy()
                        .to_ascii_lowercase()
                        .cmp(&right.to_string_lossy().to_ascii_lowercase()),
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
        std::fs::write(temp.0.join("._Ignored.mkv"), [])
            .expect("resource fork movie should be writable");
        std::fs::write(temp.0.join("Visuals").join("Nested.avi"), [])
            .expect("nested movie should be writable");

        let expected = vec![ResolvedBackgroundChange {
            start_beat: 0.0,
            target: BackgroundChangeTarget::File(movie),
        }];
        for simfile in [b"".as_slice(), b"#BGCHANGES:0=-nosongbg-;".as_slice()] {
            assert_eq!(
                resolve_background_changes_like_itg(&temp.0, simfile),
                expected
            );
        }

        let still = temp.0.join("still.png");
        std::fs::write(&still, []).expect("still image should be writable");
        let simfile = b"#BGCHANGES:0=still.png;";
        let expected = vec![ResolvedBackgroundChange {
            start_beat: 0.0,
            target: BackgroundChangeTarget::File(still),
        }];
        assert_eq!(
            resolve_background_changes_like_itg(&temp.0, simfile),
            expected
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
    fn song_asset_selection_uses_name_and_dimension_hints() {
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
