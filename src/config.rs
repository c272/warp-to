// `warp-to` Copyright (C) 2026, c272
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License v3 as published by the Free
// Software Foundation.
//
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize, ser::SerializeSeq};

#[cfg(target_os = "windows")]
const DEFAULT_IGNORES: &'static [&'static str] =
    &[".git", "Program Files", "Program Files (x86)", "Temp"];

#[cfg(not(target_os = "windows"))]
const DEFAULT_IGNORES: &'static [&'static str] = &[".git"];

/// User-defined config for warp-to.
#[derive(Serialize, Deserialize)]
pub(crate) struct Config {
    /// User shortcuts used within search queries.
    #[serde(default)]
    pub shortcuts: HashMap<String, String>,
    /// Directory names to ignore when fuzzy searching.
    #[serde(
        default,
        serialize_with = "serialize_ignore_hashset",
        deserialize_with = "deserialize_ignore_hashset"
    )]
    pub ignore: HashSet<OsString>,
}

/// Serializes the "ignore" HashSet<OsString> to a plain string array in JSON.
fn serialize_ignore_hashset<S>(ignore: &HashSet<OsString>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut seq_ser = serializer.serialize_seq(Some(ignore.len()))?;
    for elem in ignore {
        seq_ser.serialize_element(&elem.to_string_lossy())?;
    }
    seq_ser.end()
}

/// Deserializes an "ignore" HashSet<OsString> from a plain string array in JSON.
fn deserialize_ignore_hashset<'de, D>(deserializer: D) -> Result<HashSet<OsString>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    struct JsonIgnoreHashSetVisitor;

    impl<'de> serde::de::Visitor<'de> for JsonIgnoreHashSetVisitor {
        type Value = HashSet<OsString>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("An array containing directory names to ignore.")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut ignore = HashSet::new();

            while let Some(elem) = seq.next_element::<String>()? {
                ignore.insert(OsString::from(elem));
            }

            Ok(ignore)
        }
    }

    deserializer.deserialize_any(JsonIgnoreHashSetVisitor)
}

impl Config {
    pub fn new() -> Self {
        Self {
            shortcuts: HashMap::new(),
            ignore: HashSet::from_iter(DEFAULT_IGNORES.iter().map(|s| OsString::from(s))),
        }
    }

    /// Loads the `warp-to` config file from disk, or creates one if not present.
    pub fn create_or_load() -> Result<Self, String> {
        let config_path = Self::get_config_path()?;

        // If a config does not exist, create the default one & return it.
        if !config_path.exists() {
            let config = Self::new();
            config.save_to_file(&config_path)?;
            return Ok(config);
        }

        // Read the config from disk.
        let file = File::open(config_path.clone())
            .map_err(|_| format!("Failed to open config file @ '{}'.", config_path.display()))?;
        let reader = BufReader::new(file);

        serde_json::from_reader(reader)
            .map_err(|e| format!("Failed to deserialize config file:\n{}", e))
    }

    /// Saves the config file to the given location on disk.
    fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let config_str = serde_json::to_string_pretty(self)
            .map_err(|_| "Failed to serialize config file.".to_string())?;

        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|_| {
                format!(
                    "Failed to create parent directories for config file '{}'.",
                    path.display()
                )
            })?;
        }
        std::fs::write(path, &config_str)
            .map_err(|_| format!("Failed to write config file '{}'.", path.display()))
    }

    /// Gets the path that a config should be present at (Windows).
    #[cfg(target_os = "windows")]
    fn get_config_path() -> Result<PathBuf, String> {
        let user_dir = std::env::var("APPDATA")
            .map_err(|_| "Failed to find APPDATA directory to save config into.".to_string())?;
        Ok(PathBuf::from_iter([&user_dir, "warp-to", "config.json"]))
    }

    /// Gets the path that a config should be present at (Unix-like).
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn get_config_path() -> Result<PathBuf, String> {
        let config_dir = std::env::var("XDG_CONFIG_HOME").unwrap_or("~/.config".into());
        Ok(PathBuf::from_iter([&config_dir, "warp-to", "config.json"]))
    }
}
