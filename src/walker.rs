// `warp-to` Copyright (C) 2026, c272
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License v3 as published by the Free
// Software Foundation.
//
use std::{
    collections::{HashSet, VecDeque},
    ffi::{OsStr, OsString},
    os::windows::fs::FileTypeExt,
    path::PathBuf,
};

/// A directory walker which enumerates a set of directory nodes, breadth-first.
/// Can also optionally search up the directory tree from the starting point.
pub(crate) struct DirectoryWalker<'c> {
    /// The starting point for the directory walker.
    start_dir: PathBuf,
    /// The maximum distance from the starting point to walk.
    max_dist: usize,
    /// Whether to include directories that are above the starting directory.
    /// If set to false, only subdirectories will be iterated. Default: false.
    walk_upward: bool,
    /// Whether to include the starting directory in the iteration. Default: true.
    include_start_dir: bool,
    /// Directory names to ignore while iterating.
    ignores: Option<&'c HashSet<OsString>>,
}

impl<'c> DirectoryWalker<'c> {
    /// Creates a new directory walker, beginning at the given directory.
    pub fn new(start_dir: PathBuf, max_dist: usize) -> Self {
        Self {
            start_dir,
            max_dist,
            walk_upward: false,
            include_start_dir: true,
            ignores: None,
        }
    }

    /// Sets whether the directory walker should also search upward.
    pub fn walk_upward(mut self, enabled: bool) -> Self {
        self.walk_upward = enabled;
        self
    }

    /// Sets whether to include the starting directory in the iteration.
    pub fn include_start_dir(mut self, enabled: bool) -> Self {
        self.include_start_dir = enabled;
        self
    }

    /// Sets directory names to ignore while iterating.
    pub fn ignores(mut self, ignores: &'c HashSet<OsString>) -> Self {
        self.ignores = Some(ignores);
        self
    }

    pub fn into_iter(self) -> WalkerIter<'c> {
        WalkerIter::new(
            WalkerIterOpts {
                max_dist: self.max_dist,
                walk_upward: self.walk_upward,
                include_start_dir: self.include_start_dir,
                ignores: self.ignores,
            },
            self.start_dir,
        )
    }
}

/// User-configurable options for a single [`WalkerIter`].
struct WalkerIterOpts<'c> {
    /// The maximum distance from the starting point to walk.
    max_dist: usize,
    /// Whether to include directories that are above the starting directory.
    walk_upward: bool,
    /// Whether to include the starting directory in the iteration. Default: true.
    include_start_dir: bool,
    /// Directories to ignore while iterating.
    ignores: Option<&'c HashSet<OsString>>,
}

/// An iterator for breadth-first walks of filesystems.
/// Should only be constructed from a [`DirectoryWalker`] instance.
pub(crate) struct WalkerIter<'c> {
    /// User-configurable options, set on the [`DirectoryWalker`].
    opts: WalkerIterOpts<'c>,
    /// The current queue of directories to walk.
    queue: VecDeque<DirEntry>,
    /// A set of ancestors which has already been queued.
    /// Used to avoid duplicate enumeration of ancestor directories.
    seen_ancestors: HashSet<PathBuf>,
}

impl<'c> WalkerIter<'c> {
    /// Creates a new [`WalkerIter`] from user configured options.
    fn new(opts: WalkerIterOpts<'c>, start: PathBuf) -> Self {
        let mut queue = VecDeque::with_capacity(32);
        queue.push_back(DirEntry::start_dir(start));

        Self {
            opts,
            queue,
            seen_ancestors: HashSet::new(),
        }
    }

    /// Queues the given entry's ancestor and its children for later processing by the iterator.
    /// Will ignore already traversed ancestors.
    fn queue_ancestor_and_children(&mut self, dir_entry: &DirEntry) {
        // We should only ever queue the ancestors of existing ancestors, or the start dir.
        debug_assert!(
            dir_entry.relation_to_start == PathRelationship::Ancestor
                || dir_entry.relation_to_start == PathRelationship::Identical
        );

        // If there is no valid ancestor, do nothing.
        let Some(ancestor_path) = dir_entry.path.parent() else {
            return;
        };

        // Mark the current path as seen.
        self.seen_ancestors.insert(dir_entry.path.clone());

        // First, push the ancestor.
        let ancestor = DirEntry {
            path: ancestor_path.to_path_buf(),
            distance: dir_entry.distance + 1,
            relation_to_start: PathRelationship::Ancestor,
        };
        self.queue.push_back(ancestor);

        // Attempt to fetch the child entries.
        let Ok(child_entries) = std::fs::read_dir(ancestor_path) else {
            return;
        };

        for entry in child_entries {
            if let Ok(entry) = entry {
                if !Self::is_dir(&entry) {
                    continue; // Not directory.
                }

                let path = entry.path();
                if self.seen_ancestors.contains(&path) {
                    continue; // Already seen.
                }
                self.queue.push_back(DirEntry {
                    path,
                    distance: dir_entry.distance + 1,
                    relation_to_start: PathRelationship::Disjoint,
                });
            }
        }
    }

    /// Queues a directory's children for later processing by the iterator.
    fn queue_children(&mut self, dir_entry: &DirEntry) {
        let child_relationship = match dir_entry.relation_to_start {
            PathRelationship::Identical => PathRelationship::Descendant,
            PathRelationship::Descendant => PathRelationship::Descendant,
            PathRelationship::Disjoint => PathRelationship::Disjoint,
            _ => unreachable!(),
        };

        if let Ok(child_entries) = std::fs::read_dir(&dir_entry.path) {
            for entry in child_entries {
                if let Ok(entry) = entry {
                    if !Self::is_dir(&entry) {
                        continue; // Not directory.
                    }

                    self.queue.push_back(DirEntry {
                        path: entry.path(),
                        distance: dir_entry.distance + 1,
                        relation_to_start: child_relationship,
                    });
                }
            }
        }
    }

    /// Checks whether an arbitrary [`std::fs::DirEntry`] is a directory or not.
    fn is_dir(entry: &std::fs::DirEntry) -> bool {
        match entry.file_type() {
            Ok(file_type) => {
                if file_type.is_dir() || file_type.is_symlink_dir() {
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }
}

impl<'c> Iterator for WalkerIter<'c> {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        // If there are no queue items remaining, we are finished.
        let Some(cur_dir) = self.queue.pop_front() else {
            return None;
        };

        // If we have reached the distance limit, do not fetch children.
        if cur_dir.distance == self.opts.max_dist {
            return Some(cur_dir);
        }

        // If the directory matches an ignore name, don't walk into it.
        // We ignore this setting for the root search directory, as that has either been explicitly included
        // in the search pattern or is the user's CWD.
        if let Some(ignores) = self.opts.ignores.as_ref() {
            if let Some(name) = cur_dir.dir_name() {
                if cur_dir.relation_to_start != PathRelationship::Identical
                    && ignores.contains(name)
                {
                    return Some(cur_dir);
                }
            }
        }

        // Queue later directories to be iterated based on the relationship.
        match cur_dir.relation_to_start {
            PathRelationship::Identical => {
                self.queue_children(&cur_dir);
                if self.opts.walk_upward {
                    self.queue_ancestor_and_children(&cur_dir);
                }
            }
            PathRelationship::Ancestor => {
                self.queue_ancestor_and_children(&cur_dir);
            }
            PathRelationship::Descendant | PathRelationship::Disjoint => {
                self.queue_children(&cur_dir);
            }
        }

        // If we aren't meant to include the start directory & we're currently there,
        // iterate again.
        if cur_dir.relation_to_start == PathRelationship::Identical && !self.opts.include_start_dir
        {
            return self.next();
        }

        Some(cur_dir)
    }
}

/// Types of relationship between two paths, A and B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathRelationship {
    /// Path A is path B.
    Identical,
    /// Path A is an ancestor of path B.
    Ancestor,
    /// Path A is a descendant of path B.
    Descendant,
    /// Path A is disjoint (neither an ancestor or descendant) from path B.
    Disjoint,
}

/// A single iterator entry produced by [`WalkerIter`].
/// Contains information about a single directory.
#[derive(Clone)]
pub(crate) struct DirEntry {
    /// The full path of this directory.
    path: PathBuf,
    /// Distance of this directory from the start.
    distance: usize,
    /// Relationship of this directory to the start directory.
    relation_to_start: PathRelationship,
}

impl DirEntry {
    /// Creates a start directory entry.
    fn start_dir(path: PathBuf) -> Self {
        Self {
            path,
            distance: 0,
            relation_to_start: PathRelationship::Identical,
        }
    }

    pub(crate) fn path(&self) -> &PathBuf {
        &self.path
    }

    pub(crate) fn dir_name(&self) -> Option<&OsStr> {
        self.path.file_name()
    }
}
