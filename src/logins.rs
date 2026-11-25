//! More than one sign-in for the same tool, kept apart.
//!
//! Claude Code keeps its sign-in, settings and session files in one folder,
//! and an environment variable it documents, `CLAUDE_CONFIG_DIR`, says which
//! folder. A named sign-in here is a folder of its own. A session started
//! under a name is pointed at that folder for its whole life, so a work
//! sign-in and a personal one never see each other's files.
//!
//! Maestro makes the folders and passes the variable. It never opens the
//! files the tool keeps in them. A sign-in can instead name an environment
//! variable that holds an API key; the key is handed to the tool when a
//! session starts and is never written down or shown.

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::PathBuf;

/// The variable Claude Code reads to find its folder.
pub const CONFIG_VAR: &str = "CLAUDE_CONFIG_DIR";
/// The variable Claude Code reads an API key from.
pub const KEY_VAR: &str = "ANTHROPIC_API_KEY";

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Login {
    pub name: String,
    /// The name of an environment variable that holds an API key for this
    /// sign-in. The name is kept; the key never is.
    #[serde(default)]
    pub key_var: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Logins {
    /// Sign-ins for Claude Code, besides the one the tool has by itself.
    #[serde(default)]
    pub claude: Vec<Login>,
}

/// A folder-safe form of a name: lower case, with spaces as dashes.
pub fn slug(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c == ' ' { '-' } else { c.to_ascii_lowercase() })
        .collect()
}

/// Check a name someone typed. Names become folder names, so only plain
/// characters are taken.
pub fn valid_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let plain = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'));
    if name.is_empty() || name.len() > 40 || !plain {
        return Err("Use 1 to 40 letters, digits, spaces, dashes or underscores.".to_string());
    }
    if slug(name) == "default" {
        return Err("\"default\" already means the tool's own sign-in.".to_string());
    }
    Ok(name.to_string())
}

/// Check the name of an environment variable.
pub fn valid_var(name: &str) -> Result<String, String> {
    let name = name.trim();
    let plain = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if name.is_empty() || name.len() > 80 || !plain || name.starts_with(|c: char| c.is_ascii_digit()) {
        return Err("A variable name is letters, digits and underscores, and does not start with a digit.".to_string());
    }
    Ok(name.to_string())
}

impl Login {
    /// The folder this sign-in's tool keeps its files in.
    pub fn folder(&self) -> PathBuf {
        crate::paths::data_dir()
            .join("logins")
            .join("claude-code")
            .join(slug(&self.name))
    }

    /// What to set for a program started under this sign-in. Fails when the
    /// sign-in names a key variable that is not set.
    pub fn env(&self) -> Result<Vec<(String, OsString)>, String> {
        let mut env = Vec::new();
        if let Some(var) = &self.key_var {
            // Read here and handed straight on; it is not kept anywhere.
            match std::env::var_os(var).filter(|v| !v.is_empty()) {
                Some(key) => env.push((KEY_VAR.to_string(), key)),
                None => {
                    return Err(format!(
                        "The sign-in \"{}\" takes its key from {var}, which is not set. Set it and restart Maestro.",
                        self.name
                    ))
                }
            }
        }
        let folder = self.folder();
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("Could not make the folder {}. ({e})", folder.display()))?;
        env.push((CONFIG_VAR.to_string(), folder.into_os_string()));
        Ok(env)
    }

    /// Whether the key variable this sign-in names is set. `None` when it names none.
    pub fn key_set(&self) -> Option<bool> {
        self.key_var
            .as_ref()
            .map(|var| std::env::var_os(var).is_some_and(|v| !v.is_empty()))
    }
}

impl Logins {
    pub fn load() -> Self {
        crate::paths::load("logins.json")
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            crate::paths::save("logins.json", &text);
        }
    }

    pub fn find(&self, name: &str) -> Option<&Login> {
        let wanted = slug(name);
        self.claude.iter().find(|l| slug(&l.name) == wanted)
    }

    /// Add a sign-in. Two names that would share a folder are refused.
    pub fn add(&mut self, name: &str, key_var: Option<&str>) -> Result<Login, String> {
        let name = valid_name(name)?;
        let key_var = match key_var.map(str::trim).filter(|v| !v.is_empty()) {
            Some(var) => Some(valid_var(var)?),
            None => None,
        };
        if self.find(&name).is_some() {
            return Err(format!("There is already a sign-in called {name}."));
        }
        let login = Login { name, key_var };
        self.claude.push(login.clone());
        Ok(login)
    }

    /// Take a sign-in off the list. Its folder is left where it is, because
    /// it holds that sign-in's session history; the path is returned so the
    /// user can remove it themselves.
    pub fn forget(&mut self, name: &str) -> Result<PathBuf, String> {
        let login = self.find(name).cloned().ok_or(format!("There is no sign-in called {name}."))?;
        self.claude.retain(|l| l != &login);
        Ok(login.folder())
    }
}

/// `--logins`, `--login-add <name> [--key-var VAR]`, `--login-forget <name>`.
pub fn cli(args: &[String]) -> Option<i32> {
    let after = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    let mut logins = Logins::load();
    let result = if let Some(name) = after("--login-add") {
        logins.add(name, after("--key-var")).map(|login| {
            logins.save();
            match &login.key_var {
                Some(var) => format!(
                    "added {}. It takes its key from {var}; nothing else is needed.",
                    login.name
                ),
                None => format!(
                    "added {}. Sign it in from the Live page, or by running `claude auth login` with {CONFIG_VAR} set to {}",
                    login.name,
                    login.folder().display()
                ),
            }
        })
    } else if let Some(name) = after("--login-forget") {
        logins.forget(name).map(|folder| {
            logins.save();
            format!(
                "forgotten. Its folder was left in place, with its sign-in and history: {}",
                folder.display()
            )
        })
    } else if args.iter().any(|a| a == "--logins") {
        let mut out = vec!["default: Claude Code's own sign-in, in its own folder".to_string()];
        for (login, state) in crate::services::login_states(&logins) {
            out.push(format!("{}: {}. Folder: {}", login.name, state, login.folder().display()));
        }
        Ok(out.join("\n"))
    } else {
        return None;
    };
    match result {
        Ok(text) => {
            println!("{text}");
            Some(0)
        }
        Err(problem) => {
            eprintln!("{problem}");
            Some(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_plain_and_two_cannot_share_a_folder() {
        let mut logins = Logins::default();
        assert_eq!(logins.add(" Work ", None).unwrap().name, "Work");
        assert!(logins.add("work", None).is_err());
        assert!(logins.add("My Company", Some("WORK_KEY")).is_ok());
        assert!(logins.add("my-company", None).is_err());
        for bad in ["", "default", "Default", "../up", "a/b", "c:\\x", "semi;colon", &"x".repeat(41)] {
            assert!(logins.add(bad, None).is_err(), "{bad}");
        }
        assert!(logins.add("keyed", Some("1BAD")).is_err());
        assert!(logins.add("keyed", Some("A B")).is_err());
        assert_eq!(logins.claude.len(), 2);
        assert_eq!(logins.find("MY COMPANY").unwrap().key_var.as_deref(), Some("WORK_KEY"));
    }

    #[test]
    fn each_sign_in_has_its_own_folder_under_maestros_data() {
        let a = Login { name: "Work".into(), key_var: None };
        let b = Login { name: "Personal".into(), key_var: None };
        assert_ne!(a.folder(), b.folder());
        assert!(a.folder().ends_with("logins/claude-code/work") || a.folder().ends_with("logins\\claude-code\\work"));
        assert!(a.folder().starts_with(crate::paths::data_dir()));
    }

    #[test]
    fn forgetting_leaves_the_folder_and_says_where_it_is() {
        let mut logins = Logins::default();
        let login = logins.add("Side project", None).unwrap();
        assert_eq!(logins.forget("side project"), Ok(login.folder()));
        assert!(logins.claude.is_empty());
        assert!(logins.forget("side project").is_err());
    }

    #[test]
    fn a_key_sign_in_refuses_to_start_when_its_variable_is_not_set() {
        let login = Login {
            name: "keyed-test".into(),
            key_var: Some("MAESTRO_TEST_KEY_THAT_IS_NOT_SET".into()),
        };
        assert_eq!(login.key_set(), Some(false));
        assert!(login.env().unwrap_err().contains("MAESTRO_TEST_KEY_THAT_IS_NOT_SET"));
        assert_eq!(Login { name: "plain".into(), key_var: None }.key_set(), None);
    }
}
