use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Error;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use serde::Deserialize;

use crate::build;
use crate::cargo::Arch;

#[derive(Default)]
pub struct BuildConfig {
    pub target: Option<String>,
    pub profile: Option<String>,
    pub packages: BTreeSet<String>,
    pub no_default_features: bool,
    pub features: BTreeSet<String>,
    pub env: BTreeMap<OsString, OsString>,
}

impl BuildConfig {
    pub fn arch(&self) -> Option<Arch> {
        self.target
            .as_deref()?
            .split('-')
            .next()?
            .parse::<Arch>()
            .ok()
    }
}

pub fn build_package<P: AsRef<Path>>(
    config: &BuildConfig,
    project_dir: P,
) -> Result<Vec<(build::BuildTarget, PathBuf)>, Error> {
    let mut command = Command::new("cargo");
    command.arg("build");
    if let Some(target) = config.target.as_deref() {
        command.arg("--target");
        command.arg(target);
    }
    if let Some(profile) = config.profile.as_deref() {
        command.arg("--profile");
        command.arg(profile);
    }
    for package in config.packages.iter() {
        command.arg("--package");
        command.arg(package.as_str());
    }
    if config.no_default_features {
        command.arg("--no-default-features");
    }
    if !config.features.is_empty() {
        command.arg("--features");
        command.arg(join(config.features.iter().map(|s| s.as_str()), " "));
    }
    for (name, value) in config.env.iter() {
        command.env(name, value);
    }
    // TODO
    //command.arg("--release");
    command.arg("--message-format=json-render-diagnostics");
    command.current_dir(project_dir.as_ref());
    command.stdout(Stdio::piped());
    let mut child = command.spawn()?;
    let mut output_files = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = line?;
            eprintln!("line {}", line);
            let Ok(message) = serde_json::from_str::<BuildMessage>(&line) else {
                continue;
            };
            if let Some(executable) = message.executable {
                if !message.target.kind.is_empty() {
                    for kind in message.target.kind.into_iter() {
                        let target = match kind.as_str() {
                            "cdylib" | "dylib" | "staticlib" => build::BuildTarget::Library,
                            _ => build::BuildTarget::Executable,
                        };
                        output_files.push((target, executable.clone().into()));
                    }
                } else {
                    let target = build::BuildTarget::Executable;
                    output_files.push((target, executable.into()));
                }
            }
        }
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(std::io::Error::other("`cargo build` failed"));
    }
    eprintln!("Outputs {:#?}", output_files);
    Ok(output_files)
}

#[derive(Deserialize)]
struct BuildMessage {
    target: BuildTarget,
    executable: Option<String>,
}

#[derive(Deserialize)]
struct BuildTarget {
    kind: Vec<String>,
}

fn join<'a, I>(items: I, separator: &str) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let mut buf = String::new();
    let mut iter = items.into_iter();
    if let Some(first_item) = iter.next() {
        buf.push_str(first_item);
    }
    for item in iter {
        buf.push_str(separator);
        buf.push_str(item);
    }
    buf
}
