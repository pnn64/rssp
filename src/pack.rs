use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::assets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DupPolicy {
    First,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPref {
    Default,
    Null,
    Itg,
}

#[derive(Debug, Clone, Copy)]
pub struct ScanOpt {
    pub dup: DupPolicy,
}

impl Default for ScanOpt {
    fn default() -> Self {
        Self { dup: DupPolicy::First }
    }
}

#[derive(Debug)]
pub enum ScanError {
    Io(io::Error),
    InvalidUtf8Path,
    DuplicateSimfile { ext: &'static str, paths: Vec<PathBuf> },
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
    let mut files = assets::list_img_files(&dir);
    files.retain(|p| {
        p.file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| assets::match_mask_ci(n, mask))
    });
    files.sort_by_cached_key(|p| assets::lc_name(p));
    files.into_iter().next()
}

pub fn scan_song_dir(dir: &Path, opt: ScanOpt) -> Result<Option<SongScan>, ScanError> {
    let mut sms = Vec::new();
    let mut sscs = Vec::new();

    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if ext.eq_ignore_ascii_case("ssc") {
            sscs.push(path);
        } else if ext.eq_ignore_ascii_case("sm") {
            sms.push(path);
        }
    }

    if sms.is_empty() && sscs.is_empty() {
        return Ok(None);
    }

    sort_paths_ci(&mut sms);
    sort_paths_ci(&mut sscs);

    if !sscs.is_empty() {
        if opt.dup == DupPolicy::Error && sscs.len() > 1 {
            return Err(ScanError::DuplicateSimfile {
                ext: "ssc",
                paths: sscs,
            });
        }
        let simfile = sscs[0].clone();
        return Ok(Some(SongScan {
            dir: dir.to_path_buf(),
            simfile,
            extension: "ssc",
        }));
    }

    if opt.dup == DupPolicy::Error && sms.len() > 1 {
        return Err(ScanError::DuplicateSimfile { ext: "sm", paths: sms });
    }
    let simfile = sms[0].clone();
    Ok(Some(SongScan {
        dir: dir.to_path_buf(),
        simfile,
        extension: "sm",
    }))
}

pub fn scan_pack_dir(dir: &Path, opt: ScanOpt) -> Result<Option<PackScan>, ScanError> {
    if !dir.is_dir() {
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
    let series = if has_pack_ini { ini.series.clone() } else { String::new() };
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
    let (auto_banner, auto_background) = if ini_banner.is_none() || ini_background.is_none() {
        assets::resolve_song_assets(dir, "", "")
    } else {
        (None, None)
    };

    let banner_path = ini_banner
        .or(auto_banner)
        .or_else(|| pick_pack_parent_img(dir, group_name));
    let background_path = ini_background.or(auto_background);

    let mut songs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
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
        let Ok(song) = scan_song_dir(&dir, opt) else { continue };
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
            .filter(|p| p.is_dir())
            .collect();
        sort_paths_ci(&mut subdirs);
        for subdir in subdirs.into_iter().rev() {
            stack.push(subdir);
        }
    }

    out
}
