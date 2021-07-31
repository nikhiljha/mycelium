#![warn(rust_2018_idioms)]
#![allow(unused_imports)]

use thiserror::Error;

pub use objects::minecraft_proxy::MinecraftProxy;
pub use objects::minecraft_set::MinecraftSet;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kube Api Error: {0}")]
    KubeError(#[source] kube::Error),

    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// generated types
pub mod objects;
pub mod helpers;
