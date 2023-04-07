use std::fs::{File, read_to_string};
use std::io::Write;
use std::path::PathBuf;
use crate::runnable::{Download, Features, Runnable};
use crate::runnable::minecraft::Minecraft;

pub struct Paper {
    minecraft: Minecraft,
    version: String,
    build: String,
}

impl Default for Paper {
    fn default() -> Self {
        Paper {
            minecraft: Minecraft::default(),
            version: String::from("1.19.3"),
            build: String::from("latest"),
        }
    }
}

impl Paper {
    pub fn new(version: &str, build: &str) -> Self {
        Paper {
            minecraft: Minecraft { jar: Some(String::from("paper.jar")) },
            version: version.to_string(),
            build: build.to_string(),
        }
    }
}

impl Runnable for Paper {
    fn initialize(&self) -> anyhow::Result<Features> {
        Ok(Features { velocity: true })
    }

    fn download(&self) -> anyhow::Result<Vec<Download>> {
        let url = format!(
            "https://papermc.io/api/v2/projects/paper/versions/{}/builds/{}/downloads/paper-{}-{}.jar",
            self.version, self.build, self.version, self.build
        );
        Ok(vec![
            Download {
                path: PathBuf::from("paper.jar"),
                url,
                ..Download::default()
            }
        ])
    }

    fn configure(&self, base_path: PathBuf) -> anyhow::Result<()> {
        self.minecraft.configure(base_path.clone())?;

        match read_to_string(base_path.join("paper.yml")) {
            Ok(_) => {}
            Err(_) => {
                let mut f = File::create(base_path.join("paper.yml"))?;
                f.write_all(include_str!("../../defaults/paper.yml").as_bytes())?;
            }
        }

        let eula_path = base_path.join("eula.txt");
        if !eula_path.exists() {
            let mut f = File::create(eula_path)?;
            f.write_all(b"eula=true\n")?;
        }

        Ok(())
    }

    fn start(&self, base_path: PathBuf) -> anyhow::Result<()> {
        self.minecraft.start(base_path)
    }
}
