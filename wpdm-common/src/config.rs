use std::{fs::OpenOptions, path::PathBuf};
use std::io::Write;

pub fn config_path() -> Option<PathBuf> {
    Some(
        std::env::home_dir()?
            .join(".local/state/wpdm/config.conf")
    )
}

pub fn config_dir() -> Option<PathBuf> {
    config_path()?.parent().map(|p| p.to_path_buf())
}

pub fn save_wp_path(path: &str) -> std::io::Result<()> {
    let Some(dir) = config_dir() else {
        return Ok(())
    };
    let Some(conf_path) = config_path() else {
        return Ok(())
    };
    std::fs::create_dir_all(dir)?;
    let mut save = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(conf_path)?;

    writeln!(save, "{}", path)?;
    Ok(())
}

