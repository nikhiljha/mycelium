pub mod minecraft;
pub mod paper;

use std::path::PathBuf;

#[derive(Default)]
pub struct Download {
    pub path: PathBuf,
    pub url: String,
    pub sha256: Option<String>,
    pub force: bool,
}

#[derive(Default)]
pub struct Features {
    pub velocity: bool,
}

/// some kind of game server
pub trait Runnable {
    /// do any initial initialization work (e.x. check available resources), return the features available
    fn initialize(&self) -> anyhow::Result<Features>;
    /// returns the [Download]s to be asserted before running configure
    fn download(&self) -> anyhow::Result<Vec<Download>>;
    /// modify or create configuration files relative to the base_path
    fn configure(&self, base_path: PathBuf) -> anyhow::Result<()>;
    /// start and run the gameserver (probably via process fork) and optionally wrap it
    fn start(&self, base_path: PathBuf) -> anyhow::Result<()>;
}
