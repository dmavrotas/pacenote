//! Safe, idempotent registration of the pacenote hook in Claude Code's
//! `settings.json`.
//!
//! Users' settings files hold unrelated configuration (other hooks,
//! permissions, env), so edits are surgical: key order is preserved, only
//! pacenote's own entries are touched, invalid JSON is never overwritten, and
//! the previous file is kept as a backup.

use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};

const EVENT: &str = "UserPromptSubmit";

#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Added,
    Updated,
    Unchanged,
}

/// Does this hook command belong to pacenote (any path, any flags)?
pub fn is_pacenote_hook(command: &str) -> bool {
    command.contains("pacenote") && command.contains("hook prompt")
}

/// Register `command` as a `UserPromptSubmit` hook, or update an existing
/// pacenote entry in place.
pub fn install(settings: &mut Value, command: &str) -> Result<Change> {
    let root = settings
        .as_object_mut()
        .context("settings.json must contain a JSON object")?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("`hooks` in settings.json must be an object")?;
    let groups = hooks
        .entry(EVENT)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .with_context(|| format!("`hooks.{EVENT}` in settings.json must be an array"))?;

    let existing = groups
        .iter_mut()
        .filter_map(|g| g.get_mut("hooks").and_then(Value::as_array_mut))
        .flatten()
        .find(|h| {
            h.get("command")
                .and_then(Value::as_str)
                .is_some_and(is_pacenote_hook)
        });

    Ok(match existing {
        Some(hook) if hook["command"] == command => Change::Unchanged,
        Some(hook) => {
            hook["command"] = json!(command);
            Change::Updated
        }
        None => {
            groups.push(json!({ "hooks": [{ "type": "command", "command": command }] }));
            Change::Added
        }
    })
}

/// Remove every pacenote hook entry, plus any containers that become empty
/// as a result. Returns the number of hooks removed.
pub fn uninstall(settings: &mut Value) -> usize {
    let pointer = format!("/hooks/{EVENT}");
    let Some(groups) = settings.pointer_mut(&pointer).and_then(Value::as_array_mut) else {
        return 0;
    };

    let mut removed = 0;
    groups.retain_mut(|group| {
        let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = list.len();
        list.retain(|h| {
            !h.get("command")
                .and_then(Value::as_str)
                .is_some_and(is_pacenote_hook)
        });
        removed += before - list.len();
        // Drop the group only if we emptied it.
        !(list.is_empty() && before > 0)
    });

    if removed > 0 && groups.is_empty() {
        let hooks = settings["hooks"].as_object_mut().expect("checked above");
        hooks.remove(EVENT);
        if hooks.is_empty() {
            settings
                .as_object_mut()
                .expect("checked above")
                .remove("hooks");
        }
    }
    removed
}

/// Load a settings file; a missing or empty file is an empty object.
pub fn load(path: &Path) -> Result<Value> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => serde_json::from_str(&text).with_context(|| {
            format!(
                "{} is not valid JSON; refusing to modify it",
                path.display()
            )
        }),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Write settings atomically, backing up any existing file first.
/// Returns the backup path, if one was made.
pub fn save(path: &Path, settings: &Value) -> Result<Option<PathBuf>> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let backup = if path.exists() {
        let backup = with_suffix(path, ".pacenote.bak");
        fs::copy(path, &backup).with_context(|| format!("backing up {}", path.display()))?;
        Some(backup)
    } else {
        None
    };
    let tmp = with_suffix(path, ".pacenote.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(settings)? + "\n")
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(backup)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}

/// Claude Code's config directory: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_config_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("neither HOME nor USERPROFILE is set")?;
    Ok(PathBuf::from(home).join(".claude"))
}

/// The hook command for a pacenote binary at `exe`.
pub fn hook_command(exe: &Path, strict: bool) -> String {
    // Forward slashes work in every shell Claude Code uses, including Git Bash on Windows.
    let exe = exe.to_string_lossy().replace('\\', "/");
    let exe = if exe.contains(char::is_whitespace) {
        format!("\"{exe}\"")
    } else {
        exe
    };
    let strict = if strict { " --strict" } else { "" };
    format!("{exe} hook prompt{strict}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "/usr/local/bin/pacenote hook prompt";

    fn rtk_settings() -> Value {
        json!({
            "model": "opus",
            "hooks": {"PreToolUse": [{"matcher": "Bash",
                                      "hooks": [{"type": "command", "command": "rtk hook claude"}]}]},
            "tui": "fullscreen"
        })
    }

    #[test]
    fn adds_hook_and_preserves_everything_else() {
        let mut s = rtk_settings();
        assert_eq!(install(&mut s, CMD).unwrap(), Change::Added);
        assert_eq!(
            s["hooks"]["PreToolUse"],
            rtk_settings()["hooks"]["PreToolUse"]
        );
        assert_eq!(s["hooks"][EVENT][0]["hooks"][0]["command"], CMD);
        let keys: Vec<_> = s.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["model", "hooks", "tui"], "key order preserved");
    }

    #[test]
    fn install_is_idempotent_and_updates_in_place() {
        let mut s = json!({});
        install(&mut s, CMD).unwrap();
        assert_eq!(install(&mut s, CMD).unwrap(), Change::Unchanged);
        assert_eq!(
            install(&mut s, "/opt/pacenote hook prompt --strict").unwrap(),
            Change::Updated
        );
        let groups = s["hooks"][EVENT].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0]["hooks"][0]["command"],
            "/opt/pacenote hook prompt --strict"
        );
    }

    #[test]
    fn install_alongside_other_prompt_hooks() {
        let mut s =
            json!({"hooks": {EVENT: [{"hooks": [{"type": "command", "command": "other"}]}]}});
        assert_eq!(install(&mut s, CMD).unwrap(), Change::Added);
        assert_eq!(s["hooks"][EVENT].as_array().unwrap().len(), 2);
    }

    #[test]
    fn rejects_unexpected_shapes() {
        assert!(install(&mut json!([]), CMD).is_err());
        assert!(install(&mut json!({"hooks": []}), CMD).is_err());
        assert!(install(&mut json!({"hooks": {EVENT: {}}}), CMD).is_err());
    }

    #[test]
    fn uninstall_round_trips_to_original() {
        let mut s = rtk_settings();
        install(&mut s, CMD).unwrap();
        assert_eq!(uninstall(&mut s), 1);
        assert_eq!(s, rtk_settings());
        assert_eq!(uninstall(&mut s), 0);
    }

    #[test]
    fn uninstall_keeps_foreign_hooks_in_shared_group() {
        let mut s = json!({"hooks": {EVENT: [{"hooks": [
            {"type": "command", "command": "other"},
            {"type": "command", "command": CMD}]}]}});
        assert_eq!(uninstall(&mut s), 1);
        assert_eq!(
            s["hooks"][EVENT][0]["hooks"],
            json!([{"type": "command", "command": "other"}])
        );
    }

    #[test]
    fn uninstall_removes_empty_containers() {
        let mut s = json!({"model": "x"});
        install(&mut s, CMD).unwrap();
        uninstall(&mut s);
        assert_eq!(s, json!({"model": "x"}));
    }

    #[test]
    fn hook_command_quotes_and_normalizes_paths() {
        assert_eq!(
            hook_command(Path::new("/usr/bin/pacenote"), false),
            "/usr/bin/pacenote hook prompt"
        );
        assert_eq!(
            hook_command(Path::new(r"C:\Program Files\pacenote.exe"), true),
            "\"C:/Program Files/pacenote.exe\" hook prompt --strict"
        );
        assert!(is_pacenote_hook(&hook_command(
            Path::new("pacenote.exe"),
            true
        )));
    }

    #[test]
    fn load_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.json");
        assert_eq!(load(&path).unwrap(), json!({}));

        assert_eq!(save(&path, &json!({"a": 1})).unwrap(), None);
        let backup = save(&path, &json!({"a": 2})).unwrap().expect("backup");
        assert_eq!(load(&backup).unwrap(), json!({"a": 1}));
        assert_eq!(load(&path).unwrap(), json!({"a": 2}));

        fs::write(&path, "{ broken").unwrap();
        assert!(load(&path).unwrap_err().to_string().contains("refusing"));
    }
}
