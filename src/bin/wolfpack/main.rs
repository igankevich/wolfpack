mod builder;
mod config;
mod db;
mod deb;
mod do_main;
mod download;
mod error;
mod key;
mod logger;
mod project_builder;
mod repo;
mod table;

use self::builder::*;
use self::config::*;
use self::do_main::*;
use self::download::*;
use self::error::*;
use self::key::*;
use self::logger::*;
use self::project_builder::*;
use self::repo::*;
use self::table::*;

use std::process::ExitCode;

fn main() -> ExitCode {
    do_main()
        .inspect_err(|e| eprintln!("{e}"))
        .unwrap_or(ExitCode::FAILURE)
}
