use crate::args::Command;

mod args;
mod search;

fn main() {
    let command = args::parse_args(std::env::args_os()).unwrap();

    match command {
        Command::Help => todo!(),
        Command::Search(search) => {
            let dir = search.run().unwrap();
            println!("{}", dir);
        }
    }
}
