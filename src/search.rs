// `warp-to` Copyright (C) 2026, c272
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License v3 as published by the Free
// Software Foundation.
//
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;

use crate::config::Config;
use crate::fs;
use crate::walker::DirectoryWalker;

const ROOT_CHAR: char = '/';
const HOME_CHAR: char = '~';
const SEPARATOR_CHAR: char = '/';
const SHORTCUT_CHAR: char = '+';
const CWD_CHAR: char = '.';

/// A single concrete directory structure to search for.
#[derive(Debug)]
pub(crate) struct SearchStructure(Vec<String>);

impl<I> From<I> for SearchStructure
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    fn from(value: I) -> Self {
        Self(value.into_iter().map(|i| i.into()).collect())
    }
}

impl SearchStructure {
    pub fn empty() -> Self {
        Self(vec![])
    }

    pub fn parse(input: &str) -> Self {
        let structure = input.split(SEPARATOR_CHAR).filter(|p| !p.is_empty()).into();
        return structure;
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AbsolutePathBase {
    Root,
    Home,
}

/// A single group of search elements. This represents one command-line argument provided by the user.
/// For example, this could be a single atom ("." and "/") or multiple atoms chained together in a single
/// argument, representing a concrete directory structure ("some/thing/concrete").
#[derive(Debug)]
pub(crate) enum SearchGroup {
    /// An absolute search path.
    Absolute {
        base: AbsolutePathBase,
        structure: SearchStructure,
    },
    /// A search path originating from a shortcut.
    Shortcut {
        name: String,
        structure: SearchStructure,
    },
    /// Up N directories from the CWD.
    JumpUp(u32),
    /// A fuzzy search.
    Fuzzy(SearchStructure),
}

impl From<OsString> for SearchGroup {
    fn from(value: OsString) -> Self {
        let value_str = value.into_string().unwrap();

        // Standalone root/home.
        if value_str.len() == 1 {
            let first_char = value_str.chars().nth(0).unwrap();
            if first_char == ROOT_CHAR {
                return Self::new_absolute(AbsolutePathBase::Root, SearchStructure::empty());
            } else if first_char == HOME_CHAR {
                return Self::new_absolute(AbsolutePathBase::Home, SearchStructure::empty());
            }
        }

        // Root/home base with structure.
        if value_str.starts_with(ROOT_CHAR) {
            let structure = SearchStructure::parse(&value_str[1..]);
            return Self::new_absolute(AbsolutePathBase::Root, structure);
        }

        if value_str.starts_with(HOME_CHAR) {
            let structure = SearchStructure::parse(&value_str[1..]);
            return Self::new_absolute(AbsolutePathBase::Home, structure);
        }

        if value_str.starts_with(SHORTCUT_CHAR) {
            let slash_idx_opt = value_str.find(SEPARATOR_CHAR);

            let structure = if let Some(slash_idx) = &slash_idx_opt {
                SearchStructure::parse(&value_str[*slash_idx..])
            } else {
                SearchStructure::empty()
            };

            let name_end_idx = slash_idx_opt.unwrap_or(value_str.len());
            let name = value_str[1..name_end_idx].to_string();

            return Self::new_shortcut(name, structure);
        }

        if value_str.len() == 2 && value_str.starts_with(CWD_CHAR) {
            let second_char = value_str.chars().nth(1).unwrap();

            // Check if there's a valid digit for this to be a relative jump up.
            let digit = second_char.to_digit(10).or_else(|| {
                if second_char == CWD_CHAR {
                    Some(1)
                } else {
                    None
                }
            });
            if let Some(digit) = digit {
                return Self::new_jump_up(digit);
            }
        }

        let structure = SearchStructure::parse(&value_str);
        return Self::new_fuzzy(structure);
    }
}

impl SearchGroup {
    fn new_fuzzy(structure: SearchStructure) -> Self {
        Self::Fuzzy(structure)
    }

    fn new_absolute(base: AbsolutePathBase, structure: SearchStructure) -> Self {
        Self::Absolute { base, structure }
    }

    fn new_shortcut(name: String, structure: SearchStructure) -> Self {
        Self::Shortcut { name, structure }
    }

    fn new_jump_up(n: u32) -> Self {
        Self::JumpUp(n)
    }
}

/// Types of directory search to perform.
#[derive(Debug)]
pub(crate) enum Search {
    /// Search for the root directory, relative to the CWD.
    Root,
    /// Search for the home directory.
    Home,
    /// Search by a set of search groups.
    Groups {
        /// The groups to search for.
        groups: Vec<SearchGroup>,
        /// The maximum distance to use when searching.
        max_dist: usize,
    },
}

impl Search {
    pub fn new(args: Vec<OsString>, max_dist: usize) -> Self {
        if args.is_empty() {
            return Search::Home;
        }

        if args.len() == 1 {
            let arg = args[0].to_str().unwrap();

            if arg.len() == 1 {
                let first_char = arg.chars().nth(0).unwrap();
                if first_char == ROOT_CHAR {
                    return Search::Root;
                } else if first_char == HOME_CHAR {
                    return Search::Home;
                }
            }
        }

        let groups = args.into_iter().map(|arg| arg.into()).collect();

        return Self::Groups { groups, max_dist };
    }
}

/// Configuration options for the search runner.
#[derive(Debug)]
pub(crate) struct SearchRunnerOpts {
    /// Whether to respect user-defined ignores while searching.
    pub use_ignores: bool,
}

/// Utility structure used for executing individual searches.
/// Holds inter-search state.
pub(crate) struct SearchRunner {
    /// Configuration options.
    opts: SearchRunnerOpts,
    /// User-defined config for `warp-to`.
    config: Option<Rc<Config>>,
    /// The CWD used by the runner.
    cwd: PathBuf,
    /// The maximum distance used by the runner.
    max_distance: usize,
}

impl SearchRunner {
    pub fn new(opts: SearchRunnerOpts) -> Self {
        Self {
            opts,
            config: None,
            cwd: PathBuf::new(),
            max_distance: 0,
        }
    }

    /// Executes a single search.
    ///
    /// Upon success, returns the found directory.
    /// Upon failure, returns an error message.
    pub fn run(&mut self, search: Search) -> Result<String, String> {
        let path = match search {
            Search::Root => fs::fetch_root()?,
            Search::Home => fs::fetch_home()?,
            Search::Groups { groups, max_dist } => self.search_by_groups(&groups, max_dist)?,
        };
        let path_str = path
            .to_str()
            .ok_or("Found path was invalid UTF-8.".to_string())?;
        Ok(path_str.into())
    }

    fn search_by_groups(
        &mut self,
        groups: &[SearchGroup],
        max_dist: usize,
    ) -> Result<PathBuf, String> {
        self.max_distance = max_dist;

        let cur_dir = fs::get_cwd()?;
        self.cwd = cur_dir.clone();

        let matching_path = self.search_by_groups_inner(cur_dir, groups, true, max_dist)?;
        match matching_path {
            Some(path) => Ok(path),
            None => Err(format!("Nothing found matching the given groups.")),
        }
    }

    fn search_by_groups_inner(
        &mut self,
        cur_dir: PathBuf,
        rem_groups: &[SearchGroup],
        search_up: bool,
        rem_dist: usize,
    ) -> Result<Option<PathBuf>, String> {
        if rem_groups.is_empty() {
            // Nothing more to search.
            return Ok(Some(cur_dir.to_path_buf()));
        }

        if rem_dist == 0 {
            // Cannot search any further.
            return Ok(None);
        }

        let group = rem_groups.first().unwrap();
        let next_rem_groups = &rem_groups[1..];

        match group {
            SearchGroup::Absolute { base, structure } => {
                let dir = Self::search_for_group_absolute(*base, structure)?;
                self.search_by_groups_inner(dir, next_rem_groups, false, rem_dist)
            }
            SearchGroup::Shortcut { name, structure } => {
                let dir = self.search_for_group_shortcut(name, structure)?;
                self.search_by_groups_inner(dir, next_rem_groups, false, rem_dist)
            }
            SearchGroup::JumpUp(n) => {
                let dir = Self::search_for_group_jump_up(*n)?;
                self.search_by_groups_inner(dir, next_rem_groups, false, rem_dist)
            }
            SearchGroup::Fuzzy(structure) => {
                self.search_by_groups_fuzzy(cur_dir, structure, rem_groups, search_up, rem_dist)
            }
        }
    }

    fn search_by_groups_fuzzy(
        &mut self,
        cur_dir: PathBuf,
        structure: &SearchStructure,
        rem_groups: &[SearchGroup],
        search_up: bool,
        rem_dist: usize,
    ) -> Result<Option<PathBuf>, String> {
        if structure.is_empty() {
            // Nothing more to search.
            return Ok(Some(cur_dir.to_path_buf()));
        }

        // Create the directory walker.
        let mut _config_rc = None;
        let mut walker = DirectoryWalker::new(cur_dir, rem_dist)
            .include_start_dir(false)
            .walk_upward(search_up);

        // If we are using the ignorelist, load and configure that.
        if self.opts.use_ignores {
            _config_rc = Some(self.get_or_load_config()?);
            walker = walker.ignores(&_config_rc.as_ref().unwrap().ignore);
        }

        let first_elem = structure.0.first().unwrap();
        let remaining_elems = &structure.0[1..];

        for dir in walker.into_iter() {
            let Some(dir_name) = dir.dir_name() else {
                continue; // No directory name (volume label etc.).
            };

            if !dir_name.eq_ignore_ascii_case(&first_elem) {
                continue; // No match.
            }

            // First element match! Check if the entire group matches.
            let dir_str = dir.path().to_str().unwrap();
            let structure_path = PathBuf::from_iter(
                [dir_str]
                    .into_iter()
                    .chain(remaining_elems.iter().map(|e| e.as_str())),
            );
            if !structure_path.exists() {
                continue;
            }

            // Group match! Recursively try the next group along.
            let matching_path =
                self.search_by_groups_inner(structure_path, &rem_groups[1..], false, rem_dist - 1)?;
            if let Some(path) = matching_path {
                return Ok(Some(path));
            }
        }

        Ok(None)
    }

    /// Finds an absolute path based group.
    fn search_for_group_absolute(
        base: AbsolutePathBase,
        structure: &SearchStructure,
    ) -> Result<PathBuf, String> {
        let base_path = match base {
            AbsolutePathBase::Home => fs::fetch_home()?,
            AbsolutePathBase::Root => fs::fetch_root()?,
        };
        let base_path_str = base_path.to_str().expect("Path was invalid UTF-8.");

        let path = PathBuf::from_iter(
            [base_path_str]
                .into_iter()
                .chain(structure.0.iter().map(|i| i.as_str())),
        );
        if !path.exists() {
            return Err(format!("No absolute path exists at '{}'.", path.display()));
        }
        Ok(path)
    }

    /// Finds a shortcut based group.
    fn search_for_group_shortcut(
        &mut self,
        name: &str,
        structure: &SearchStructure,
    ) -> Result<PathBuf, String> {
        let config = self.get_or_load_config()?;

        // If there is no configured shortcut which matches, error out.
        let Some(shortcut_str) = config.shortcuts.get(name) else {
            return Err(format!("No shortcut named '{}' configured.", name));
        };

        let path = PathBuf::from_iter(
            [shortcut_str.as_str()]
                .into_iter()
                .chain(structure.0.iter().map(|i| i.as_str())),
        );

        if !path.exists() {
            return Err(format!("No valid path exists at '{}'.", path.display()));
        }

        Ok(path)
    }

    /// Fetches the currently loaded config, or reads it from disk if not currently loaded.
    fn get_or_load_config(&mut self) -> Result<Rc<Config>, String> {
        if self.config.is_none() {
            let config = Config::create_or_load()?;
            self.config = Some(Rc::new(config));
        }
        Ok(self.config.as_ref().unwrap().clone())
    }

    /// Finds a group based on a relative jump up from the CWD.
    fn search_for_group_jump_up(n: u32) -> Result<PathBuf, String> {
        Ok(fs::fetch_ancestor(n)?)
    }
}
