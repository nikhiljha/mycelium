use std::fmt::format;

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Serialize, Deserialize, Debug)]
struct Versions {
    project_id: String,
    project_name: String,
    version_groups: Vec<String>,
    versions: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Builds {
    project_id: String,
    project_name: String,
    version: String,
    builds: Vec<u32>,
}

pub async fn get_versions(kind: &str) -> Result<Vec<String>, Error> {
    let url = format!("https://papermc.io/api/v2/projects/{kind}", kind = kind);
    // .header("User-Agent", format!("mycelium/{}", env!("CARGO_PKG_VERSION")))
    let resp = reqwest::get(url).await?.json::<Versions>().await?;
    Ok(resp.versions)
}

pub async fn get_builds(kind: &str, version: &str) -> Result<Vec<u32>, Error> {
    let url = format!(
        "https://papermc.io/api/v2/projects/{kind}/versions/{version}",
        kind = kind,
        version = version
    );
    // .header("User-Agent", format!("mycelium/{}", env!("CARGO_PKG_VERSION")))
    let resp = reqwest::get(url).await?.json::<Builds>().await?;
    Ok(resp.builds)
}

pub fn get_download_url(kind: &str, version: &str, build: &str) -> String {
    format!(
        "https://papermc.io/api/v2/projects/{kind}/versions/{version}/builds/{build}/downloads/{kind}-{version}-{build}.jar",
        version = version, build = build, kind = kind
    )
}
