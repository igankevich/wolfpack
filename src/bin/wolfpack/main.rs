mod config;
mod logger;

use self::config::*;
use self::logger::*;

use clap::Parser;
use clap::Subcommand;
use std::collections::BTreeMap;
use std::io::Error;
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
    name: String,
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
    Logger::init().map_err(|e| Error::other(format!("Failed to init logger: {}", e)))?;
    let args = Args::parse();
    let config = Config::open(&args.config_dir)?;
    match args.command {
        Command::Pull => pull(config),
        Command::Search(more_args) => search(config, more_args),
        Command::Install(more_args) => install(config, more_args),
    }
}

fn pull(config: Config) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let _guard = rt.enter();
    rt.block_on(async {
        let mut repos = config
            .repos
            .into_iter()
            .map(|(name, repo_config)| (name, <dyn Repo>::new(repo_config)))
            .collect::<BTreeMap<_, _>>();
        for (name, repo) in repos.iter_mut() {
            repo.pull(config.store_dir.as_path(), name.as_str()).await?;
        }
        Ok(ExitCode::SUCCESS)
    })
}

fn search(config: Config, args: SearchArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut repos = config
        .repos
        .into_iter()
        .map(|(name, repo_config)| (name, <dyn Repo>::new(repo_config)))
        .collect::<BTreeMap<_, _>>();
    let keyword = args.keyword.to_lowercase();
    for (name, repo) in repos.iter_mut() {
        repo.search(config.store_dir.as_path(), name.as_str(), &keyword)?;
    }
    Ok(ExitCode::SUCCESS)
}

fn install(
    _config: Config,
    _install_args: InstallArgs,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    Ok(ExitCode::SUCCESS)
}
