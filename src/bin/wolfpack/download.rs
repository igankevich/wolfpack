use std::process::ExitCode;

use crate::Config;

#[derive(clap::Args)]
pub struct DownloadArgs {
    #[clap(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "PACKAGE"
    )]
    packages: Vec<String>,
    // TODO recursive, with dependencies
}

pub fn download(
    mut config: Config,
    args: DownloadArgs,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let _guard = rt.enter();
    rt.block_on(async {
        let mut repos = config.take_repos();
        for (name, repo) in repos.iter_mut() {
            let files = repo
                .download(&config, name.as_str(), args.packages.clone())
                .await?;
            for file in files {
                println!("{}", file.display());
            }
        }
        Ok(ExitCode::SUCCESS)
    })
}
