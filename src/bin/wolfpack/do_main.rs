use clap::Parser;
use clap::Subcommand;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::Config;
use crate::Error;
use crate::Logger;
use crate::MasterSecretKey;
use crate::PackageBuilder;
use crate::PackageFormat;
use crate::ProjectBuilder;
use crate::SigningKeyGenerator;

#[derive(Parser)]
struct Args {
    #[arg(short = 'c', long = "config", default_value = "/etc/wolfpack")]
    config_dir: PathBuf,
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sync repository metadata.
    Pull,
    /// Find packages.
    Search(SearchArgs),
    /// Install an existing package.
    Install(InstallArgs),
    /// Resolve dependencies.
    Resolve(ResolveArgs),
    /// Generate signing key.
    Key(KeyArgs),
    /// Build a new project.
    BuildProject(BuildProjectArgs),
    /// Build a new package.
    BuildPackage(BuildPackageArgs),
    /// Build a new repository.
    BuildRepo(BuildRepoArgs),
}

#[derive(clap::Args)]
struct SearchArgs {
    /// Search query.
    #[clap(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "KEYWORD"
    )]
    query: Vec<String>,
}

#[derive(clap::Args)]
struct InstallArgs {
    #[clap(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "PACKAGE"
    )]
    packages: Vec<String>,
}

#[derive(clap::Args)]
struct ResolveArgs {
    #[clap(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "DEPENDENCY"
    )]
    dependencies: Vec<String>,
}

#[derive(clap::Args)]
struct KeyArgs {}

#[derive(clap::Args)]
struct BuildProjectArgs {
    /// Directory with the source code.
    #[clap(value_name = "source code directory")]
    source_dir: PathBuf,

    /// Output directory.
    #[clap(value_name = "output directory")]
    output_dir: PathBuf,
}

#[derive(clap::Args)]
struct BuildPackageArgs {
    /// Secret key file.
    #[clap(long = "secret-key-file", env = "WOLFPACK_SECRET_KEY_FILE")]
    secret_key_file: Option<PathBuf>,

    /// Package metadata file.
    #[clap(value_name = "METADATA-FILE")]
    metadata_file: PathBuf,

    /// Directory with package contents.
    #[clap(value_name = "ROOTFS-DIRECTORY")]
    rootfs_dir: PathBuf,

    /// Output directory.
    #[clap(value_name = "OUTPUT-DIRECTORY")]
    output_dir: PathBuf,
}

#[derive(clap::Args)]
struct BuildRepoArgs {
    /// Secret key file.
    #[clap(long = "secret-key-file", env = "WOLFPACK_SECRET_KEY_FILE")]
    secret_key_file: Option<PathBuf>,

    /// Repository metadata file.
    #[clap(value_name = "METADATA-FILE")]
    metadata_file: PathBuf,

    /// Directory with pre-built packages.
    #[clap(value_name = "INPUT-DIRECTORY")]
    input_dir: PathBuf,

    /// Output directory.
    #[clap(value_name = "OUTPUT-DIRECTORY")]
    output_dir: PathBuf,
}

pub fn do_main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    Logger::init().map_err(Error::Logger)?;
    let args = Args::parse();
    let config = Config::open(&args.config_dir)?;
    match args.command {
        Command::Pull => pull(config),
        Command::Search(more_args) => search(config, more_args),
        Command::Install(more_args) => install(config, more_args),
        Command::Resolve(more_args) => resolve(config, more_args),
        Command::BuildProject(more_args) => build_project(more_args),
        Command::BuildPackage(more_args) => build_package(more_args),
        Command::BuildRepo(more_args) => build_repo(more_args),
        Command::Key(more_args) => key(more_args),
    }
}

fn pull(mut config: Config) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .thread_name("tokio")
        .enable_all()
        .build()?;
    let _guard = rt.enter();
    rt.block_on(async {
        let mut repos = config.take_repos();
        for (name, repo) in repos.iter_mut() {
            repo.pull(&config, name.as_str()).await?;
        }
        Ok(ExitCode::SUCCESS)
    })
}

fn search(mut config: Config, args: SearchArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut repos = config.take_repos();
    let query = args.query.join(" ");
    for (name, repo) in repos.iter_mut() {
        repo.search(&config, name.as_str(), &query)?;
    }
    Ok(ExitCode::SUCCESS)
}

fn install(mut config: Config, args: InstallArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let _guard = rt.enter();
    rt.block_on(async {
        let mut repos = config.take_repos();
        for (name, repo) in repos.iter_mut() {
            repo.install(&config, name.as_str(), args.packages.clone())
                .await?;
        }
        Ok(ExitCode::SUCCESS)
    })
}

fn resolve(mut config: Config, args: ResolveArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let _guard = rt.enter();
    rt.block_on(async {
        let mut repos = config.take_repos();
        for (name, repo) in repos.iter_mut() {
            repo.resolve(&config, name.as_str(), args.dependencies.clone())?;
        }
        Ok(ExitCode::SUCCESS)
    })
}

fn build_project(args: BuildProjectArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let builder = ProjectBuilder::new();
    builder.build(&args.source_dir, &args.output_dir)?;
    Ok(ExitCode::SUCCESS)
}

fn build_package(args: BuildPackageArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let master_secret_key = read_master_key(args.secret_key_file.as_ref())?;
    let gen = SigningKeyGenerator::new(&master_secret_key);
    let builder = PackageBuilder::new(HashSet::from_iter(PackageFormat::all().iter().copied()));
    builder.build_package(
        &args.metadata_file,
        &args.rootfs_dir,
        &args.output_dir,
        &gen,
    )?;
    Ok(ExitCode::SUCCESS)
}

fn build_repo(args: BuildRepoArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let master_secret_key = read_master_key(args.secret_key_file.as_ref())?;
    let gen = SigningKeyGenerator::new(&master_secret_key);
    let builder = PackageBuilder::new(HashSet::from_iter(PackageFormat::all().iter().copied()));
    builder.build_repo(&args.metadata_file, &args.input_dir, &args.output_dir, &gen)?;
    Ok(ExitCode::SUCCESS)
}

fn key(_key_args: KeyArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let master_secret_key = MasterSecretKey::generate();
    println!("{}", master_secret_key);
    Ok(ExitCode::SUCCESS)
}

fn read_master_key<P: AsRef<Path>>(
    secret_key_file: Option<&P>,
) -> Result<MasterSecretKey, Box<dyn std::error::Error>> {
    let s = match secret_key_file {
        Some(file) => {
            let file = file.as_ref();
            std::fs::read_to_string(file).map_err(|e| Error::file_read(file, e))?
        }
        None => {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            line
        }
    };
    let master_secret_key: MasterSecretKey = s
        .trim()
        .parse()
        .map_err(|_| "Invalid master secret key string")?;
    Ok(master_secret_key)
}
