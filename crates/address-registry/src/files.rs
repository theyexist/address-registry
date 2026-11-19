//! The one `*.toml` listing, so every directory in the tree is read the same way.

use crate::error::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// The `*.toml` files in `dir`, sorted. The directory listing is the index, so
/// every reader starts here rather than deciding for itself what counts.
pub fn toml(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let entries = fs::read_dir(dir).map_err(|error| Error::at(dir, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| Error::at(dir, error))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}
