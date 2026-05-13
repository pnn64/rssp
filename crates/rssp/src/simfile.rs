use std::fs;
use std::io;
use std::path::Path;

/// A simfile loaded from disk.
///
/// `extension` is normalized to `"sm"` or `"ssc"`.
#[derive(Debug, Clone)]
pub struct OpenedSimfile {
    pub data: Vec<u8>,
    pub extension: &'static str,
}

fn ext_of(path: &Path) -> io::Result<&'static str> {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Missing file extension (must be .sm or .ssc)",
        ));
    };
    if ext.eq_ignore_ascii_case("sm") {
        Ok("sm")
    } else if ext.eq_ignore_ascii_case("ssc") {
        Ok("ssc")
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported file extension (must be .sm or .ssc)",
        ))
    }
}

/// Reads a `.sm` or `.ssc` simfile from `path`.
pub fn open(path: impl AsRef<Path>) -> io::Result<OpenedSimfile> {
    let path = path.as_ref();
    let extension = ext_of(path)?;
    let data = fs::read(path)?;
    Ok(OpenedSimfile { data, extension })
}
