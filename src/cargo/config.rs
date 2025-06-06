use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use command_error::ChildExt;
use command_error::CommandExt;
use elb::DynamicTag;
use elb::Elf;
use elb::SectionKind;
use elb::StringTable;
use log::warn;
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
    #[serde(flatten)]
    pub common: build::Config,
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
            common: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct BuildOutput {
    pub files: Vec<(build::BuildTarget, PathBuf)>,
    pub interpreter: Option<(PathBuf, PathBuf)>,
}

pub fn build_package(
    package_name: &str,
    config: &BuildConfig,
    project_dir: &Path,
) -> Result<BuildOutput, Error> {
    // Build a dummy package to figure out what dynamic loader Cargo uses.
    let old_interpreter = build_dummy_package(package_name, config)?;
    let (interpreter, rust_flags) = old_interpreter
        .map(|old_interpreter| {
            let mut buf = PathBuf::new();
            buf.push(&config.common.prefix);
            buf.push(package_name);
            buf.push("lib");
            buf.push(old_interpreter.file_name().expect("File name exists"));
            let new_interpreter = buf;
            let mut flags = std::env::var_os(RUSTFLAGS).unwrap_or_default();
            if !flags.is_empty() {
                flags.push(" ");
            }
            flags.push("-Clink-arg=-Wl,-rpath=");
            flags.push(&config.common.prefix);
            flags.push("/");
            flags.push(package_name);
            flags.push("/lib");
            flags.push(" ");
            flags.push("-Clink-arg=-Wl,-dynamic-linker=");
            flags.push(&new_interpreter);
            (Some((old_interpreter, new_interpreter)), flags)
        })
        .unwrap_or_default();
    // Build the actual package with "patched" interpreter and runpath.
    let files = do_build_package(package_name, config, project_dir, &rust_flags)?;
    Ok(BuildOutput { files, interpreter })
}

fn build_dummy_package(package_name: &str, config: &BuildConfig) -> Result<Option<PathBuf>, Error> {
    let tmpdir = tempfile::TempDir::with_prefix("wolfpack-cargo-")?;
    let dummy_project_dir = tmpdir.path().join("dummy");
    let mut command = Command::new("cargo");
    if let Some(rust_flags) = std::env::var_os(RUSTFLAGS) {
        command.env(RUSTFLAGS, rust_flags);
    }
    command.arg("new");
    command.args(["--quiet", "--name", "dummy"]);
    command.arg(&dummy_project_dir);
    command.stdin(Stdio::null());
    command.status_checked()?;
    let mut outputs = do_build_package("dummy", config, &dummy_project_dir, Default::default())?;
    assert_eq!(1, outputs.len());
    let mut interpreter = None;
    let (_type, path) = outputs.remove(0);
    let mut file = fs_err::File::open(&path)?;
    let elf = Elf::read(&mut file, page_size::get() as u64)?;
    if let Some(interp) = elf.read_interpreter(&mut file)? {
        interpreter = Some(interp);
    }
    check_rpath(elf, file, &config.common.prefix.join(package_name))?;
    let path = interpreter.map(|cstring| OsString::from_vec(cstring.into_bytes()).into());
    Ok(path)
}

fn check_rpath(elf: Elf, mut file: fs_err::File, prefix: &Path) -> Result<(), Error> {
    let Some(dynamic_table) = elf.read_dynamic_table(&mut file)? else {
        return Ok(());
    };
    let Some(addr) = dynamic_table.get(DynamicTag::StringTableAddress) else {
        return Ok(());
    };
    let Some(dynstr_table_index) = elf.sections.iter().position(|section| {
        section.kind == SectionKind::StringTable && section.virtual_address == addr
    }) else {
        return Ok(());
    };
    let dynstr_table: StringTable = elf.sections[dynstr_table_index].read_content(
        &mut file,
        elf.header.class,
        elf.header.byte_order,
    )?;
    // Check for rpath/runpath outside of the installation prefix.
    for (tag, tag_name) in [
        (DynamicTag::Runpath, "RUNPATH"),
        (DynamicTag::Rpath, "RPATH"),
    ] {
        let Some(runpath_offset) = dynamic_table.get(tag) else {
            continue;
        };
        let Some(runpath) = dynstr_table.get_string(runpath_offset as usize) else {
            continue;
        };
        let runpath = OsString::from_vec(runpath.to_bytes().to_vec());
        for dir in std::env::split_paths(&runpath) {
            if dir.strip_prefix(prefix).is_err() {
                warn!(
                    "{tag_name} directory {dir:?}, that was added by the linker by default, \
                    is outside of the installation prefix {prefix:?}",
                );
            }
        }
    }
    Ok(())
}

fn do_build_package(
    package_name: &str,
    config: &BuildConfig,
    project_dir: &Path,
    rust_flags: &OsStr,
) -> Result<Vec<(build::BuildTarget, PathBuf)>, Error> {
    let mut command = Command::new("cargo");
    if !rust_flags.is_empty() {
        command.env(RUSTFLAGS, rust_flags);
    }
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
    command.current_dir(project_dir);
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
    #[error("ELF error: {0}")]
    Elb(#[from] elb::Error),
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

const RUSTFLAGS: &str = "RUSTFLAGS";
