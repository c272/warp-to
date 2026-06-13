// `warp-to` Copyright (C) 2026, c272
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License v3 as published by the Free
// Software Foundation.
//
use std::ffi::OsString;

use crate::search::Search;

/// The command to be performed on a single execution of `warp-to`.
#[derive(Debug)]
pub enum Command {
    /// Prints the help menu.
    Help,
    /// Performs a search & navigate.
    Search(Search),
}

/// Parses command line arguments and finds a command to be performed.
pub fn parse_args<I>(args: I) -> Result<Command, lexopt::Error>
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    use lexopt::prelude::*;

    let mut values: Vec<OsString> = Vec::new();
    let mut max_dist: usize = 5;

    let mut parser = lexopt::Parser::from_iter(args);
    while let Some(arg) = parser.next()? {
        match arg {
            Value(val) => {
                values.push(val);
            }
            Short('d') | Long("distance") => {
                let dist = parser.value()?.parse::<usize>()?;
                max_dist = dist;
            }
            Long("help") => {
                return Ok(Command::Help);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    let search = Search::new(values, max_dist);

    Ok(Command::Search(search))
}
