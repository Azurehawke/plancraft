//! The name a device saves its changes under (`devices\<name>.loro`).
//!
//! It must stay the same on one device and differ between devices, and it must **not** live in
//! the shared folder: if it did, the sync tool would copy it to every device and they would all
//! write the same file. The app keeps it in its own settings folder (`%APPDATA%\PlanCraft` and the
//! like); the Obsidian plugin keeps it in the browser's local storage.

use std::path::Path;

use crate::{Error, Result, random_id};

const MAX_NAME: usize = 40;

/// The device name stored in `state_file`, made from `hint` (the computer's name) and four random
/// hex digits on first use, e.g. `surface-pro-7-3f9a`.
pub fn device_name(state_file: &Path, hint: &str) -> Result<String> {
    if let Ok(s) = std::fs::read_to_string(state_file) {
        let name = s.lines().next().map(sanitize_device_name).unwrap_or_default();
        if !name.is_empty() {
            return Ok(name);
        }
    }
    let base = sanitize_device_name(hint);
    let base = if base.is_empty() { "device".to_string() } else { base };
    let suffix = random_id();
    let name = sanitize_device_name(&format!("{}-{}", base.get(..MAX_NAME - 5).unwrap_or(&base), suffix.get(..4).unwrap_or("0000")));
    if let Some(dir) = state_file.parent() {
        std::fs::create_dir_all(dir).map_err(|source| Error::Io { path: dir.display().to_string(), source })?;
    }
    std::fs::write(state_file, format!("{name}\n")).map_err(|source| Error::Io { path: state_file.display().to_string(), source })?;
    Ok(name)
}

/// Lowercase ASCII letters, digits and single dashes, at most 40 characters: safe as a file name
/// on every system and readable in a sync tool's file list.
pub fn sanitize_device_name(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= MAX_NAME {
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes() {
        assert_eq!(sanitize_device_name("  Steve's Surface Pro 7  "), "steve-s-surface-pro-7");
        assert_eq!(sanitize_device_name("../../etc"), "etc");
        assert_eq!(sanitize_device_name("ÜBER-PC"), "ber-pc");
        assert_eq!(sanitize_device_name(""), "");
        assert!(sanitize_device_name(&"x".repeat(500)).len() <= MAX_NAME);
    }

    #[test]
    fn stays_the_same_once_made() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("sub/device");
        let a = device_name(&f, "Laptop").unwrap();
        assert!(a.starts_with("laptop-") && a.len() == "laptop-".len() + 4, "{a}");
        assert_eq!(device_name(&f, "Something else").unwrap(), a);
        let other = device_name(&dir.path().join("other"), "Laptop").unwrap();
        assert!(other.starts_with("laptop-"));
    }

    #[test]
    fn empty_hint_still_names_the_device() {
        let dir = tempfile::tempdir().unwrap();
        assert!(device_name(&dir.path().join("d"), "!!!").unwrap().starts_with("device-"));
    }
}
