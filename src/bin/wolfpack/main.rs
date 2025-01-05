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
    Install(InstallArgs),
}

#[derive(clap::Args)]
struct InstallArgs {
    name: String,
}

fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    do_main().inspect_err(|e| eprintln!("{e}"))
}

fn do_main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    Logger::init().map_err(|e| Error::other(format!("Failed to init logger: {}", e)))?;
    let args = Args::parse();
    let config = Config::open(&args.config_dir)?;
    match args.command {
        Command::Install(more_args) => install(config, more_args),
    }
}

fn install(
    config: Config,
    install_args: InstallArgs,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
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
        println!("Hello world");
        Ok(ExitCode::SUCCESS)
    })
}
