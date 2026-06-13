use std::collections::HashSet;
use std::ffi::OsString;
use std::hash::Hash;
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

/// One result of searching for a directory with the search query system.
/// Contains a buffer with the matching path, as well as its distance from the query root.
struct SearchResult {
    /// The path which matched the search.
    path: PathBuf,
    /// The distance score assigned to this search result. Lower is closer.
    distance: usize,
}

impl PartialEq for SearchResult {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for SearchResult {}

impl Hash for SearchResult {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.path.hash(state)
    }
}

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

    fn walk_and_find_candidates(
        &self,
        start: &Path,
        ignore: Option<&PathBuf>,
        cur_depth: usize,
        max_depth: usize,
    ) -> Vec<SearchResult> {
        // If there's no structure, we don't do anything.
        if self.is_empty() {
            return vec![];
        }

        let first_elem = OsString::from(self.0.first().unwrap());
        let mut candidates = Vec::new();

        // Do not include the start directory itself if it's the CWD.
        let min_depth = if cur_depth > 0 { 0 } else { 1 };

        // Search through lower directories first to find candidates.
        let walker = WalkDir::new(start)
            .follow_links(true)
            .min_depth(min_depth)
            .max_depth(max_depth);
        let iter = walker
            .into_iter()
            .filter_entry(|e| ignore.is_none_or(|ignore_path| e.path() != ignore_path))
            .filter_map(|e| e.ok())
            .filter(|d| d.file_type().is_dir() || d.file_type().is_symlink_dir());

        for dir in iter {
            if dir.file_name().eq_ignore_ascii_case(&first_elem) {
                let depth = dir.depth();
                candidates.push(SearchResult {
                    path: dir.into_path(),
                    distance: cur_depth + depth,
                });
            }
        }

        // Extend all candidates with the remaining search group members.
        for candidate in candidates.iter_mut() {
            candidate.path.extend(self.0.iter().skip(1));
        }

        // Eliminate candidates which do not match the rest of the search group.
        candidates.into_iter().filter(|c| c.path.exists()).collect()
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
    Absolute {
        base: AbsolutePathBase,
        structure: SearchStructure,
    },
    Shortcut {
        name: String,
        structure: SearchStructure,
    },
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
        } else if value_str.starts_with(HOME_CHAR) {
            let structure = SearchStructure::parse(&value_str[1..]);
            return Self::new_absolute(AbsolutePathBase::Home, structure);
        } else if value_str.starts_with(SHORTCUT_CHAR) {
            let slash_idx_opt = value_str.find(SEPARATOR_CHAR);

            let structure = if let Some(slash_idx) = &slash_idx_opt {
              SearchStructure::parse(&value_str[*slash_idx..])
            } else { SearchStructure::empty() };

            let name_end_idx = slash_idx_opt.unwrap_or(value_str.len());
            let name = value_str[1..name_end_idx].to_string();

            return Self::new_shortcut(name, structure);
        }

        let structure = SearchStructure::parse(&value_str);
        return Self::new_fuzzy(structure);
    }
}

impl std::fmt::Display for SearchGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Header.
        match self {
            SearchGroup::Absolute { base, structure } => {
                let base_str = match base {
                    AbsolutePathBase::Root => "/",
                    AbsolutePathBase::Home => {
                        if structure.is_empty() {
                            "~"
                        } else {
                            "~/"
                        }
                    }
                };
                write!(f, "{}", base_str)?;
            }
            SearchGroup::Shortcut { name, structure } => {
                write!(f, "@{}", name)?;
                if !structure.is_empty() {
                    write!(f, "/")?;
                }
            }
            SearchGroup::Fuzzy(_) => {}
        }

        let structure = match self {
            SearchGroup::Absolute {
                base: _base,
                structure,
            } => structure,
            SearchGroup::Shortcut {
                name: _name,
                structure,
            } => structure,
            SearchGroup::Fuzzy(s) => s,
        };

        for (idx, elem) in structure.0.iter().enumerate() {
            write!(f, "{}", elem)?;
            if idx < structure.0.len() - 1 {
                write!(f, ",")?;
            }
        }

        Ok(())
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
}

/// Types of directory search to perform.
#[derive(Debug)]
pub(crate) enum Search {
    /// Search for the root directory, relative to the CWD.
    Root,
    /// Search for the home directory.
    Home,
    /// Move up N directories.
    Up(u32),
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

            if Self::is_up_jump_arg(arg) {
                return Search::Up(arg.chars().nth(1).unwrap().to_digit(10).unwrap());
            }
        }

        let groups = args.into_iter().map(|arg| arg.into()).collect();

        return Self::Groups { groups, max_dist };
    }

    /// Determines whether a given string is a relative jump (.N) argument.
    fn is_up_jump_arg(arg: &str) -> bool {
        if arg.len() != 2 {
            return false;
        }
        return arg.chars().nth(0).unwrap() == '.' && arg.chars().nth(1).unwrap().is_digit(10);
    }
}

/// Utility structure used for executing individual searches.
/// Holds inter-search state.
pub(crate) struct SearchRunner {
    /// User-defined config for `warp-to`.
    config: Option<Config>,
}

impl SearchRunner {
    pub fn new() -> Self {
        Self { config: None }
    }

    /// Executes a single search.
    ///
    /// Upon success, returns the found directory.
    /// Upon failure, returns an error message.
    pub fn run(&mut self, search: Search) -> Result<String, String> {
        let path = match search {
            Search::Root => fs::fetch_root()?,
            Search::Home => fs::fetch_home()?,
            Search::Up(n) => fs::fetch_ancestor(n)?,
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
        let cur_dir = fs::get_cwd()?;
        if groups.is_empty() {
            return Ok(cur_dir);
        }

        let first_group = groups.first().unwrap();
        let mut candidates: HashSet<SearchResult> = self
            .search_for_group(
                first_group,
                &cur_dir,
                SearchOptions {
                    max_distance: max_dist,
                    search_up: true,
                },
            )?
            .into_iter()
            .collect();

        if candidates.is_empty() {
            return Err(format!("Nothing found matching '{}'.", first_group));
        }

        // Iterate all groups and find paths which match from the existing candidates.
        // Now we only search downwards, since we've found all candidate starting paths.
        for group in groups.iter().skip(1) {
            let mut new_candidates = HashSet::new();

            for candidate in &candidates {
                let search_options = SearchOptions {
                    max_distance: max_dist - candidate.distance,
                    search_up: false,
                };
                let mut results = self.search_for_group(group, &candidate.path, search_options)?;
                for result in results.iter_mut() {
                    result.distance += candidate.distance;
                }

                new_candidates.extend(results);
            }

            candidates = new_candidates;
        }

        // Find the remaining candidate with the lowest distance.
        let winner = candidates
            .into_iter()
            .min_by_key(|c| c.distance)
            .ok_or(format!("Nothing found matching the given search groups."))?;
        Ok(winner.path)
    }

    /// Searches for any directories matching the given group, starting from the provided path.
    fn search_for_group(
        &mut self,
        group: &SearchGroup,
        start: &PathBuf,
        search_options: SearchOptions,
    ) -> Result<Vec<SearchResult>, String> {
        match group {
            SearchGroup::Absolute { base, structure } => {
                self.search_for_group_absolute(*base, structure)
            }
            SearchGroup::Shortcut { name, structure } => {
                self.search_for_group_shortcut(name, structure)
            }
            SearchGroup::Fuzzy(structure) => Ok(Self::search_for_group_fuzzy(
                start,
                structure,
                search_options,
            )),
        }
    }

    /// Finds an absolute path based group.
    fn search_for_group_absolute(
        &mut self,
        base: AbsolutePathBase,
        structure: &SearchStructure,
    ) -> Result<Vec<SearchResult>, String> {
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
        Ok(vec![SearchResult { path, distance: 0 }])
    }

    /// Finds a shortcut based group.
    fn search_for_group_shortcut(
        &mut self,
        name: &str,
        structure: &SearchStructure,
    ) -> Result<Vec<SearchResult>, String> {
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

        Ok(vec![SearchResult { path, distance: 0 }])
    }

    /// Finds possible matches for a fuzzy-search group.
    fn search_for_group_fuzzy(
        start: &PathBuf,
        structure: &SearchStructure,
        search_options: SearchOptions,
    ) -> Vec<SearchResult> {
        // First, find candidates below the start directory.
        let mut candidates = Vec::new();
        candidates.extend(structure.walk_and_find_candidates(
            start,
            None,
            0,
            search_options.max_distance,
        ));

        // Next, if configured, find candidates that begin above the start directory.
        if search_options.search_up {
            let mut cur_path = start.as_path();

            for dist_up in 1..=search_options.max_distance {
                match cur_path.parent() {
                    Some(parent) => {
                        cur_path = parent;
                    }
                    None => {
                        break;
                    }
                }

                let max_dist_down = search_options.max_distance - dist_up;
                candidates.extend(structure.walk_and_find_candidates(
                    cur_path,
                    Some(start),
                    dist_up,
                    max_dist_down,
                ));
            }
        }

        candidates
    }
}

#[derive(Clone, Copy)]
struct SearchOptions {
    /// The maximum distance (upward, downward) to search to.
    max_distance: usize,
    /// In addition to a downward search, also search the contents of ancestors
    /// of the starting directory in sequence.
    search_up: bool,
}
