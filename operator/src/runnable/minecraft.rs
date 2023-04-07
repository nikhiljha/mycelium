use std::{env, thread};
use std::fs::{File, read_to_string};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use nix::libc::pid_t;
use nix::sys::signal;
use nix::unistd::Pid;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use crate::runnable::{Download, Features, Runnable};

#[derive(Default)]
pub struct Minecraft {
    pub jar: Option<String>
}

impl Runnable for Minecraft {
    fn initialize(&self) -> anyhow::Result<Features> {
        Ok(Features::default())
    }

    #[allow(unreachable_code)]
    fn download(&self) -> anyhow::Result<Vec<Download>> {
        Ok(vec![
            Download {
                path: PathBuf::from("server.jar"),
                url: todo!("The vanilla Minecraft server is currently not supported."),
                ..Download::default()
            }
        ])
    }

    fn configure(&self, base_path: PathBuf) -> anyhow::Result<()> {
        // TODO: Support configurable server.properties.
        match read_to_string(base_path.join("server.properties")) {
            Ok(_) => {}
            Err(_) => {
                let mut f = File::create(base_path.join("server.properties"))?;
                f.write_all(include_str!("../../defaults/server.properties").as_bytes())?;
            }
        }
        Ok(())
    }

    fn start(&self, base_path: PathBuf) -> anyhow::Result<()> {
        let jvm_opts = env::var("MYCELIUM_JVM_OPTS").unwrap_or_else(|_| "".into());
        let jar = self.jar.as_deref().unwrap_or("server.jar");
        let args: Vec<&str> = jvm_opts
            .split_terminator(' ')
            .chain(["-Dsun.net.inetaddr.ttl=0", "-jar", jar])
            .collect();

        let mut signals = Signals::new([SIGTERM, SIGINT])?;
        let mut minecraft = Command::new("java")
            .args(args)
            .current_dir(base_path)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

        let id = minecraft.id();
        let handle = signals.handle();
        thread::spawn(move || {
            for _ in signals.forever() {
                println!("[runner] Caught interrupt, sending sigterm to java...");
                signal::kill(Pid::from_raw(id as pid_t), signal::Signal::SIGTERM)
                    .expect("can't kill java");
            }
        });

        minecraft.wait()?;
        handle.close();
        Ok(())
    }
}
