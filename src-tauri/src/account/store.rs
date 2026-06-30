use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use super::error::{AccountError, Result};
use super::model::Account;

/// 账号元数据持久化抽象。
pub trait AccountStore: Send + Sync {
    fn list(&self) -> Result<Vec<Account>>;
    fn save_all(&self, accounts: &[Account]) -> Result<()>;
}

/// JSON 文件实现，写入采用「临时文件 + rename」原子替换（spec §8）。
pub struct JsonFileStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl JsonFileStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }
}

impl AccountStore for JsonFileStore {
    fn list(&self) -> Result<Vec<Account>> {
        let _guard = self.lock.lock().expect("account store lock poisoned");
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let data =
            fs::read_to_string(&self.path).map_err(|e| AccountError::Storage(e.to_string()))?;
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&data).map_err(|e| AccountError::Storage(e.to_string()))
    }

    fn save_all(&self, accounts: &[Account]) -> Result<()> {
        let _guard = self.lock.lock().expect("account store lock poisoned");
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| AccountError::Storage(e.to_string()))?;
        }
        let json = serde_json::to_string_pretty(accounts)
            .map_err(|e| AccountError::Storage(e.to_string()))?;
        // 原子写：先写临时文件，再 rename 覆盖
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| AccountError::Storage(e.to_string()))?;
        fs::rename(&tmp, &self.path).map_err(|e| AccountError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::model::Tool;

    fn sample(id: &str) -> Account {
        Account {
            id: id.into(),
            name: format!("acc-{id}"),
            tool: Tool::Claude,
            base_url: "https://relay.example.com".into(),
            model: None,
            token_ref: id.into(),
            tags: None,
            extra_args: None,
            created_at: "2026-06-30T00:00:00Z".into(),
            updated_at: "2026-06-30T00:00:00Z".into(),
        }
    }

    #[test]
    fn list_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().join("accounts.json"));
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn save_then_list_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().join("accounts.json"));
        let accounts = vec![sample("a"), sample("b")];
        store.save_all(&accounts).unwrap();
        let loaded = store.list().unwrap();
        assert_eq!(loaded, accounts);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("deep/nested/accounts.json");
        let store = JsonFileStore::new(nested.clone());
        store.save_all(&[sample("a")]).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn save_overwrites_atomically_no_tmp_left() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.json");
        let store = JsonFileStore::new(path.clone());
        store.save_all(&[sample("a")]).unwrap();
        store.save_all(&[sample("b"), sample("c")]).unwrap();
        assert_eq!(store.list().unwrap().len(), 2);
        assert!(!path.with_extension("json.tmp").exists());
    }
}
