use std::path::PathBuf;

pub fn config_path() -> Option<PathBuf> {
    Some(
        std::env::home_dir()?
            .join(".local/state/wpdm/config.conf")
    )
}

pub fn config_dir() -> Option<PathBuf> {
    config_path()?.parent().map(|p| p.to_path_buf())
}

