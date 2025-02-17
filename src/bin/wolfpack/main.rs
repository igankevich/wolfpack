mod builder;
mod config;
mod db;
mod deb;
mod download;
mod error;
mod key;
mod logger;
mod repo;
mod table;

use self::builder::*;
use self::config::*;
use self::download::*;
use self::error::*;
use self::key::*;
use self::logger::*;
use self::repo::*;
use self::table::*;

use base58::FromBase58;
use base58::ToBase58;
use clap::Parser;
use clap::Subcommand;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

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
    /// Build a new package.
    Build(BuildArgs),
    /// Generate signing key.
    Key(KeyArgs),
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
struct BuildArgs {
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
struct KeyArgs {}

fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    do_main().inspect_err(|e| eprintln!("{e}"))
}

fn do_main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    Logger::init().map_err(Error::Logger)?;
    let args = Args::parse();
    let config = Config::open(&args.config_dir)?;
    match args.command {
        Command::Pull => pull(config),
        Command::Search(more_args) => search(config, more_args),
        Command::Install(more_args) => install(config, more_args),
        Command::Resolve(more_args) => resolve(config, more_args),
        Command::Build(more_args) => build(config, more_args),
        Command::Key(more_args) => key(config, more_args),
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

fn build(_config: Config, args: BuildArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let entropy = read_entropy()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let _guard = rt.enter();
    rt.block_on(async {
        let gen = SigningKeyGenerator::new(&entropy);
        let builder = PackageBuilder::new(HashSet::from_iter(PackageFormat::all().iter().copied()));
        builder.build(
            &args.metadata_file,
            &args.rootfs_dir,
            &args.output_dir,
            &gen,
        )?;
        Ok(ExitCode::SUCCESS)
    })
}

fn key(_config: Config, _args: KeyArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let entropy = generate_entropy()?;
    println!("{}", entropy.to_base58());
    Ok(ExitCode::SUCCESS)
}

fn read_entropy() -> Result<Entropy, Box<dyn std::error::Error>> {
    use std::env::VarError::*;
    let s = match std::env::var(ENTROPY_ENV) {
        Ok(value) => value,
        Err(NotPresent) => {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            line
        }
        Err(e @ NotUnicode(..)) => return Err(e.into()),
    };
    let entropy = s
        .from_base58()
        .map_err(|_| "Invalid entropy string")?
        .try_into()
        .map_err(|_| "Invalid entropy string")?;
    Ok(entropy)
}

const ENTROPY_ENV: &str = "WOLFPACK_ENTROPY";
