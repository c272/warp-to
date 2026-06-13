// `warp-to` Copyright (C) 2026, c272
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License v3 as published by the Free
// Software Foundation.
//
use std::ffi::OsString;
use std::os::windows::fs::FileTypeExt;
use std::path::Path;
use std::path::PathBuf;

use walkdir::WalkDir;

use crate::config::Config;
use crate::fs;

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

    fn create_walker(
        &self,
        start: &Path,
        ignore: Option<PathBuf>,
        cur_depth: usize,
        max_depth: usize,
    ) -> impl Iterator<Item = walkdir::DirEntry> {
        // Do not include the start directory itself if it's the CWD.
        let min_depth = if cur_depth > 0 { 0 } else { 1 };

        let walker = WalkDir::new(start)
            .follow_links(true)
            .min_depth(min_depth)
            .max_depth(max_depth);

        // TODO: We probably don't actually want depth-first search here.
        // Since our goal is to find the closest item (lowest distance), initially going to super high
        // depths is actually a detriment to us when returning the first viable match.
        // `walkdir` doesn't support breadth-first though, so changing it requires rolling something custom.
        walker
            .into_iter()
            .filter_entry(move |e| {
                ignore
                    .as_ref()
                    .is_none_or(|ignore_path| e.path() != ignore_path)
            })
            .filter_map(|e| e.ok())
            .filter(|d| d.file_type().is_dir() || d.file_type().is_symlink_dir())
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

/// Utility structure used for executing individual searches.
/// Holds inter-search state.
pub(crate) struct SearchRunner {
    /// User-defined config for `warp-to`.
    config: Option<Config>,
    /// The CWD used by the runner.
    cwd: PathBuf,
    /// The maximum distance used by the runner.
    max_distance: usize,
}

impl SearchRunner {
    pub fn new() -> Self {
        Self {
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

        let mut iters = vec![];
        let cur_dist = self.max_distance - rem_dist;
        let ignore_dir = if cur_dist == 0 {
            None
        } else {
            Some(self.cwd.clone())
        };
        iters.push(structure.create_walker(&cur_dir, ignore_dir, cur_dist, rem_dist));

        if search_up {
            // We should only search up at the very top layer of recursion.
            assert!(rem_dist == self.max_distance);
            let mut cur_up_path = cur_dir.as_path();

            // Add the requisite iterators for searching ancestor directories.
            for dist_up in 1..=rem_dist {
                match cur_up_path.parent() {
                    Some(parent) => {
                        cur_up_path = parent;
                    }
                    None => {
                        break;
                    }
                }

                let max_dist_down = rem_dist - dist_up;
                iters.push(structure.create_walker(
                    cur_up_path,
                    Some(self.cwd.clone()),
                    cur_dist + dist_up,
                    max_dist_down,
                ));
            }
        }

        let first_elem = structure.0.first().unwrap();
        let remaining_elems = &structure.0[1..];

        for iter in iters {
            for dir in iter {
                if !dir.file_name().eq_ignore_ascii_case(&first_elem) {
                    continue;
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
                let matching_path = self.search_by_groups_inner(
                    structure_path,
                    &rem_groups[1..],
                    false,
                    rem_dist - 1,
                )?;
                if let Some(path) = matching_path {
                    return Ok(Some(path));
                }
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
        // Load the config, if not already loaded.
        if self.config.is_none() {
            self.config = Some(Config::create_or_load()?);
        }
        let config = self.config.as_ref().unwrap();

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

    /// Finds a group based on a relative jump up from the CWD.
    fn search_for_group_jump_up(n: u32) -> Result<PathBuf, String> {
        Ok(fs::fetch_ancestor(n)?)
    }
}
