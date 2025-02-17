mod config;
mod db;
mod deb;
mod download;
mod error;
mod logger;
mod repo;
mod table;

use self::config::*;
use self::download::*;
use self::error::*;
use self::logger::*;
use self::repo::*;
use self::table::*;

use clap::Parser;
use clap::Subcommand;
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
    Install(InstallArgs),
    Resolve(ResolveArgs),
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
    }
}

fn pull(mut config: Config) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
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
