use std::{
    collections::HashSet,
    fs::File,
    path::Path,
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};
use tracing::info;

pub(crate) static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Config {
    posted_blogs: HashSet<String>,
    posted_bug_ids: HashSet<i64>,
}

impl Config {
    pub fn is_blog_posted(&self, id: &str) -> bool {
        self.posted_blogs.contains(id)
    }

    pub fn add_posted_blog(&mut self, id: String) {
        self.posted_blogs.insert(id);
    }

    pub fn is_bug_posted(&self, id: i64) -> bool {
        self.posted_bug_ids.contains(&id)
    }

    pub fn add_posted_bug(&mut self, id: i64) {
        self.posted_bug_ids.insert(id);
    }

    pub fn flush(&self) -> anyhow::Result<()> {
        info!("Flushing config...");
        let mut file = File::create("config.json")?;
        serde_json::to_writer(&mut file, self)?;

        Ok(())
    }
}

pub fn load() {
    let config = if let Ok(config_file) = File::open("config.json") {
        serde_json::from_reader(config_file).unwrap_or_default()
    } else {
        Default::default()
    };

    CONFIG
        .set(Mutex::new(config))
        .expect("could not set config");
}

pub fn get() -> &'static Mutex<Config> {
    CONFIG.get_or_init(|| Default::default())
}
