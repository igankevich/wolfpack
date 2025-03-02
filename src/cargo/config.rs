use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use command_error::ChildExt;
use command_error::CommandExt;
use serde::de::Deserializer;
use serde::Deserialize;

use crate::build;

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct BuildConfig {
    pub target: Option<String>,
    pub profile: Option<String>,
    pub release: bool,
    pub default_features: bool,
    pub features: BTreeSet<String>,
    pub env: BTreeMap<OsString, OsString>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            target: Default::default(),
            profile: Default::default(),
            release: true,
            default_features: true,
            features: Default::default(),
            env: Default::default(),
        }
    }
}

pub fn build_package<P: AsRef<Path>>(
    package_name: &str,
    config: &BuildConfig,
    project_dir: P,
) -> Result<Vec<(build::BuildTarget, PathBuf)>, Error> {
    let mut command = Command::new("cargo");
    command.arg("build");
    command.arg("--package");
    command.arg(package_name);
    if let Some(target) = config.target.as_deref() {
        command.arg("--target");
        command.arg(target);
    }
    if let Some(profile) = config.profile.as_deref() {
        command.arg("--profile");
        command.arg(profile);
    }
    if !config.default_features {
        command.arg("--no-default-features");
    }
    if !config.features.is_empty() {
        command.arg("--features");
        command.arg(join(config.features.iter().map(|s| s.as_str()), " "));
    }
    for (name, value) in config.env.iter() {
        command.env(name, value);
    }
    if config.release {
        command.arg("--release");
    }
    command.arg("--message-format=json-render-diagnostics");
    command.current_dir(project_dir.as_ref());
    command.stdout(Stdio::piped());
    let mut child = command.spawn_checked()?;
    let mut output_files = Vec::new();
    if let Some(stdout) = child.child_mut().stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = line?;
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
    child.wait_checked()?;
    Ok(output_files)
}

pub fn get_packages<P: AsRef<Path>>(project_dir: P) -> Result<Vec<Package>, Error> {
    let mut command = Command::new("cargo");
    command.arg("metadata");
    command.arg("--format-version=1");
    command.arg("--no-deps");
    command.current_dir(project_dir.as_ref());
    command.stdout(Stdio::piped());
    let mut child = command.spawn_checked()?;
    let json = {
        let mut stdout = child.child_mut().stdout.take().expect("Stdout exists");
        let mut json = String::new();
        stdout.read_to_string(&mut json)?;
        json
    };
    let mut metadata: Metadata = serde_json::from_str(&json)?;
    child.wait_checked()?;
    for package in metadata.packages.iter_mut() {
        if package.metadata.wolfpack.is_empty() {
            package.metadata = Default::default();
        }
    }
    Ok(metadata.packages)
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

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Deserialize, Debug)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub documentation: Option<String>,
    pub readme: Option<String>,
    pub license: Option<String>,
    pub license_file: Option<String>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub metadata: PackageMetadata,
}

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct PackageMetadata {
    pub wolfpack: BTreeMap<String, BuildConfig>,
}

impl Default for PackageMetadata {
    fn default() -> Self {
        Self {
            wolfpack: [("default".into(), BuildConfig::default())].into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Input/output error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Command(#[from] command_error::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
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

fn deserialize_nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    let value = Option::<T>::deserialize(deserializer)?;
    Ok(value.unwrap_or_default())
}
