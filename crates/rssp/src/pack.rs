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

fn keep_first_path(first: &mut Option<(String, PathBuf)>, candidate: PathBuf) {
    let key = assets::lc_name(&candidate);
    if first
        .as_ref()
        .is_none_or(|(current_key, _)| key < *current_key)
    {
        *first = Some((key, candidate));
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

#[derive(Default)]
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
        let val = v.trim().to_string();
        match key.to_ascii_lowercase().as_str() {
            "version" => out.version = val,
            "displaytitle" => out.display_title = val,
            "sorttitle" => out.sort_title = val,
            "translittitle" => out.translit_title = val,
            "series" => out.series = val,
            "banner" => out.banner = val,
            "background" => out.background = val,
            "syncoffset" => out.sync_offset = val,
            "year" => out.year = val,
            _ => {}
        }
    }

    out
}

fn read_pack_ini(pack_dir: &Path, group_name: &str) -> (PackIniRaw, bool) {
    let path = pack_ini_path(pack_dir);
    let Ok(text) = fs::read_to_string(path) else {
        return (PackIniRaw::default(), false);
    };
    let raw = parse_pack_ini(&text);
    if raw.version.trim().is_empty() {
        return (PackIniRaw::default(), false);
    }
    let mut raw = raw;
    if raw.display_title.trim().is_empty() {
        raw.display_title = group_name.to_string();
    }
    if raw.sort_title.trim().is_empty() {
        raw.sort_title = group_name.to_string();
    }
    if raw.translit_title.trim().is_empty() {
        raw.translit_title = raw.display_title.clone();
    }
    (raw, true)
}

fn pick_pack_parent_img(pack_dir: &Path, group_name: &str) -> Option<PathBuf> {
    let parent = pack_dir.parent()?;
    for ext in ["png", "jpg", "jpeg", "gif", "bmp"] {
        let name = format!("{group_name}.{ext}");
        if let Some(p) = assets::is_file_ci(parent, &name) {
            return Some(p);
        }
    }
    None
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
    first.map(|(_, path)| path)
}

fn pick_pack_dir_img(pack_dir: &Path) -> Option<PathBuf> {
    pick_first_img(pack_dir, |_| true)
}

fn pick_ini_img(pack_dir: &Path, hint: &str) -> Option<PathBuf> {
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
        .map(|(_, path)| song_scan(dir, path, "ssc"))
        .or_else(|| first_sm.map(|(_, path)| song_scan(dir, path, "sm"))))
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

pub fn scan_pack_dir(dir: &Path, opt: ScanOpt) -> Result<Option<PackScan>, ScanError> {
    if assets::is_mac_resource_fork(dir) || !dir.is_dir() {
        return Ok(None);
    }
    let Some(group_name) = dir.file_name().and_then(|s| s.to_str()) else {
        return Err(ScanError::InvalidUtf8Path);
    };

    let (ini, has_pack_ini) = read_pack_ini(dir, group_name);
    let display_title = if has_pack_ini {
        ini.display_title.clone()
    } else {
        group_name.to_string()
    };
    let sort_title = if has_pack_ini {
        ini.sort_title.clone()
    } else {
        group_name.to_string()
    };
    let translit_title = if has_pack_ini {
        ini.translit_title.clone()
    } else {
        display_title.clone()
    };
    let series = if has_pack_ini {
        ini.series.clone()
    } else {
        String::new()
    };
    let year = if has_pack_ini {
        ini.year.trim().parse().unwrap_or(0)
    } else {
        0
    };
    let version = if has_pack_ini {
        ini.version.trim().parse().unwrap_or(0)
    } else {
        0
    };
    let sync_pref = if has_pack_ini {
        parse_sync_pref(&ini.sync_offset)
    } else {
        SyncPref::Default
    };

    let ini_banner = pick_ini_img(dir, &ini.banner);
    let ini_background = pick_ini_img(dir, &ini.background);
    let auto_background = if ini_background.is_none() {
        assets::resolve_song_assets(dir, "", "").1
    } else {
        None
    };

    // ITGmania group banners are simpler than song assets: if the pack root
    // contains any image, the first one is treated as the group banner.
    let banner_path = ini_banner
        .or_else(|| pick_pack_dir_img(dir))
        .or_else(|| pick_pack_parent_img(dir, group_name));
    let background_path = ini_background.or(auto_background);

    let mut songs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if assets::is_mac_resource_fork(&path) {
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        if let Some(song) = scan_song_dir(&path, opt)? {
            songs.push(song);
        }
    }

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
        DupPolicy, ScanError, ScanOpt, find_simfiles, scan_pack_dir, scan_song_dir, scan_songs_dir,
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
