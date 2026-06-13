use std::process::ExitCode;

use crate::args::Command;

mod args;
mod search;

const HELP: &str = "TODO";

fn main() -> ExitCode {
    let command = args::parse_args(std::env::args_os()).unwrap();

    match command {
        Command::Help => {
          println!("{}", HELP);
          ExitCode::FAILURE
        },
        Command::Search(search) => {
          match search.run() {
            Ok(dir) => {
              println!("{}", dir);
              ExitCode::SUCCESS
            },
            Err(err) => {
              eprintln!("warp-to: {}", err);
              ExitCode::FAILURE
            }
          }
        }
    }
}
