#![warn(rust_2018_idioms)]
#![allow(unused_imports)]

pub use objects::{minecraft_proxy::MinecraftProxy, minecraft_set::MinecraftSet};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kube Api Error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),

    #[error("ReqwestError: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("VarError: {0}")]
    VarError(#[from] std::env::VarError),

    #[error("MyceliumError: {0}")]
    MyceliumError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub mod helpers;
/// generated types
pub mod objects;
