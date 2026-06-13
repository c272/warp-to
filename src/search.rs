use std::collections::HashSet;
use std::ffi::OsString;
use std::hash::Hash;
use std::os::windows::fs::FileTypeExt;
use std::path::PathBuf;
use std::path::Path;

use walkdir::WalkDir;

const ROOT_CHAR: &'static str = "/";
const HOME_CHAR: &'static str = "~";

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
        let structure = input.split('/').filter(|p| !p.is_empty()).into();
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
  Absolute {
    base: AbsolutePathBase,
    structure: SearchStructure
  },
  Fuzzy(SearchStructure),
}

impl From<OsString> for SearchGroup {
    fn from(value: OsString) -> Self {
        let value_str = value.into_string().unwrap();

        // Standalone root/home.
        if value_str == ROOT_CHAR {
            return Self::new_absolute(AbsolutePathBase::Root, SearchStructure::empty());
        } else if value_str == HOME_CHAR {
            return Self::new_absolute(AbsolutePathBase::Home, SearchStructure::empty());
        }

        // Root/home base with structure.
        if value_str.starts_with(ROOT_CHAR) {
            let structure = SearchStructure::parse(&value_str[1..]);
            return Self::new_absolute(AbsolutePathBase::Root, structure);
        } else if value_str.starts_with(HOME_CHAR) {
            let structure = SearchStructure::parse(&value_str[1..]);
            return Self::new_absolute(AbsolutePathBase::Home, structure);
        }

        let structure = SearchStructure::parse(&value_str);
        return Self::new_fuzzy(structure);
    }
}

impl std::fmt::Display for SearchGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      // Header.
      if let SearchGroup::Absolute { base, structure } = self {
          let base_str = match base {
            AbsolutePathBase::Root => "/",
            AbsolutePathBase::Home => if structure.is_empty() { "~" } else { "~/"},
          };
          write!(f, "{}", base_str)?;
      }

      let structure = match self {
        SearchGroup::Absolute { base: _base, structure } => structure,
        SearchGroup::Fuzzy(s) => s
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
}

/// Types of directory search to perform.
#[derive(Debug)]
pub(crate) enum Search {
    /// Search for the root directory, relative to the CWD.
    Root,
    /// Search for the home directory.
    Home,
    /// Search by a set of search groups.
    Groups(Vec<SearchGroup>),
}

impl Search {
    /// Executes a single search.
    ///
    /// Upon success, returns the found directory.
    /// Upon failure, returns an error message.
    pub fn run(&self) -> Result<String, String> {
        let path = match self {
            Self::Root => fetch_root()?,
            Self::Home => fetch_home()?,
            Self::Groups(groups) => search_by_groups(groups)?,
        };
        let path_str = path.to_str().ok_or("Found path was invalid UTF-8.".to_string())?;
        Ok(path_str.into())
    }
}

impl From<Vec<OsString>> for Search {
    fn from(args: Vec<OsString>) -> Self {
        if args.is_empty() {
            return Search::Home;
        }

        if args.len() == 1 {
            let arg = &args[0];
            if arg == ROOT_CHAR {
                return Search::Root;
            } else if arg == HOME_CHAR {
                return Search::Home;
            }
        }

        let groups = args.into_iter().map(|arg| arg.into()).collect();

        return Self::Groups(groups);
    }
}

fn get_cwd() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir();
    match cwd {
        Ok(dir) => Ok(dir),
        Err(_) => Err("Failed to find current working directory.".into()),
    }
}

#[cfg(target_os = "windows")]
fn fetch_root() -> Result<PathBuf, String> {
    let cwd = get_cwd()?;
    if !cwd.has_root() {
        return Err("No root found from current working directory.".into());
    }

    let mut components = cwd.components();
    let prefix = components.next();
    let root = components.next();

    match (prefix, root) {
        (Some(std::path::Component::Prefix(p)), Some(std::path::Component::RootDir)) => {
            let mut root_path = std::path::PathBuf::new();
            root_path.push(p.as_os_str());
            root_path.push("\\");
            Ok(root_path)
        }
        _ => Err("No prefix/root component found from working directory.".into()),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn fetch_root() -> Result<PathBuf, String> {
    Ok(PathBuf::from(ROOT_CHAR))
}

#[cfg(target_os = "windows")]
fn fetch_home() -> Result<PathBuf, String> {
    todo!()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn fetch_home() -> Result<PathBuf, String> {
  Ok(PathBuf::from(HOME_CHAR))
}

#[derive(Clone, Copy)]
struct SearchOptions {
  /// The maximum distance (upward, downward) to search to.
  max_distance: usize,
  /// In addition to a downward search, also search the contents of ancestors
  /// of the starting directory in sequence.
  search_up: bool,
}

struct GroupSearchResult {
  /// The path which matched the search.
  path: PathBuf,
  /// The distance score assigned to this search result. Lower is closer.
  distance: usize
}

impl PartialEq for GroupSearchResult {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for GroupSearchResult {}

impl Hash for GroupSearchResult {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.path.hash(state)
    }
}

fn search_by_groups(groups: &[SearchGroup]) -> Result<PathBuf, String> {
    const MAX_DIST: usize = 5;

    let cur_dir = get_cwd()?;
    if groups.is_empty() {
      return Ok(cur_dir);
    }

    let first_group = groups.first().unwrap();
    let mut candidates: HashSet<GroupSearchResult> = search_for_group(first_group, &cur_dir, SearchOptions { max_distance: MAX_DIST, search_up: true })?.into_iter().collect();

    if candidates.is_empty() {
      return Err(format!("Nothing found matching '{}'.", first_group))
    }

    // Iterate all groups and find paths which match from the existing candidates.
    // Now we only search downwards, since we've found all candidate starting paths.
    for group in groups.iter().skip(1) {
      let mut new_candidates = HashSet::new();

      for candidate in &candidates {
        let search_options = SearchOptions { max_distance: MAX_DIST - candidate.distance, search_up: true };
        let mut results = search_for_group(group, &candidate.path, search_options)?;
        for result in results.iter_mut() {
          result.distance += candidate.distance;
        }

        new_candidates.extend(results);
      }

      candidates = new_candidates;
    }

    // Find the remaining candidate with the lowest distance.
    let winner = candidates.into_iter().min_by_key(|c| c.distance).ok_or(format!("Nothing found matching the given search groups."))?;
    Ok(winner.path)
}

/// Searches for any directories matching the given group, starting from the provided path.
fn search_for_group(group: &SearchGroup, start: &PathBuf, search_options: SearchOptions) -> Result<Vec<GroupSearchResult>, String> {
  match group {
    SearchGroup::Absolute { base, structure } => {
      search_for_group_absolute(*base, structure)
    },
    SearchGroup::Fuzzy(structure) => {
      Ok(search_for_group_fuzzy(start, structure, search_options))
    }
  }
}

fn search_for_group_absolute(base: AbsolutePathBase, structure: &SearchStructure) -> Result<Vec<GroupSearchResult>, String> {
  let base_path = match base {
    AbsolutePathBase::Home => fetch_home()?,
    AbsolutePathBase::Root => fetch_root()?
  };
  let base_path_str = base_path.to_str().expect("Path was invalid UTF-8.");

  let path = PathBuf::from_iter([base_path_str].into_iter().chain(structure.0.iter().map(|i| i.as_str())));
  if !path.exists() {
    return Err(format!("No absolute path exists at '{}'.", path.display()));
  }
  Ok(vec![GroupSearchResult{ path, distance: 0 }])
}

fn search_for_group_fuzzy(start: &PathBuf, structure: &SearchStructure, search_options: SearchOptions) -> Vec<GroupSearchResult> {
  // First, find candidates below the start directory.
  let mut candidates = Vec::new();
  candidates.extend(walk_and_find_candidates(start, None, structure, 0, search_options.max_distance));

  // Next, if configured, find candidates that begin above the start directory.
  if search_options.search_up {
    let max_dist_up = search_options.max_distance - 1;
    let mut cur_path = start.as_path();

    for dist_up in 1..max_dist_up {
      match cur_path.parent() {
        Some(parent) => { cur_path = parent; },
        None => { break;}
      }

      let max_dist_down = search_options.max_distance - dist_up;
      candidates.extend(walk_and_find_candidates(cur_path, Some(start), structure, dist_up, max_dist_down));
    }
  }

  candidates
}

fn walk_and_find_candidates(start: &Path, ignore: Option<&PathBuf>, structure: &SearchStructure, cur_depth: usize, max_depth: usize) -> Vec<GroupSearchResult> {
  // If there's no structure, we don't do anything.
  if structure.is_empty() {
    return vec![];
  }

  let first_elem = OsString::from(structure.0.first().unwrap());
  let mut candidates = Vec::new();

  // Search through lower directories first to find candidates.
  let walker = WalkDir::new(start).follow_links(true).min_depth(1).max_depth(max_depth);
  let iter = walker.into_iter()
    .filter_entry(|e| ignore.is_none_or(|ignore_path| e.path() != ignore_path))
    .filter_map(|e| e.ok())
    .filter(|d| d.file_type().is_dir() || d.file_type().is_symlink_dir());

  for dir in iter {
    if dir.file_name().eq_ignore_ascii_case(&first_elem) {
        let depth = dir.depth();
        candidates.push(GroupSearchResult { path: dir.into_path(), distance: cur_depth + depth });
      }
  }

  // Extend all candidates with the remaining search group members.
  for candidate in candidates.iter_mut() {
    candidate.path.extend(structure.0.iter().skip(1));
  }

  // Eliminate candidates which do not match the rest of the search group.
  candidates.into_iter().filter(|c| c.path.exists()).collect()
}
