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

fn sort_paths_ci(paths: &mut [PathBuf]) {
    paths.sort_by_cached_key(|p| assets::lc_name(p));
}

fn should_replace(first: Option<&Path>, candidate: &Path) -> bool {
    first.is_none_or(|current| assets::cmp_name_ci(candidate, current).is_lt())
}

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
struct PackIniRaw {
    version: String,
    display_title: String,
    sort_title: String,
    translit_title: String,
    series: String,
    banner: String,
    background: String,
    sync_offset: String,
    year: String,
}

fn parse_pack_ini(text: &str) -> PackIniRaw {
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
        if key.eq_ignore_ascii_case("version") {
            out.version = value.to_string();
        } else if key.eq_ignore_ascii_case("displaytitle") {
            out.display_title = value.to_string();
        } else if key.eq_ignore_ascii_case("sorttitle") {
            out.sort_title = value.to_string();
        } else if key.eq_ignore_ascii_case("translittitle") {
            out.translit_title = value.to_string();
        } else if key.eq_ignore_ascii_case("series") {
            out.series = value.to_string();
        } else if key.eq_ignore_ascii_case("banner") {
            out.banner = value.to_string();
        } else if key.eq_ignore_ascii_case("background") {
            out.background = value.to_string();
        } else if key.eq_ignore_ascii_case("syncoffset") {
            out.sync_offset = value.to_string();
        } else if key.eq_ignore_ascii_case("year") {
            out.year = value.to_string();
        }
    }

    out
}

fn read_pack_ini(pack_dir: &Path) -> (PackIniRaw, bool) {
    let path = pack_ini_path(pack_dir);
    let Ok(text) = fs::read_to_string(path) else {
        return (PackIniRaw::default(), false);
    };
    let raw = parse_pack_ini(&text);
    if raw.version.trim().is_empty() {
        return (PackIniRaw::default(), false);
    }
    (raw, true)
}

fn pick_pack_parent_img(pack_dir: &Path, group_name: &str) -> Option<PathBuf> {
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

fn pick_first_img(dir: &Path, mut matches: impl FnMut(&Path) -> bool) -> Option<PathBuf> {
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
    pick_first_img(pack_dir, |_| true)
}

#[cfg(any(test, feature = "profile"))]
fn pick_ini_img_legacy(pack_dir: &Path, hint: &str) -> Option<PathBuf> {
    let hint = hint.trim();
    if hint.is_empty() {
        return None;
    }
    let hint = assets::to_slash(hint);
    let (subdir, mask) = hint.rsplit_once('/').unwrap_or(("", hint.as_str()));
    let dir = if subdir.is_empty() {
        pack_dir.to_path_buf()
    } else {
        assets::is_dir_ci(pack_dir, subdir).unwrap_or_else(|| pack_dir.join(subdir))
    };
    pick_first_img(&dir, |path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| assets::match_mask_ci(name, mask))
    })
}

fn normalized_img_hint(hint: &str) -> Option<String> {
    let hint = hint.trim();
    (!hint.is_empty()).then(|| assets::to_slash(hint))
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
    pick_first_img(&dir, |path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| assets::match_mask_ci(name, mask))
    })
}

fn simfile_paths(dir: &Path) -> io::Result<impl Iterator<Item = (&'static str, PathBuf)>> {
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
        let ext = path.extension().and_then(|s| s.to_str())?;
        if ext.eq_ignore_ascii_case("ssc") {
            Some(("ssc", path))
        } else if ext.eq_ignore_ascii_case("sm") {
            Some(("sm", path))
        } else {
            None
        }
    }))
}

fn song_scan(dir: &Path, simfile: PathBuf, extension: &'static str) -> SongScan {
    SongScan {
        dir: dir.to_path_buf(),
        simfile,
        extension,
    }
}

fn scan_song_dir_first(dir: &Path) -> Result<Option<SongScan>, ScanError> {
    let mut first_sm = None;
    let mut first_ssc = None;
    for (extension, path) in simfile_paths(dir)? {
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

fn scan_song_dir_duplicates(dir: &Path) -> Result<Option<SongScan>, ScanError> {
    let mut sms = Vec::new();
    let mut sscs = Vec::new();
    for (extension, path) in simfile_paths(dir)? {
        if extension == "ssc" {
            sscs.push(path);
        } else {
            sms.push(path);
        }
    }
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

pub fn scan_song_dir(dir: &Path, opt: ScanOpt) -> Result<Option<SongScan>, ScanError> {
    if assets::is_mac_resource_fork(dir) {
        return Ok(None);
    }

    match opt.dup {
        DupPolicy::First => scan_song_dir_first(dir),
        DupPolicy::Error => scan_song_dir_duplicates(dir),
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
    let Some(group_name) = dir.file_name().and_then(|s| s.to_str()) else {
        return Err(ScanError::InvalidUtf8Path);
    };

    let (ini, has_pack_ini) = read_pack_ini(dir);
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
        display_title
    } else {
        group_name.to_string()
    };
    let sort_title = if has_pack_ini && !sort_title.is_empty() {
        sort_title
    } else {
        group_name.to_string()
    };
    let translit_title = if has_pack_ini && !translit_title.is_empty() {
        translit_title
    } else {
        display_title.clone()
    };
    let series = if has_pack_ini { series } else { String::new() };
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

    let root = scan_pack_root(dir, opt, &banner, &background)?;
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
        let path = entry.path();
        if assets::is_mac_resource_fork(&path) {
            continue;
        }
        if let Some(pack) = scan_pack_dir(&path, opt)? {
            packs.push(pack);
        }
    }
    packs.sort_by_cached_key(|p| p.group_name.to_ascii_lowercase());
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
            .map(|e| e.path())
            .filter(|p| !assets::is_mac_resource_fork(p) && p.is_dir())
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
        DupPolicy, PackIniRaw, ScanError, ScanOpt, find_simfiles, parse_pack_ini,
        pick_pack_parent_img, scan_pack_dir, scan_pack_root, scan_pack_root_legacy, scan_song_dir,
        scan_songs_dir,
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

        let error = scan_song_dir(
            &root,
            ScanOpt {
                dup: DupPolicy::Error,
            },
        )
        .unwrap_err();
        let ScanError::DuplicateSimfile { ext, paths } = error else {
            panic!("expected a duplicate simfile error");
        };
        assert_eq!(ext, "ssc");
        assert_eq!(
            paths,
            ["Alpha.sSc", "beta.SSC", "Zulu.ssc"].map(|name| root.join(name))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pack_ini_parsing_preserves_group_and_key_behavior() {
        let parsed = parse_pack_ini(
            "\
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
                Year=1900\n",
        );

        assert_eq!(
            parsed,
            PackIniRaw {
                version: "2".to_string(),
                display_title: "Final title".to_string(),
                sort_title: "Sort me".to_string(),
                translit_title: "Translit".to_string(),
                series: "Series name".to_string(),
                banner: "banner*.png".to_string(),
                background: "background.jpg".to_string(),
                sync_offset: "ITG".to_string(),
                year: "2026".to_string(),
            }
        );
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
