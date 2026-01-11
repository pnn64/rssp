use std::path::{Path, PathBuf};

use crate::{AnalysisOptions, SimfileSummary, analyze};

pub struct SimfilePack {
    /// The name of the simfile pack (derived from the basename of the directory)
    pub name: String,
    /// The directory containing the simfile pack
    pub directory: PathBuf,
}

pub enum SimfilePackError {
    InvalidPath,
    IoError(std::io::Error),
}

impl From<std::io::Error> for SimfilePackError {
    fn from(value: std::io::Error) -> Self {
        SimfilePackError::IoError(value)
    }
}

impl SimfilePack {
    /// Construct a SimfilePack from the path to its containing directory
    pub fn from_path(path: &Path) -> Result<SimfilePack, SimfilePackError> {
        let directory = path.canonicalize()?;
        let name = directory
            .file_name()
            .expect("directory to be in absolute, canonical form")
            .to_str()
            .ok_or(SimfilePackError::InvalidPath)?
            .to_string();
        match directory.try_exists() {
            Err(e) => Err(SimfilePackError::IoError(e)),
            Ok(false) => Err(SimfilePackError::InvalidPath),
            Ok(true) => Ok(SimfilePack { name, directory }),
        }
        // TODO: parse Pack.ini and include pack metadata
    }

    pub fn simfiles(
        self,
        analysis_options: AnalysisOptions,
    ) -> Result<impl Iterator<Item = Result<SimfileSummary, String>>, SimfilePackError> {
        Ok(self
            .directory
            .read_dir()?
            // filter to only readable subdirectories and read
            .filter_map(|entry_res| entry_res.ok()?.path().read_dir().ok())
            // get paths to .ssc/.sm files (preferring .ssc)
            .filter_map(|read_dir| {
                let mut sm_path: Option<PathBuf> = None;
                let mut ssc_path: Option<PathBuf> = None;
                for pack_subdir in read_dir {
                    let Ok(pack_subdir) = pack_subdir else {
                        continue;
                    };
                    if let Some(ext) = pack_subdir.path().extension() {
                        if ext.eq_ignore_ascii_case("ssc") {
                            ssc_path = Some(pack_subdir.path());
                        }
                        if ext.eq_ignore_ascii_case("sm") {
                            sm_path = Some(pack_subdir.path());
                        }
                    };
                }
                ssc_path.or(sm_path)
            })
            // Pass simfile contents to `analyze`, discarding any read errors.
            .filter_map(move |simfile_path| {
                //TODO: adapt main::analyze_simfile to be used here instead.
                let extension = simfile_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                let contents = std::fs::read(simfile_path.clone()).ok()?;
                Some(analyze(&contents, extension, analysis_options.clone()))
            }))
    }
}
