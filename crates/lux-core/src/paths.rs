// SPDX-License-Identifier: GPL-2.0-or-later
use std::path::{Path, PathBuf};

/// Data directory: `--home`, else `$LUX_HOME`, else `~/.lux`.
pub fn home_dir(explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Some(p) = std::env::var_os("LUX_HOME") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join(".lux")
}

pub fn config_path(home: &Path) -> PathBuf {
    home.join("config.toml")
}

pub fn identity_path(home: &Path) -> PathBuf {
    home.join("identity.key")
}

pub fn socket_path(home: &Path) -> PathBuf {
    home.join("luxd.sock")
}

pub fn db_path(home: &Path) -> PathBuf {
    home.join("node.db")
}
