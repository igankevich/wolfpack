use std::process::ExitCode;

use clap::ValueEnum;

use crate::Config;

#[derive(clap::Args)]
pub struct SearchArgs {
    /// Which field to search by.
    #[clap(short = 'b', long = "by", value_name = "BY", default_value = "keyword")]
    by: SearchBy,
    /// Search query.
    #[clap(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "KEYWORD"
    )]
    query: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SearchBy {
    Keyword,
    File,
    Command,
}

pub fn search(
    mut config: Config,
    args: SearchArgs,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut repos = config.take_repos();
    let query = args.query.join(" ");
    for (name, repo) in repos.iter_mut() {
        repo.search(&config, name.as_str(), args.by, &query)?;
    }
    Ok(ExitCode::SUCCESS)
}
