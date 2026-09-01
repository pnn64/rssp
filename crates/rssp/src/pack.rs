use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::assets;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DupPolicy {
    #[default]
    First,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPref {
    Default,
    Null,
    Itg,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOpt {
    pub dup: DupPolicy,
}

#[derive(Debug)]
pub enum ScanError {
    Io(io::Error),
    InvalidUtf8Path,
    DuplicateSimfile {
        ext: &'static str,
        paths: Vec<PathBuf>,
    },
}

impl From<io::Error> for ScanError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone)]
pub struct SongScan {
    pub dir: PathBuf,
    pub simfile: PathBuf,
    /// Normalized to `"sm"` or `"ssc"`.
    pub extension: &'static str,
}

#[derive(Debug, Clone)]
pub struct PackScan {
    pub dir: PathBuf,
    pub group_name: String,
    pub display_title: String,
    pub sort_title: String,
    pub translit_title: String,
    pub series: String,
    pub year: i32,
    pub version: i32,
    pub has_pack_ini: bool,
    pub sync_pref: SyncPref,
    pub banner_path: Option<PathBuf>,
    pub background_path: Option<PathBuf>,
    pub songs: Vec<SongScan>,
}

struct CompactKey {
    start: u32,
    end: u32,
    original: u32,
}

fn sort_compact_ci<T, const IN_PLACE: bool>(
    values: &mut [T],
    estimate: usize,
    mut append_key: impl FnMut(&T, &mut Vec<u8>),
    fallback: impl FnMut(&T, &T) -> std::cmp::Ordering,
) {
    if u32::try_from(values.len()).is_err() {
        values.sort_by(fallback);
        return;
    }
    let mut text = Vec::with_capacity(values.len().saturating_mul(estimate));
    let mut keys = Vec::with_capacity(values.len());
    let mut compact = true;
    for (original, value) in values.iter().enumerate() {
        let start = text.len();
        append_key(value, &mut text);
        if u32::try_from(text.len()).is_err() {
            compact = false;
            break;
        }
        keys.push(CompactKey {
            start: start as u32,
            end: text.len() as u32,
            original: original as u32,
        });
    }
    if !compact {
        values.sort_by(fallback);
        return;
    }
    if IN_PLACE {
        keys.sort_unstable_by(|left, right| {
            text[left.start as usize..left.end as usize]
                .cmp(&text[right.start as usize..right.end as usize])
                .then_with(|| left.original.cmp(&right.original))
        });
    } else {
        keys.sort_by(|left, right| {
            text[left.start as usize..left.end as usize]
                .cmp(&text[right.start as usize..right.end as usize])
        });
    }

    let mut destinations = vec![0u32; values.len()];
    for (target, key) in keys.iter().enumerate() {
        destinations[key.original as usize] = target as u32;
    }
    for index in 0..values.len() {
        while destinations[index] as usize != index {
            let target = destinations[index] as usize;
            values.swap(index, target);
            destinations.swap(index, target);
        }
    }
}

#[cfg(any(test, feature = "profile"))]
fn sort_paths_ci(paths: &mut [PathBuf]) {
    sort_paths_ci_with::<true>(paths);
}

#[cfg(any(test, feature = "profile"))]
fn sort_paths_ci_with<const IN_PLACE: bool>(paths: &mut [PathBuf]) {
    sort_compact_ci::<_, IN_PLACE>(
        paths,
        24,
        |path, text| {
            if let Some(name) = path.file_name() {
                text.extend(
                    name.to_string_lossy()
                        .as_bytes()
                        .iter()
                        .map(u8::to_ascii_lowercase),
                );
            }
        },
        |left, right| assets::cmp_name_ci(left, right),
    );
}

fn sort_names_ci(names: &mut [OsString]) {
    names.sort_by(|left, right| assets::cmp_os_ci(left, right));
}

fn sort_packs_ci(packs: &mut [PackScan]) {
    sort_compact_ci::<_, true>(
        packs,
        24,
        |pack, text| {
            text.extend(
                pack.group_name
                    .as_bytes()
                    .iter()
                    .map(u8::to_ascii_lowercase),
            );
        },
        |left, right| assets::cmp_ascii_ci(left.group_name.as_bytes(), right.group_name.as_bytes()),
    );
}

#[cfg(feature = "profile")]
pub(crate) fn profile_sort_paths_ci(paths: &mut [PathBuf], legacy: bool) {
    if legacy {
        paths.sort_by_cached_key(|path| assets::lc_name(path));
    } else {
        sort_paths_ci(paths);
    }
}

#[cfg(feature = "profile")]
pub(crate) fn profile_sort_paths_ci_in_place(paths: &mut [PathBuf], in_place: bool) {
    if in_place {
        sort_paths_ci_with::<true>(paths);
    } else {
        sort_paths_ci_with::<false>(paths);
    }
}

#[cfg(any(test, feature = "profile"))]
fn should_replace(first: Option<&Path>, candidate: &Path) -> bool {
    first.is_none_or(|current| assets::cmp_name_ci(candidate, current).is_lt())
}

#[cfg(any(test, feature = "profile"))]
fn keep_first_path(first: &mut Option<PathBuf>, candidate: PathBuf) {
    if should_replace(first.as_deref(), &candidate) {
        *first = Some(candidate);
    }
}

#[cfg(feature = "profile")]
fn keep_first_ref(first: &mut Option<PathBuf>, candidate: &Path) {
    if should_replace(first.as_deref(), candidate) {
        *first = Some(candidate.to_path_buf());
    }
}

fn keep_first_name(first: &mut Option<OsString>, candidate: &OsStr) {
    if first
        .as_ref()
        .is_none_or(|current| assets::cmp_os_ci(candidate, current).is_lt())
    {
        *first = Some(candidate.to_os_string());
    }
}

fn keep_first_owned(first: &mut Option<OsString>, candidate: OsString) {
    if first
        .as_ref()
        .is_none_or(|current| assets::cmp_os_ci(&candidate, current).is_lt())
    {
        *first = Some(candidate);
    }
}

fn pack_ini_path(pack_dir: &Path) -> PathBuf {
    pack_dir.join("Pack.ini")
}

fn parse_sync_pref(s: &str) -> SyncPref {
    match s.trim() {
        "NULL" => SyncPref::Null,
        "ITG" => SyncPref::Itg,
        _ => SyncPref::Default,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PackIniRaw<T> {
    version: T,
    display_title: T,
    sort_title: T,
    translit_title: T,
    series: T,
    banner: T,
    background: T,
    sync_offset: T,
    year: T,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackIniKey {
    Version,
    DisplayTitle,
    SortTitle,
    TranslitTitle,
    Series,
    Banner,
    Background,
    SyncOffset,
    Year,
    Unknown,
}

fn pack_ini_key(key: &str) -> PackIniKey {
    match key.len() {
        4 if key.eq_ignore_ascii_case("year") => PackIniKey::Year,
        6 if key.eq_ignore_ascii_case("series") => PackIniKey::Series,
        6 if key.eq_ignore_ascii_case("banner") => PackIniKey::Banner,
        7 if key.eq_ignore_ascii_case("version") => PackIniKey::Version,
        9 if key.eq_ignore_ascii_case("sorttitle") => PackIniKey::SortTitle,
        10 if key.eq_ignore_ascii_case("background") => PackIniKey::Background,
        10 if key.eq_ignore_ascii_case("syncoffset") => PackIniKey::SyncOffset,
        12 if key.eq_ignore_ascii_case("displaytitle") => PackIniKey::DisplayTitle,
        13 if key.eq_ignore_ascii_case("translittitle") => PackIniKey::TranslitTitle,
        _ => PackIniKey::Unknown,
    }
}

#[cfg(any(test, feature = "profile"))]
fn pack_ini_key_sequential(key: &str) -> PackIniKey {
    if key.eq_ignore_ascii_case("version") {
        PackIniKey::Version
    } else if key.eq_ignore_ascii_case("displaytitle") {
        PackIniKey::DisplayTitle
    } else if key.eq_ignore_ascii_case("sorttitle") {
        PackIniKey::SortTitle
    } else if key.eq_ignore_ascii_case("translittitle") {
        PackIniKey::TranslitTitle
    } else if key.eq_ignore_ascii_case("series") {
        PackIniKey::Series
    } else if key.eq_ignore_ascii_case("banner") {
        PackIniKey::Banner
    } else if key.eq_ignore_ascii_case("background") {
        PackIniKey::Background
    } else if key.eq_ignore_ascii_case("syncoffset") {
        PackIniKey::SyncOffset
    } else if key.eq_ignore_ascii_case("year") {
        PackIniKey::Year
    } else {
        PackIniKey::Unknown
    }
}

fn parse_pack_ini_with<'a, T: Default, const INDEXED_KEYS: bool>(
    text: &'a str,
    mut take_value: impl FnMut(&'a str) -> T,
) -> PackIniRaw<T> {
    let mut out = PackIniRaw::default();
    let mut in_group = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let sec = line[1..line.len() - 1].trim();
            in_group = sec.eq_ignore_ascii_case("group");
            continue;
        }
        if !in_group {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        let key = if INDEXED_KEYS {
            pack_ini_key(key)
        } else {
            #[cfg(any(test, feature = "profile"))]
            {
                pack_ini_key_sequential(key)
            }
            #[cfg(not(any(test, feature = "profile")))]
            {
                unreachable!("sequential Pack.ini dispatch requires profile feature")
            }
        };
        match key {
            PackIniKey::Version => out.version = take_value(value),
            PackIniKey::DisplayTitle => out.display_title = take_value(value),
            PackIniKey::SortTitle => out.sort_title = take_value(value),
            PackIniKey::TranslitTitle => out.translit_title = take_value(value),
            PackIniKey::Series => out.series = take_value(value),
            PackIniKey::Banner => out.banner = take_value(value),
            PackIniKey::Background => out.background = take_value(value),
            PackIniKey::SyncOffset => out.sync_offset = take_value(value),
            PackIniKey::Year => out.year = take_value(value),
            PackIniKey::Unknown => {}
        }
    }

    out
}

fn parse_pack_ini(text: &str) -> PackIniRaw<&str> {
    parse_pack_ini_with::<_, true>(text, |value| value)
}

#[cfg(any(test, feature = "profile"))]
fn parse_pack_ini_owned(text: &str) -> PackIniRaw<String> {
    parse_pack_ini_with::<_, false>(text, str::to_string)
}

#[cfg(any(test, feature = "profile"))]
fn parse_pack_ini_sequential(text: &str) -> PackIniRaw<&str> {
    parse_pack_ini_with::<_, false>(text, |value| value)
}

#[cfg(feature = "profile")]
fn pack_ini_len<T: AsRef<str>>(raw: &PackIniRaw<T>) -> usize {
    raw.version.as_ref().len()
        + raw.display_title.as_ref().len()
        + raw.sort_title.as_ref().len()
        + raw.translit_title.as_ref().len()
        + raw.series.as_ref().len()
        + raw.banner.as_ref().len()
        + raw.background.as_ref().len()
        + raw.sync_offset.as_ref().len()
        + raw.year.as_ref().len()
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_parse_pack_ini(text: &str, owned: bool) -> usize {
    if owned {
        let raw = parse_pack_ini_owned(text);
        let total = pack_ini_len(&raw);
        std::hint::black_box(raw);
        total
    } else {
        let raw = parse_pack_ini(text);
        let total = pack_ini_len(&raw);
        std::hint::black_box(raw);
        total
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_parse_pack_ini_dispatch(text: &str, sequential: bool) -> usize {
    let raw = if sequential {
        parse_pack_ini_sequential(text)
    } else {
        parse_pack_ini(text)
    };
    let total = pack_ini_len(&raw);
    std::hint::black_box(raw);
    total
}

fn pick_pack_parent_img(pack_dir: &Path, group_name: &str) -> Option<PathBuf> {
    let parent = pack_dir.parent()?;
    let entries = fs::read_dir(parent).ok()?;
    let mut first = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_path = Path::new(&name);
        if assets::is_mac_resource_fork(name_path)
            || !name_path
                .file_stem()
                .is_some_and(|stem| assets::name_eq_ci(stem, group_name))
        {
            continue;
        }
        let Some(rank) = name_path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(assets::img_rank)
        else {
            continue;
        };
        if !assets::entry_is_file(&entry) {
            continue;
        }
        if rank == 0 {
            return Some(parent.join(name));
        }
        if first
            .as_ref()
            .is_none_or(|(current_rank, _)| rank < *current_rank)
        {
            first = Some((rank, name));
        }
    }
    first.map(|(_, name)| parent.join(name))
}

#[cfg(feature = "profile")]
fn pick_pack_parent_img_legacy(pack_dir: &Path, group_name: &str) -> Option<PathBuf> {
    let parent = pack_dir.parent()?;
    let entries = fs::read_dir(parent).ok()?;
    let mut first = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("._") {
            continue;
        }
        let path = entry.path();
        if !path.is_file()
            || !path
                .file_stem()
                .is_some_and(|stem| assets::name_eq_ci(stem, group_name))
        {
            continue;
        }
        let Some(rank) = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(assets::img_rank)
        else {
            continue;
        };
        if rank == 0 {
            return Some(path);
        }
        if first
            .as_ref()
            .is_none_or(|(current_rank, _)| rank < *current_rank)
        {
            first = Some((rank, path));
        }
    }
    first.map(|(_, path)| path)
}

#[cfg(feature = "profile")]
pub(crate) fn profile_pick_pack_parent_img(
    pack_dir: &Path,
    group_name: &str,
    legacy: bool,
) -> Option<PathBuf> {
    if legacy {
        pick_pack_parent_img_legacy(pack_dir, group_name)
    } else {
        pick_pack_parent_img(pack_dir, group_name)
    }
}

fn pick_first_img(dir: &Path, mut matches: impl FnMut(&OsStr) -> bool) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut first = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_path = Path::new(&name);
        if assets::is_mac_resource_fork(name_path) {
            continue;
        }
        let Some(ext) = name_path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if assets::img_rank(ext).is_none() || !assets::entry_is_file(&entry) {
            continue;
        }
        if matches(&name) {
            keep_first_name(&mut first, &name);
        }
    }
    first.map(|name| dir.join(name))
}

#[cfg(any(test, feature = "profile"))]
fn pick_first_img_full_paths(
    dir: &Path,
    mut matches: impl FnMut(&Path) -> bool,
) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut first = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if assets::is_mac_resource_fork(&path) || !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if assets::img_rank(ext).is_none() {
            continue;
        }
        if matches(&path) {
            keep_first_path(&mut first, path);
        }
    }
    first
}

#[cfg(any(test, feature = "profile"))]
fn pick_pack_dir_img(pack_dir: &Path) -> Option<PathBuf> {
    pick_first_img_full_paths(pack_dir, |_| true)
}

#[cfg(any(test, feature = "profile"))]
fn pick_ini_img_legacy(pack_dir: &Path, hint: &str) -> Option<PathBuf> {
    let hint = hint.trim();
    if hint.is_empty() {
        return None;
    }
    let hint = assets::to_slash_legacy(hint);
    let (subdir, mask) = hint.rsplit_once('/').unwrap_or(("", hint.as_str()));
    let dir = if subdir.is_empty() {
        pack_dir.to_path_buf()
    } else {
        assets::is_dir_ci(pack_dir, subdir).unwrap_or_else(|| pack_dir.join(subdir))
    };
    pick_first_img_full_paths(&dir, |path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| assets::match_mask_ci(name, mask))
    })
}

fn normalized_img_hint(hint: &str) -> Option<std::borrow::Cow<'_, str>> {
    let hint = hint.trim();
    (!hint.is_empty()).then(|| assets::to_slash(hint))
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[must_use]
pub fn profile_normalized_img_hint(hint: &str, legacy: bool) -> Option<std::borrow::Cow<'_, str>> {
    if !legacy {
        return normalized_img_hint(hint);
    }
    let hint = hint.trim();
    (!hint.is_empty()).then(|| std::borrow::Cow::Owned(assets::to_slash_legacy(hint)))
}

fn split_img_hint(hint: &str) -> (&str, &str) {
    hint.rsplit_once('/').unwrap_or(("", hint))
}

fn root_img_mask(hint: Option<&str>) -> Option<&str> {
    let (subdir, mask) = split_img_hint(hint?);
    subdir.is_empty().then_some(mask)
}

fn pick_ini_img(
    pack_dir: &Path,
    hint: Option<&str>,
    root_match: Option<PathBuf>,
) -> Option<PathBuf> {
    let hint = hint?;
    let (subdir, mask) = split_img_hint(hint);
    if subdir.is_empty() {
        return root_match;
    }
    let dir = assets::is_dir_ci(pack_dir, subdir).unwrap_or_else(|| pack_dir.join(subdir));
    pick_first_img(&dir, |name| {
        name.to_str()
            .is_some_and(|name| assets::match_mask_ci(name, mask))
    })
}

#[cfg(feature = "profile")]
pub(crate) fn profile_pick_subdir_img(
    pack_dir: &Path,
    hint: &str,
    legacy: bool,
) -> Option<PathBuf> {
    if legacy {
        pick_ini_img_legacy(pack_dir, hint)
    } else {
        let hint = normalized_img_hint(hint)?;
        pick_ini_img(pack_dir, Some(&hint), None)
    }
}

fn simfile_ext(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("ssc") {
        Some("ssc")
    } else if extension.eq_ignore_ascii_case("sm") {
        Some("sm")
    } else {
        None
    }
}

fn simfile_names(dir: &Path) -> io::Result<impl Iterator<Item = (&'static str, OsString)>> {
    Ok(fs::read_dir(dir)?.filter_map(|entry| {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name_path = Path::new(&name);
        if assets::is_mac_resource_fork(name_path) {
            return None;
        }
        let extension = simfile_ext(name_path)?;
        assets::entry_is_file(&entry).then_some((extension, name))
    }))
}

#[cfg(any(test, feature = "profile"))]
fn simfile_paths(dir: &Path) -> io::Result<impl Iterator<Item = (&'static str, PathBuf)> + '_> {
    Ok(simfile_names(dir)?.map(move |(extension, name)| (extension, dir.join(name))))
}

#[cfg(feature = "profile")]
fn simfile_paths_full_paths(
    dir: &Path,
) -> io::Result<impl Iterator<Item = (&'static str, PathBuf)>> {
    Ok(fs::read_dir(dir)?.filter_map(|entry| {
        let Ok(entry) = entry else {
            return None;
        };
        let path = entry.path();
        if assets::is_mac_resource_fork(&path) {
            return None;
        }
        if !path.is_file() {
            return None;
        }
        simfile_ext(&path).map(|extension| (extension, path))
    }))
}

fn song_scan(dir: &Path, simfile: PathBuf, extension: &'static str) -> SongScan {
    SongScan {
        dir: dir.to_path_buf(),
        simfile,
        extension,
    }
}

fn first_song_scan(
    dir: &Path,
    first_sm: Option<OsString>,
    first_ssc: Option<OsString>,
) -> Option<SongScan> {
    first_ssc
        .map(|name| song_scan(dir, dir.join(name), "ssc"))
        .or_else(|| first_sm.map(|name| song_scan(dir, dir.join(name), "sm")))
}

fn scan_song_dir_first(dir: &Path) -> Result<Option<SongScan>, ScanError> {
    let mut first_sm: Option<OsString> = None;
    let mut first_ssc: Option<OsString> = None;
    for (extension, name) in simfile_names(dir)? {
        let first = if extension == "ssc" {
            &mut first_ssc
        } else {
            &mut first_sm
        };
        keep_first_owned(first, name);
    }

    Ok(first_song_scan(dir, first_sm, first_ssc))
}

#[cfg(feature = "profile")]
fn scan_song_dir_first_full_paths(dir: &Path) -> Result<Option<SongScan>, ScanError> {
    let mut first_sm = None;
    let mut first_ssc = None;
    for (extension, path) in simfile_paths_full_paths(dir)? {
        if extension == "ssc" {
            keep_first_path(&mut first_ssc, path);
        } else {
            keep_first_path(&mut first_sm, path);
        }
    }

    Ok(first_ssc
        .map(|path| song_scan(dir, path, "ssc"))
        .or_else(|| first_sm.map(|path| song_scan(dir, path, "sm"))))
}

#[cfg(any(test, feature = "profile"))]
fn scan_song_dir_duplicates_joined(
    dir: &Path,
    paths: impl Iterator<Item = (&'static str, PathBuf)>,
) -> Result<Option<SongScan>, ScanError> {
    let mut sms = Vec::new();
    let mut sscs = Vec::new();
    for (extension, path) in paths {
        if extension == "ssc" {
            sscs.push(path);
        } else {
            sms.push(path);
        }
    }
    select_duplicate_scan(dir, sms, sscs)
}

fn scan_song_dir_duplicates(
    dir: &Path,
    names: impl Iterator<Item = (&'static str, OsString)>,
) -> Result<Option<SongScan>, ScanError> {
    let mut sms = SimfileNames::default();
    let mut sscs = SimfileNames::default();
    for (extension, name) in names {
        if extension == "ssc" {
            push_simfile_name(&mut sscs, name);
        } else {
            push_simfile_name(&mut sms, name);
        }
    }
    select_duplicate_names(dir, sms, sscs)
}

#[derive(Default)]
enum SimfileNames {
    #[default]
    None,
    One(OsString),
    Many(Vec<OsString>),
}

fn push_simfile_name(names: &mut SimfileNames, name: OsString) {
    match names {
        SimfileNames::None => *names = SimfileNames::One(name),
        SimfileNames::One(_) => {
            let SimfileNames::One(first) = std::mem::take(names) else {
                unreachable!("matched the single-name state")
            };
            *names = SimfileNames::Many(vec![first, name]);
        }
        SimfileNames::Many(names) => names.push(name),
    }
}

#[cfg(any(test, feature = "profile"))]
fn select_duplicate_scan(
    dir: &Path,
    mut sms: Vec<PathBuf>,
    mut sscs: Vec<PathBuf>,
) -> Result<Option<SongScan>, ScanError> {
    sort_paths_ci(&mut sms);
    sort_paths_ci(&mut sscs);

    if let Some(simfile) = sscs.pop() {
        if !sscs.is_empty() {
            sscs.push(simfile);
            return Err(ScanError::DuplicateSimfile {
                ext: "ssc",
                paths: sscs,
            });
        }
        return Ok(Some(song_scan(dir, simfile, "ssc")));
    }

    let Some(simfile) = sms.pop() else {
        return Ok(None);
    };
    if !sms.is_empty() {
        sms.push(simfile);
        return Err(ScanError::DuplicateSimfile {
            ext: "sm",
            paths: sms,
        });
    }
    Ok(Some(song_scan(dir, simfile, "sm")))
}

fn select_duplicate_names(
    dir: &Path,
    sms: SimfileNames,
    sscs: SimfileNames,
) -> Result<Option<SongScan>, ScanError> {
    match sscs {
        SimfileNames::One(name) => return Ok(Some(song_scan(dir, dir.join(name), "ssc"))),
        SimfileNames::Many(names) => return Err(duplicate_name_error(dir, "ssc", names)),
        SimfileNames::None => {}
    }

    match sms {
        SimfileNames::None => Ok(None),
        SimfileNames::One(name) => Ok(Some(song_scan(dir, dir.join(name), "sm"))),
        SimfileNames::Many(names) => Err(duplicate_name_error(dir, "sm", names)),
    }
}

#[cold]
fn duplicate_name_error(dir: &Path, ext: &'static str, mut names: Vec<OsString>) -> ScanError {
    sort_names_ci(&mut names);
    ScanError::DuplicateSimfile {
        ext,
        paths: names.into_iter().map(|name| dir.join(name)).collect(),
    }
}

#[cfg(feature = "profile")]
fn scan_song_dir_growing_names(
    dir: &Path,
    names: impl Iterator<Item = (&'static str, OsString)>,
) -> Result<Option<SongScan>, ScanError> {
    let mut sms = Vec::new();
    let mut sscs = Vec::new();
    for (extension, name) in names {
        if extension == "ssc" {
            sscs.push(name);
        } else {
            sms.push(name);
        }
    }
    select_growing_names(dir, sms, sscs)
}

#[cfg(feature = "profile")]
fn select_growing_names(
    dir: &Path,
    mut sms: Vec<OsString>,
    mut sscs: Vec<OsString>,
) -> Result<Option<SongScan>, ScanError> {
    sort_names_ci(&mut sms);
    sort_names_ci(&mut sscs);
    if let Some(name) = sscs.pop() {
        if !sscs.is_empty() {
            sscs.push(name);
            return Err(ScanError::DuplicateSimfile {
                ext: "ssc",
                paths: sscs.into_iter().map(|name| dir.join(name)).collect(),
            });
        }
        return Ok(Some(song_scan(dir, dir.join(name), "ssc")));
    }
    let Some(name) = sms.pop() else {
        return Ok(None);
    };
    if !sms.is_empty() {
        sms.push(name);
        return Err(ScanError::DuplicateSimfile {
            ext: "sm",
            paths: sms.into_iter().map(|name| dir.join(name)).collect(),
        });
    }
    Ok(Some(song_scan(dir, dir.join(name), "sm")))
}

fn scan_tree_dir(dir: &Path, opt: ScanOpt) -> Result<(Option<SongScan>, Vec<OsString>), ScanError> {
    let mut first_sm = None;
    let mut first_ssc = None;
    let mut sms = SimfileNames::default();
    let mut sscs = SimfileNames::default();
    let mut subdirs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name();
        let name_path = Path::new(&name);
        if assets::is_mac_resource_fork(name_path) {
            continue;
        }
        if assets::entry_is_dir(&entry) {
            subdirs.push(name);
            continue;
        }
        let Some(extension) = simfile_ext(name_path) else {
            continue;
        };
        if !assets::entry_is_file(&entry) {
            continue;
        }
        match opt.dup {
            DupPolicy::First => {
                let first = if extension == "ssc" {
                    &mut first_ssc
                } else {
                    &mut first_sm
                };
                keep_first_owned(first, name);
            }
            DupPolicy::Error => {
                let names = if extension == "ssc" {
                    &mut sscs
                } else {
                    &mut sms
                };
                push_simfile_name(names, name);
            }
        }
    }
    let song = match opt.dup {
        DupPolicy::First => first_song_scan(dir, first_sm, first_ssc),
        DupPolicy::Error => select_duplicate_names(dir, sms, sscs)?,
    };
    Ok((song, subdirs))
}

pub fn scan_song_dir(dir: &Path, opt: ScanOpt) -> Result<Option<SongScan>, ScanError> {
    if assets::is_mac_resource_fork(dir) {
        return Ok(None);
    }

    match opt.dup {
        DupPolicy::First => scan_song_dir_first(dir),
        DupPolicy::Error => scan_song_dir_duplicates(dir, simfile_names(dir)?),
    }
}

#[cfg(feature = "profile")]
pub(crate) fn profile_scan_song_dir_full_paths(
    dir: &Path,
    opt: ScanOpt,
) -> Result<Option<SongScan>, ScanError> {
    if assets::is_mac_resource_fork(dir) {
        return Ok(None);
    }

    match opt.dup {
        DupPolicy::First => scan_song_dir_first_full_paths(dir),
        DupPolicy::Error => scan_song_dir_duplicates_joined(dir, simfile_paths_full_paths(dir)?),
    }
}

#[cfg(any(test, feature = "profile"))]
pub(crate) fn profile_scan_song_dir_joined_paths(
    dir: &Path,
    opt: ScanOpt,
) -> Result<Option<SongScan>, ScanError> {
    if assets::is_mac_resource_fork(dir) {
        return Ok(None);
    }

    match opt.dup {
        DupPolicy::First => scan_song_dir_first(dir),
        DupPolicy::Error => scan_song_dir_duplicates_joined(dir, simfile_paths(dir)?),
    }
}

#[cfg(feature = "profile")]
pub(crate) fn profile_scan_song_dir_growing_names(
    dir: &Path,
    opt: ScanOpt,
) -> Result<Option<SongScan>, ScanError> {
    if assets::is_mac_resource_fork(dir) {
        return Ok(None);
    }

    match opt.dup {
        DupPolicy::First => scan_song_dir_first(dir),
        DupPolicy::Error => scan_song_dir_growing_names(dir, simfile_names(dir)?),
    }
}

struct PackRoot {
    banner: Option<PathBuf>,
    background: Option<PathBuf>,
    songs: Vec<SongScan>,
}

struct RootEntries {
    first_img: Option<PathBuf>,
    banner: Option<PathBuf>,
    background: Option<PathBuf>,
    songs: Vec<SongScan>,
}

#[cfg(feature = "profile")]
fn scan_root_entries_full_paths(
    dir: &Path,
    opt: ScanOpt,
    banner_mask: Option<&str>,
    background_mask: Option<&str>,
) -> Result<RootEntries, ScanError> {
    let mut first_img = None;
    let mut banner = None;
    let mut background = None;
    let mut songs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if assets::is_mac_resource_fork(&path) {
            continue;
        }
        if path.is_dir() {
            if let Some(song) = scan_song_dir(&path, opt)? {
                songs.push(song);
            }
            continue;
        }
        if !path.is_file()
            || path
                .extension()
                .and_then(|value| value.to_str())
                .and_then(assets::img_rank)
                .is_none()
        {
            continue;
        }
        keep_first_ref(&mut first_img, &path);
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if banner_mask.is_some_and(|mask| assets::match_mask_ci(name, mask)) {
            keep_first_ref(&mut banner, &path);
        }
        if background_mask.is_some_and(|mask| assets::match_mask_ci(name, mask)) {
            keep_first_ref(&mut background, &path);
        }
    }
    Ok(RootEntries {
        first_img,
        banner,
        background,
        songs,
    })
}

/// Takes one worker-owned snapshot of a pack root and releases it when the
/// scan ends. It has no shared state, eviction, or gameplay miss path; memory
/// is bounded by the returned songs plus three selected image paths, and work
/// is one root entry visit plus each candidate song's normal scan.
fn scan_root_entries(
    dir: &Path,
    opt: ScanOpt,
    banner_mask: Option<&str>,
    background_mask: Option<&str>,
) -> Result<RootEntries, ScanError> {
    let mut first_img = None;
    let mut banner = None;
    let mut background = None;
    let mut songs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name();
        let name_path = Path::new(&name);
        if assets::is_mac_resource_fork(name_path) {
            continue;
        }
        if assets::entry_is_dir(&entry) {
            let path = dir.join(&name);
            if let Some(song) = scan_song_dir(&path, opt)? {
                songs.push(song);
            }
            continue;
        }
        if name_path
            .extension()
            .and_then(|value| value.to_str())
            .and_then(assets::img_rank)
            .is_none()
            || !assets::entry_is_file(&entry)
        {
            continue;
        }
        keep_first_name(&mut first_img, &name);
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if banner_mask.is_some_and(|mask| assets::match_mask_ci(name_str, mask)) {
            keep_first_name(&mut banner, &name);
        }
        if background_mask.is_some_and(|mask| assets::match_mask_ci(name_str, mask)) {
            keep_first_name(&mut background, &name);
        }
    }
    Ok(RootEntries {
        first_img: first_img.map(|name| dir.join(name)),
        banner: banner.map(|name| dir.join(name)),
        background: background.map(|name| dir.join(name)),
        songs,
    })
}

fn finish_pack_root(
    dir: &Path,
    banner_hint: Option<&str>,
    background_hint: Option<&str>,
    root: RootEntries,
) -> PackRoot {
    PackRoot {
        banner: pick_ini_img(dir, banner_hint, root.banner).or(root.first_img),
        background: pick_ini_img(dir, background_hint, root.background),
        songs: root.songs,
    }
}

fn scan_pack_root(
    dir: &Path,
    opt: ScanOpt,
    banner: &str,
    background: &str,
) -> Result<PackRoot, ScanError> {
    let banner_hint = normalized_img_hint(banner);
    let background_hint = normalized_img_hint(background);
    let root = scan_root_entries(
        dir,
        opt,
        root_img_mask(banner_hint.as_deref()),
        root_img_mask(background_hint.as_deref()),
    )?;
    Ok(finish_pack_root(
        dir,
        banner_hint.as_deref(),
        background_hint.as_deref(),
        root,
    ))
}

#[cfg(feature = "profile")]
fn scan_pack_root_full_paths(
    dir: &Path,
    opt: ScanOpt,
    banner: &str,
    background: &str,
) -> Result<PackRoot, ScanError> {
    let banner_hint = normalized_img_hint(banner);
    let background_hint = normalized_img_hint(background);
    let root = scan_root_entries_full_paths(
        dir,
        opt,
        root_img_mask(banner_hint.as_deref()),
        root_img_mask(background_hint.as_deref()),
    )?;
    Ok(finish_pack_root(
        dir,
        banner_hint.as_deref(),
        background_hint.as_deref(),
        root,
    ))
}

#[cfg(any(test, feature = "profile"))]
fn scan_pack_root_legacy(
    dir: &Path,
    opt: ScanOpt,
    banner: &str,
    background: &str,
) -> Result<PackRoot, ScanError> {
    let ini_banner = pick_ini_img_legacy(dir, banner);
    let background = pick_ini_img_legacy(dir, background);
    let banner = ini_banner.or_else(|| pick_pack_dir_img(dir));
    let mut songs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if assets::is_mac_resource_fork(&path) || !path.is_dir() {
            continue;
        }
        if let Some(song) = scan_song_dir(&path, opt)? {
            songs.push(song);
        }
    }
    Ok(PackRoot {
        banner,
        background,
        songs,
    })
}

#[cfg(feature = "profile")]
pub(crate) type ProfilePackRoot = (Option<PathBuf>, Option<PathBuf>, Vec<SongScan>);

#[cfg(feature = "profile")]
pub(crate) fn profile_pack_root(
    dir: &Path,
    opt: ScanOpt,
    banner: &str,
    background: &str,
    legacy: bool,
) -> Result<ProfilePackRoot, ScanError> {
    let root = if legacy {
        scan_pack_root_legacy(dir, opt, banner, background)?
    } else {
        scan_pack_root(dir, opt, banner, background)?
    };
    Ok((root.banner, root.background, root.songs))
}

#[cfg(feature = "profile")]
pub(crate) fn profile_pack_root_full_paths(
    dir: &Path,
    opt: ScanOpt,
    banner: &str,
    background: &str,
) -> Result<ProfilePackRoot, ScanError> {
    let root = scan_pack_root_full_paths(dir, opt, banner, background)?;
    Ok((root.banner, root.background, root.songs))
}

pub fn scan_pack_dir(dir: &Path, opt: ScanOpt) -> Result<Option<PackScan>, ScanError> {
    if assets::is_mac_resource_fork(dir) || !dir.is_dir() {
        return Ok(None);
    }
    scan_pack_dir_valid(dir, opt)
}

fn scan_pack_dir_valid(dir: &Path, opt: ScanOpt) -> Result<Option<PackScan>, ScanError> {
    let Some(group_name) = dir.file_name().and_then(|s| s.to_str()) else {
        return Err(ScanError::InvalidUtf8Path);
    };

    let ini_text = fs::read_to_string(pack_ini_path(dir)).unwrap_or_default();
    let ini = parse_pack_ini(&ini_text);
    let has_pack_ini = !ini.version.trim().is_empty();
    let PackIniRaw {
        version,
        display_title,
        sort_title,
        translit_title,
        series,
        banner,
        background,
        sync_offset,
        year,
    } = ini;
    let display_title = if has_pack_ini && !display_title.is_empty() {
        display_title.to_string()
    } else {
        group_name.to_string()
    };
    let sort_title = if has_pack_ini && !sort_title.is_empty() {
        sort_title.to_string()
    } else {
        group_name.to_string()
    };
    let translit_title = if has_pack_ini && !translit_title.is_empty() {
        translit_title.to_string()
    } else {
        display_title.clone()
    };
    let series = if has_pack_ini {
        series.to_string()
    } else {
        String::new()
    };
    let year = if has_pack_ini {
        year.parse().unwrap_or(0)
    } else {
        0
    };
    let version = if has_pack_ini {
        version.parse().unwrap_or(0)
    } else {
        0
    };
    let sync_pref = if has_pack_ini {
        parse_sync_pref(&sync_offset)
    } else {
        SyncPref::Default
    };

    let (banner, background) = if has_pack_ini {
        (banner, background)
    } else {
        ("", "")
    };
    let root = scan_pack_root(dir, opt, banner, background)?;
    let auto_background = if root.background.is_none() {
        assets::resolve_song_assets(dir, "", "").1
    } else {
        None
    };

    // ITGmania group banners are simpler than song assets: if the pack root
    // contains any image, the first one is treated as the group banner.
    let banner_path = root
        .banner
        .or_else(|| pick_pack_parent_img(dir, group_name));
    let background_path = root.background.or(auto_background);
    let songs = root.songs;

    if songs.is_empty() {
        return Ok(None);
    }

    Ok(Some(PackScan {
        dir: dir.to_path_buf(),
        group_name: group_name.to_string(),
        display_title,
        sort_title,
        translit_title,
        series,
        year,
        version,
        has_pack_ini,
        sync_pref,
        banner_path,
        background_path,
        songs,
    }))
}

pub fn scan_songs_dir(dir: &Path, opt: ScanOpt) -> Result<Vec<PackScan>, ScanError> {
    let mut packs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name();
        if assets::is_mac_resource_fork(Path::new(&name)) || !assets::entry_is_dir(&entry) {
            continue;
        }
        if let Some(pack) = scan_pack_dir_valid(&dir.join(name), opt)? {
            packs.push(pack);
        }
    }
    sort_packs_ci(&mut packs);
    Ok(packs)
}

#[cfg(feature = "profile")]
pub(crate) fn profile_scan_songs_dir_legacy(
    dir: &Path,
    opt: ScanOpt,
) -> Result<Vec<PackScan>, ScanError> {
    let mut packs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if assets::is_mac_resource_fork(&path) {
            continue;
        }
        if let Some(pack) = scan_pack_dir(&path, opt)? {
            packs.push(pack);
        }
    }
    packs.sort_by_cached_key(|pack| pack.group_name.to_ascii_lowercase());
    Ok(packs)
}

#[must_use]
pub fn find_simfiles(root: &Path, opt: ScanOpt) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if assets::is_mac_resource_fork(&dir) {
            continue;
        }
        let Ok((song, mut subdirs)) = scan_tree_dir(&dir, opt) else {
            continue;
        };
        if let Some(song) = song {
            out.push(song.simfile);
            continue;
        }

        subdirs.sort_by(|left, right| assets::cmp_os_ci(left, right));
        for name in subdirs.into_iter().rev() {
            stack.push(dir.join(name));
        }
    }

    out
}

#[cfg(feature = "profile")]
pub(crate) fn profile_find_simfiles_legacy(root: &Path, opt: ScanOpt) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if assets::is_mac_resource_fork(&dir) {
            continue;
        }
        let Ok(song) = scan_song_dir(&dir, opt) else {
            continue;
        };
        if let Some(song) = song {
            out.push(song.simfile);
            continue;
        }

        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut subdirs: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| !assets::is_mac_resource_fork(path) && path.is_dir())
            .collect();
        sort_paths_ci(&mut subdirs);
        for subdir in subdirs.into_iter().rev() {
            stack.push(subdir);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{
        DupPolicy, PackIniKey, PackIniRaw, ScanError, ScanOpt, find_simfiles, pack_ini_key,
        pack_ini_key_sequential, parse_pack_ini, parse_pack_ini_owned, parse_pack_ini_sequential,
        pick_pack_parent_img, profile_scan_song_dir_joined_paths, scan_pack_dir, scan_pack_root,
        scan_pack_root_legacy, scan_song_dir, scan_songs_dir, sort_paths_ci,
    };
    use crate::assets;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rssp-pack-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn compact_path_sort_matches_cached_keys() {
        let mut expected = vec![
            PathBuf::from("Songs/Alpha.ssc"),
            PathBuf::from("Songs/alpha.SM"),
            PathBuf::from("Songs/BETA.ssc"),
            PathBuf::from("Songs/beta.ssc"),
            PathBuf::from("Songs/éclair.ssc"),
            PathBuf::from("Songs/Äther.sm"),
        ];
        let mut actual = expected.clone();
        expected.sort_by_cached_key(|path| assets::lc_name(path));
        sort_paths_ci(&mut actual);
        assert_eq!(actual, expected);
    }

    fn write_file(path: &Path) {
        fs::write(path, b"").unwrap();
    }

    #[test]
    fn scan_song_dir_ignores_mac_resource_fork_simfiles() {
        let root = test_dir("resource-fork-simfiles");
        write_file(&root.join("._chart.ssc"));
        write_file(&root.join("chart.ssc"));
        write_file(&root.join("fallback.sm"));

        let scan = scan_song_dir(&root, ScanOpt::default()).unwrap().unwrap();
        assert_eq!(scan.simfile, root.join("chart.ssc"));
        assert_eq!(scan.extension, "ssc");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_song_dir_returns_none_for_only_resource_forks() {
        let root = test_dir("only-resource-forks");
        write_file(&root.join("._chart.ssc"));
        write_file(&root.join("._chart.sm"));

        assert!(scan_song_dir(&root, ScanOpt::default()).unwrap().is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_song_dir_uses_case_insensitive_first_ssc() {
        let root = test_dir("case-insensitive-first");
        for name in [
            "Aardvark.sm",
            "zebra.SM",
            "Chart.ssc",
            "backup.SSC",
            "alpha.sSc",
        ] {
            write_file(&root.join(name));
        }

        let scan = scan_song_dir(&root, ScanOpt::default()).unwrap().unwrap();
        assert_eq!(scan.simfile, root.join("alpha.sSc"));
        assert_eq!(scan.extension, "ssc");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_error_paths_remain_case_insensitively_sorted() {
        let root = test_dir("sorted-duplicate-error");
        for name in ["Zulu.ssc", "beta.SSC", "Alpha.sSc", "fallback.sm"] {
            write_file(&root.join(name));
        }

        let opt = ScanOpt {
            dup: DupPolicy::Error,
        };
        let previous = profile_scan_song_dir_joined_paths(&root, opt).unwrap_err();
        let error = scan_song_dir(&root, opt).unwrap_err();
        let ScanError::DuplicateSimfile {
            ext: previous_ext,
            paths: previous_paths,
        } = previous
        else {
            panic!("expected a prior duplicate simfile error");
        };
        let ScanError::DuplicateSimfile { ext, paths } = error else {
            panic!("expected a duplicate simfile error");
        };
        assert_eq!(ext, previous_ext);
        assert_eq!(paths, previous_paths);
        assert_eq!(ext, "ssc");
        assert_eq!(
            paths,
            ["Alpha.sSc", "beta.SSC", "Zulu.ssc"].map(|name| root.join(name))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pack_ini_parsing_preserves_group_and_key_behavior() {
        let input = "\
                Version=ignored\n\
                [Other]\n\
                Version=also ignored\n\
                [gRoUp]\n\
                VERSION = 2\n\
                displayTITLE = First title\n\
                UnknownKey = ignored value\n\
                DisplayTitle = Final title\n\
                SORTTITLE = Sort me\n\
                translittitle = Translit\n\
                Series = Series name\n\
                Banner = banner*.png\n\
                BACKGROUND = background.jpg\n\
                SyncOffset = ITG\n\
                Year = 2026\n\
                [Other]\n\
                Year=1900\n";
        let parsed = parse_pack_ini(input);
        let sequential = parse_pack_ini_sequential(input);
        let owned = parse_pack_ini_owned(input);

        assert_eq!(
            parsed, sequential,
            "indexed Pack.ini dispatch changed fields"
        );
        assert_eq!(
            [
                parsed.version,
                parsed.display_title,
                parsed.sort_title,
                parsed.translit_title,
                parsed.series,
                parsed.banner,
                parsed.background,
                parsed.sync_offset,
                parsed.year,
            ],
            [
                owned.version.as_str(),
                owned.display_title.as_str(),
                owned.sort_title.as_str(),
                owned.translit_title.as_str(),
                owned.series.as_str(),
                owned.banner.as_str(),
                owned.background.as_str(),
                owned.sync_offset.as_str(),
                owned.year.as_str(),
            ],
            "borrowed Pack.ini parsing changed owned parser behavior"
        );

        assert_eq!(
            parsed,
            PackIniRaw {
                version: "2",
                display_title: "Final title",
                sort_title: "Sort me",
                translit_title: "Translit",
                series: "Series name",
                banner: "banner*.png",
                background: "background.jpg",
                sync_offset: "ITG",
                year: "2026",
            }
        );
    }

    #[test]
    fn indexed_pack_ini_keys_match_sequential_dispatch() {
        for key in [
            "VERSION",
            "displaytitle",
            "SortTitle",
            "TRANSLITTITLE",
            "series",
            "Banner",
            "BACKGROUND",
            "syncOffset",
            "Year",
            "UnknownKey",
            "",
        ] {
            assert_eq!(
                pack_ini_key(key),
                pack_ini_key_sequential(key),
                "Pack.ini key dispatch changed for {key:?}"
            );
        }
        assert_eq!(pack_ini_key("VERSION"), PackIniKey::Version);
    }

    #[test]
    fn pack_metadata_defaults_survive_owned_field_transfer() {
        let root = test_dir("pack-metadata-defaults");
        fs::write(
            root.join("Pack.ini"),
            b"[Group]\nVersion=7\nDisplayTitle=Display\nSeries=Series\nYear=2026\n",
        )
        .unwrap();
        let song = root.join("Song");
        fs::create_dir(&song).unwrap();
        write_file(&song.join("chart.ssc"));

        let scan = scan_pack_dir(&root, ScanOpt::default()).unwrap().unwrap();
        let group_name = root.file_name().unwrap().to_str().unwrap();
        assert_eq!(scan.display_title, "Display");
        assert_eq!(scan.sort_title, group_name);
        assert_eq!(scan.translit_title, "Display");
        assert_eq!(scan.series, "Series");
        assert_eq!(scan.version, 7);
        assert_eq!(scan.year, 2026);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parent_image_selection_preserves_extension_priority_in_one_scan() {
        let root = test_dir("parent-image");
        let pack = root.join("Performance.Pack");
        fs::create_dir(&pack).unwrap();
        let jpg = root.join("performance.pack.JPG");
        let png = root.join("PERFORMANCE.PACK.PNG");
        write_file(&root.join("Performance.Pack.GIF"));
        write_file(&root.join("._Performance.Pack.png"));
        write_file(&jpg);
        write_file(&png);

        assert_eq!(
            pick_pack_parent_img(&pack, "Performance.Pack"),
            Some(png.clone())
        );
        fs::remove_file(png).unwrap();
        assert_eq!(pick_pack_parent_img(&pack, "Performance.Pack"), Some(jpg));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pack_image_selection_uses_case_insensitive_first_matches() {
        let root = test_dir("first-pack-images");
        fs::write(
            root.join("Pack.ini"),
            b"[Group]\nVersion=1\nBanner=missing*.png\nBackground=back*.jpg\n",
        )
        .unwrap();
        for name in ["Zeta.png", "alpha.png", "BackB.jpg", "backA.jpg"] {
            write_file(&root.join(name));
        }
        let song = root.join("Song");
        fs::create_dir(&song).unwrap();
        write_file(&song.join("chart.ssc"));

        let scan = scan_pack_dir(&root, ScanOpt::default()).unwrap().unwrap();
        assert_eq!(scan.banner_path, Some(root.join("alpha.png")));
        assert_eq!(scan.background_path, Some(root.join("backA.jpg")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn one_pass_pack_root_matches_repeated_scans() {
        let root = test_dir("one-pass-root-parity");
        for name in ["Zeta.png", "alpha.png", "backB.jpg", "backA.jpg"] {
            write_file(&root.join(name));
        }
        write_file(&root.join("._ignored.png"));
        let images = root.join("Images");
        fs::create_dir(&images).unwrap();
        write_file(&images.join("BannerZ.PNG"));
        write_file(&images.join("bannerA.png"));
        for name in ["SongB", "SongA"] {
            let song = root.join(name);
            fs::create_dir(&song).unwrap();
            write_file(&song.join("chart.ssc"));
            write_file(&song.join("fallback.sm"));
        }

        for (banner, background, expected_banner, expected_background) in [
            (
                "Images/banner*.png",
                "back*.jpg",
                images.join("bannerA.png"),
                Some(root.join("backA.jpg")),
            ),
            ("missing*.png", "missing*.jpg", root.join("alpha.png"), None),
        ] {
            let legacy =
                scan_pack_root_legacy(&root, ScanOpt::default(), banner, background).unwrap();
            let one_pass = scan_pack_root(&root, ScanOpt::default(), banner, background).unwrap();
            assert_eq!(one_pass.banner, legacy.banner);
            assert_eq!(one_pass.background, legacy.background);
            assert_eq!(one_pass.banner, Some(expected_banner));
            assert_eq!(one_pass.background, expected_background);
            assert_eq!(one_pass.songs.len(), legacy.songs.len());
            for (actual, expected) in one_pass.songs.iter().zip(&legacy.songs) {
                assert_eq!(actual.dir, expected.dir);
                assert_eq!(actual.simfile, expected.simfile);
                assert_eq!(actual.extension, expected.extension);
            }
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pack_image_masks_match_ascii_case_without_allocated_normalization() {
        for (name, mask, expected) in [
            ("Banner.PNG", "banner.png", true),
            ("Background-Wide.JPG", "back*.jpg", true),
            ("back.jpg", "back*.jpg", true),
            ("back.png", "back*.jpg", false),
            ("prefix-MIDDLE-suffix.PNG", "PRE*middle*FIX.png", true),
            ("prefix-other-suffix.png", "pre*middle*fix.png", false),
            ("é-BANNER.PNG", "é-*.png", true),
        ] {
            assert_eq!(
                assets::match_mask_ci(name, mask),
                expected,
                "{name:?} against {mask:?}"
            );
        }
    }

    #[test]
    fn scan_songs_dir_ignores_resource_fork_pack_and_song_dirs() {
        let root = test_dir("resource-fork-dirs");
        let ignored_pack_song = root.join("._Pack").join("Song");
        fs::create_dir_all(&ignored_pack_song).unwrap();
        write_file(&ignored_pack_song.join("song.ssc"));

        let ignored_song = root.join("Pack").join("._Song");
        fs::create_dir_all(&ignored_song).unwrap();
        write_file(&ignored_song.join("song.ssc"));

        write_file(&root.join("Pack").join("._banner.png"));
        let banner = root.join("Pack").join("banner.png");
        write_file(&banner);

        let song = root.join("Pack").join("Song");
        fs::create_dir_all(&song).unwrap();
        write_file(&song.join("song.ssc"));

        assert!(
            scan_pack_dir(&root.join("._Pack"), ScanOpt::default())
                .unwrap()
                .is_none()
        );

        let packs = scan_songs_dir(&root, ScanOpt::default()).unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].group_name, "Pack");
        assert_eq!(packs[0].banner_path, Some(banner));
        assert_eq!(packs[0].songs.len(), 1);
        assert_eq!(packs[0].songs[0].dir, song);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn find_simfiles_ignores_resource_fork_paths() {
        let root = test_dir("find-simfiles-resource-forks");
        let ignored = root.join("Pack").join("._Ignored");
        fs::create_dir_all(&ignored).unwrap();
        write_file(&ignored.join("song.ssc"));

        let song = root.join("Pack").join("Song");
        fs::create_dir_all(&song).unwrap();
        write_file(&song.join("._song.ssc"));
        write_file(&song.join("song.ssc"));

        let files = find_simfiles(&root, ScanOpt::default());
        assert_eq!(files, vec![song.join("song.ssc")]);

        let _ = fs::remove_dir_all(root);
    }
}
