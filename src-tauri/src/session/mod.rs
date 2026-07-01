//! 会话持久化（sessions.json）。
//!
//! 记住起过的会话（账号+项目目录），支持：
//! - 重启自动恢复上次打开的会话（`open` 标记）
//! - 会话历史面板手动重起任意一条
//!
//! 注意：PTY 进程本身在 app 关闭时终止，这里只持久化「会话配置」用于快速重建。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::account::Tool;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub account_id: String,
    pub tool: Tool,
    pub project_dir: String,
    pub title: String,
    pub last_used_at: String,
    /// 上次关闭 app 时该会话是否处于打开状态（用于重启自动恢复）。
    #[serde(default)]
    pub open: bool,
    /// 起任务时是否跳过权限确认（恢复 / 重起会话时按此值重放）。
    #[serde(default)]
    pub skip_permissions: bool,
}

fn key(account_id: &str, project_dir: &str) -> String {
    format!("{account_id}::{project_dir}")
}

/// sessions.json 的内存缓存 + 持久化，作为 Tauri 托管状态。
pub struct SessionStore {
    path: PathBuf,
    cache: Mutex<Vec<SessionRecord>>,
}

impl SessionStore {
    pub fn load(path: PathBuf) -> Self {
        let sessions = read(&path).unwrap_or_default();
        Self {
            path,
            cache: Mutex::new(sessions),
        }
    }

    /// 全部历史，按最近使用倒序。
    pub fn list(&self) -> Vec<SessionRecord> {
        let mut v = self.cache.lock().unwrap().clone();
        v.sort_by(|a, b| b.last_used_at.cmp(&a.last_used_at));
        v
    }

    /// 上次打开的会话（用于重启自动恢复），按使用时间正序（恢复顺序稳定）。
    pub fn open_sessions(&self) -> Vec<SessionRecord> {
        let mut v: Vec<_> = self
            .cache
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.open)
            .cloned()
            .collect();
        v.sort_by(|a, b| a.last_used_at.cmp(&b.last_used_at));
        v
    }

    /// 起任务：按 (account_id, project_dir) upsert 会话并标记打开。
    #[allow(clippy::too_many_arguments)]
    pub fn record_open(
        &self,
        account_id: &str,
        tool: Tool,
        project_dir: &str,
        title: &str,
        skip_permissions: bool,
        now: &str,
    ) -> Result<(), String> {
        let mut all = self.cache.lock().unwrap();
        let k = key(account_id, project_dir);
        if let Some(rec) = all
            .iter_mut()
            .find(|s| key(&s.account_id, &s.project_dir) == k)
        {
            rec.last_used_at = now.to_string();
            rec.title = title.to_string();
            rec.tool = tool;
            rec.open = true;
            rec.skip_permissions = skip_permissions;
        } else {
            all.push(SessionRecord {
                account_id: account_id.to_string(),
                tool,
                project_dir: project_dir.to_string(),
                title: title.to_string(),
                last_used_at: now.to_string(),
                open: true,
                skip_permissions,
            });
        }
        persist(&self.path, &all)
    }

    /// 关闭标签：标记未打开（保留历史）。
    pub fn mark_closed(&self, account_id: &str, project_dir: &str) -> Result<(), String> {
        let mut all = self.cache.lock().unwrap();
        let k = key(account_id, project_dir);
        if let Some(rec) = all
            .iter_mut()
            .find(|s| key(&s.account_id, &s.project_dir) == k)
        {
            rec.open = false;
        }
        persist(&self.path, &all)
    }

    /// 从历史中删除会话。
    pub fn remove(&self, account_id: &str, project_dir: &str) -> Result<(), String> {
        let mut all = self.cache.lock().unwrap();
        let k = key(account_id, project_dir);
        all.retain(|s| key(&s.account_id, &s.project_dir) != k);
        persist(&self.path, &all)
    }
}

fn read(path: &Path) -> Option<Vec<SessionRecord>> {
    let s = fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

fn persist(path: &Path, sessions: &[SessionRecord]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(sessions).map_err(|e| e.to_string())?;
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
    fn record_open_upserts_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        let store = SessionStore::load(path.clone());
        store
            .record_open("a1", Tool::Claude, "/p", "A · p", false, "2026-06-30T00:00:00Z")
            .unwrap();
        store
            .record_open("a1", Tool::Claude, "/p", "A · p", true, "2026-06-30T01:00:00Z")
            .unwrap();

        let reloaded = SessionStore::load(path);
        assert_eq!(reloaded.list().len(), 1, "同 key 应 upsert 不重复");
        assert_eq!(reloaded.list()[0].last_used_at, "2026-06-30T01:00:00Z");
        assert!(
            reloaded.list()[0].skip_permissions,
            "upsert 应更新 skip_permissions"
        );
        assert_eq!(reloaded.open_sessions().len(), 1);
    }

    #[test]
    fn mark_closed_keeps_history_but_not_open() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::load(dir.path().join("sessions.json"));
        store
            .record_open("a1", Tool::Codex, "/p", "t", false, "2026-06-30T00:00:00Z")
            .unwrap();
        store.mark_closed("a1", "/p").unwrap();
        assert_eq!(store.list().len(), 1, "历史保留");
        assert!(store.open_sessions().is_empty(), "不再自动恢复");
    }

    #[test]
    fn remove_deletes_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::load(dir.path().join("sessions.json"));
        store
            .record_open("a1", Tool::Claude, "/p", "t", false, "2026-06-30T00:00:00Z")
            .unwrap();
        store.remove("a1", "/p").unwrap();
        assert!(store.list().is_empty());
    }
}
