use std::ffi::OsString;

use lexopt::ValueExt;

const ROOT_CHAR: &'static str = "/";
const HOME_CHAR: &'static str = "~";

/// A single concrete directory structure to search for.
#[derive(Debug)]
struct SearchStructure(Vec<String>);

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
}

#[derive(Debug)]
enum SearchBase {
    Root,
    Home,
    Unspecified,
}

/// A single group of search elements. This represents one command-line argument provided by the user.
/// For example, this could be a single atom ("." and "/") or multiple atoms chained together in a single
/// argument, representing a concrete directory structure ("some/thing/concrete").
#[derive(Debug)]
struct SearchGroup {
    base: SearchBase,
    structure: SearchStructure,
}

impl From<OsString> for SearchGroup {
    fn from(value: OsString) -> Self {
        let value_str = value.string().unwrap();

        // Standalone root/home.
        if value_str == ROOT_CHAR {
            return Self::new(SearchBase::Root, SearchStructure::empty());
        } else if value_str == HOME_CHAR {
            return Self::new(SearchBase::Home, SearchStructure::empty());
        }

        // Root/home base with structure.
        if value_str.starts_with(ROOT_CHAR) {
            let structure = SearchStructure::parse(&value_str[1..]);
            return Self::new(SearchBase::Root, structure);
        } else if value_str.starts_with(HOME_CHAR) {
            let structure = SearchStructure::parse(&value_str[1..]);
            return Self::new(SearchBase::Home, structure);
        }

        let structure = SearchStructure::parse(&value_str);
        return Self::new(SearchBase::Unspecified, structure);
    }
}

impl SearchGroup {
    fn new(base: SearchBase, structure: SearchStructure) -> Self {
        Self { base, structure }
    }
}

/// Types of directory search to perform.
#[derive(Debug)]
pub enum Search {
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
        match self {
            Self::Root => return Ok(ROOT_CHAR.into()),
            _ => todo!(),
        }
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
