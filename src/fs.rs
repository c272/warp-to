use std::path::PathBuf;

pub(crate) fn get_cwd() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir();
    match cwd {
        Ok(dir) => Ok(dir),
        Err(_) => Err("Failed to find current working directory.".into()),
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn fetch_root() -> Result<PathBuf, String> {
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
pub(crate) fn fetch_root() -> Result<PathBuf, String> {
    Ok(PathBuf::from(ROOT_CHAR))
}

pub(crate) fn fetch_home() -> Result<PathBuf, String> {
    let home_dir = std::env::home_dir().ok_or("Failed to find home directory.".to_string())?;
    Ok(home_dir)
}

pub(crate) fn fetch_ancestor(n: u32) -> Result<PathBuf, String> {
    let cwd = get_cwd()?;
    let mut cur_dir = cwd.as_path();

    for _ in 0..n {
        match cur_dir.parent() {
            Some(parent) => {
                cur_dir = parent;
            }
            None => {
                break;
            }
        }
    }

    Ok(cur_dir.to_path_buf())
}
