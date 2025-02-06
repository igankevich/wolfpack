mod config;
mod db;
mod download;
mod error;
mod logger;

use self::config::*;
use self::db::*;
use self::download::*;
use self::error::*;
use self::logger::*;

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
struct SearchArgs {
    /// Search keyword.
    #[clap(value_name = "KEYWORD")]
    keyword: String,
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
    let keyword = args.keyword.to_lowercase();
    for (name, repo) in repos.iter_mut() {
        repo.search(&config, name.as_str(), &keyword)?;
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
