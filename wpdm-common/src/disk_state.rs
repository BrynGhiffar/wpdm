use std::{collections::HashMap, fs::OpenOptions, io::{BufReader, BufWriter}, path::{Path, PathBuf}};

use anyhow::Context;

use crate::config::{self, config_dir, config_path};


#[derive(serde::Serialize, serde::Deserialize)]
pub struct DiskState {
    pub curr_wp: HashMap<String, PathBuf>
}

impl DiskState {
    fn empty() -> Self {
        Self { curr_wp: HashMap::new() } 
    }

    pub fn try_load() -> anyhow::Result<Self> {
        let path = config::config_path().context("Failed to get config_path")?;

        let file = OpenOptions::new().read(true).open(path)?;

        let reader = BufReader::new(file);

        let state: Self = serde_json::from_reader(reader)?;

        Ok(state)
    }

    pub fn load() -> Self {
        Self::try_load().unwrap_or_else(|_| Self::empty())
    }

    pub fn get_curr_wp(monitor: &str) -> anyhow::Result<PathBuf> {
        let ds = Self::load();
        ds.curr_wp.get(monitor).cloned().context("No Wallpaper")
    }

    pub fn try_save_wp(monitor: &str, path: &Path) -> anyhow::Result<()> {
        let mut ds = Self::load();
        ds.curr_wp.insert(monitor.to_string(), path.to_path_buf());
        ds.try_save()?;
        Ok(())
    }

    pub fn try_save(&self) -> anyhow::Result<()> {
        let dir = config_dir().context("Failed to get config directory")?;
        let conf_path = config_path().context("Failed to load config path")?;

        std::fs::create_dir_all(dir)?;

        let file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(conf_path)?;

        let writer = BufWriter::new(file);

        serde_json::to_writer_pretty(writer, &self)?;
        Ok(())
    }
}
