//! 偏好与记忆规则（prefs.json）。
//!
//! - `default_account_by_tool`：每个工具的全局默认账号（M4，UI 显示★）
//! - `last_account_by_project`：项目上次用的账号（M5 记忆，起任务条预填）

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::account::Tool;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    #[serde(default)]
    pub default_account_by_tool: HashMap<String, String>,
    #[serde(default)]
    pub last_account_by_project: HashMap<String, String>,
}

fn project_key(project_dir: &str, tool: Tool) -> String {
    format!("{project_dir}::{}", tool.as_str())
}

/// prefs.json 的内存缓存 + 持久化，作为 Tauri 托管状态。
pub struct PrefsStore {
    path: PathBuf,
    cache: Mutex<Prefs>,
}

impl PrefsStore {
    pub fn load(path: PathBuf) -> Self {
        let prefs = read_prefs(&path).unwrap_or_default();
        Self {
            path,
            cache: Mutex::new(prefs),
        }
    }

    pub fn snapshot(&self) -> Prefs {
        self.cache.lock().unwrap().clone()
    }

    pub fn set_default(&self, tool: Tool, account_id: &str) -> std::result::Result<(), String> {
        let mut prefs = self.cache.lock().unwrap();
        prefs
            .default_account_by_tool
            .insert(tool.as_str().to_string(), account_id.to_string());
        persist(&self.path, &prefs)
    }

    pub fn clear_default(&self, tool: Tool) -> std::result::Result<(), String> {
        let mut prefs = self.cache.lock().unwrap();
        prefs.default_account_by_tool.remove(tool.as_str());
        persist(&self.path, &prefs)
    }

    pub fn set_last(
        &self,
        project_dir: &str,
        tool: Tool,
        account_id: &str,
    ) -> std::result::Result<(), String> {
        let mut prefs = self.cache.lock().unwrap();
        prefs
            .last_account_by_project
            .insert(project_key(project_dir, tool), account_id.to_string());
        persist(&self.path, &prefs)
    }

    pub fn get_last(&self, project_dir: &str, tool: Tool) -> Option<String> {
        self.cache
            .lock()
            .unwrap()
            .last_account_by_project
            .get(&project_key(project_dir, tool))
            .cloned()
    }
}

fn read_prefs(path: &Path) -> Option<Prefs> {
    let s = fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

fn persist(path: &Path, prefs: &Prefs) -> std::result::Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    let mut tmp_os = path.as_os_str().to_owned();
    tmp_os.push(".tmp");
    let tmp = PathBuf::from(tmp_os);
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_default_and_last_persist_across_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let store = PrefsStore::load(path.clone());
        store.set_default(Tool::Claude, "acc1").unwrap();
        store.set_last("/proj", Tool::Codex, "acc2").unwrap();

        let reloaded = PrefsStore::load(path);
        assert_eq!(
            reloaded
                .snapshot()
                .default_account_by_tool
                .get("claude")
                .map(String::as_str),
            Some("acc1")
        );
        assert_eq!(
            reloaded.get_last("/proj", Tool::Codex).as_deref(),
            Some("acc2")
        );
    }

    #[test]
    fn clear_default_removes() {
        let dir = tempfile::tempdir().unwrap();
        let store = PrefsStore::load(dir.path().join("prefs.json"));
        store.set_default(Tool::Claude, "acc1").unwrap();
        store.clear_default(Tool::Claude).unwrap();
        assert!(!store
            .snapshot()
            .default_account_by_tool
            .contains_key("claude"));
    }
}
