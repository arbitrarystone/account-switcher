use crate::keychain::KeychainStore;

use super::error::{AccountError, Result};
use super::model::{
    now_rfc3339, validate_base_url, validate_name, validate_token, Account, AccountUpdate,
    NewAccount, Tool,
};
use super::store::AccountStore;

/// 账号业务逻辑：协调元数据存储（`AccountStore`）与 Token 存储（`KeychainStore`）。
///
/// 通过 trait object 注入依赖，便于单元测试用内存实现替换。
pub struct AccountService {
    store: Box<dyn AccountStore>,
    keychain: Box<dyn KeychainStore>,
}

impl AccountService {
    pub fn new(store: Box<dyn AccountStore>, keychain: Box<dyn KeychainStore>) -> Self {
        Self { store, keychain }
    }

    /// 创建账号：校验 → Token 入钥匙串 → 元数据入库（失败回滚钥匙串）。
    pub fn create(&self, input: NewAccount) -> Result<Account> {
        validate_name(&input.name)?;
        validate_base_url(&input.base_url)?;
        validate_token(&input.token)?;

        let id = uuid::Uuid::new_v4().to_string();
        let token_ref = id.clone();
        let now = now_rfc3339();

        self.keychain
            .set_token(&token_ref, &input.token)
            .map_err(|e| AccountError::Keychain(e.to_string()))?;

        let account = Account {
            id,
            name: input.name.trim().to_string(),
            tool: input.tool,
            base_url: input.base_url.trim().to_string(),
            model: normalize_model(input.model),
            token_ref: token_ref.clone(),
            tags: input.tags,
            created_at: now.clone(),
            updated_at: now,
        };

        let mut all = match self.store.list() {
            Ok(a) => a,
            Err(e) => {
                let _ = self.keychain.delete_token(&token_ref);
                return Err(e);
            }
        };
        all.push(account.clone());
        if let Err(e) = self.store.save_all(&all) {
            let _ = self.keychain.delete_token(&token_ref);
            return Err(e);
        }
        Ok(account)
    }

    pub fn list(&self) -> Result<Vec<Account>> {
        self.store.list()
    }

    pub fn get(&self, id: &str) -> Result<Account> {
        self.store
            .list()?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| AccountError::NotFound(id.to_string()))
    }

    /// 更新账号。`token` 提供时同步更新钥匙串。
    pub fn update(&self, id: &str, upd: AccountUpdate) -> Result<Account> {
        let mut all = self.store.list()?;
        let pos = all
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| AccountError::NotFound(id.to_string()))?;

        let mut acc = all[pos].clone();
        if let Some(name) = upd.name {
            validate_name(&name)?;
            acc.name = name.trim().to_string();
        }
        if let Some(base_url) = upd.base_url {
            validate_base_url(&base_url)?;
            acc.base_url = base_url.trim().to_string();
        }
        if let Some(model) = upd.model {
            acc.model = normalize_model(model);
        }
        if let Some(tags) = upd.tags {
            acc.tags = tags;
        }
        if let Some(token) = upd.token {
            validate_token(&token)?;
            self.keychain
                .set_token(&acc.token_ref, &token)
                .map_err(|e| AccountError::Keychain(e.to_string()))?;
        }
        acc.updated_at = now_rfc3339();

        all[pos] = acc.clone();
        self.store.save_all(&all)?;
        Ok(acc)
    }

    /// 删除账号：先移除元数据，再删钥匙串条目（幂等）。
    pub fn delete(&self, id: &str) -> Result<()> {
        let mut all = self.store.list()?;
        let pos = all
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| AccountError::NotFound(id.to_string()))?;
        let removed = all.remove(pos);
        self.store.save_all(&all)?;
        self.keychain
            .delete_token(&removed.token_ref)
            .map_err(|e| AccountError::Keychain(e.to_string()))?;
        Ok(())
    }

    /// 克隆账号到另一个工具（复制 BASE_URL / 模型 / Token，生成新 id）。
    pub fn clone_to(&self, id: &str, target: Tool) -> Result<Account> {
        let src = self.get(id)?;
        let token = self
            .keychain
            .get_token(&src.token_ref)
            .map_err(|e| AccountError::Keychain(e.to_string()))?;
        self.create(NewAccount {
            name: format!("{} (副本)", src.name),
            tool: target,
            base_url: src.base_url,
            model: src.model,
            token,
            tags: src.tags,
        })
    }

    /// 取账号 Token（供起任务时注入 env 使用）。
    pub fn get_token(&self, id: &str) -> Result<String> {
        let acc = self.get(id)?;
        self.keychain
            .get_token(&acc.token_ref)
            .map_err(|e| AccountError::Keychain(e.to_string()))
    }
}

/// 空白模型名归一为 `None`。
fn normalize_model(model: Option<String>) -> Option<String> {
    model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::store::JsonFileStore;
    use crate::keychain::InMemoryKeychain;

    fn service() -> (AccountService, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Box::new(JsonFileStore::new(dir.path().join("accounts.json")));
        let keychain = Box::new(InMemoryKeychain::default());
        (AccountService::new(store, keychain), dir)
    }

    fn new_claude(name: &str) -> NewAccount {
        NewAccount {
            name: name.into(),
            tool: Tool::Claude,
            base_url: "https://relay.example.com".into(),
            model: None,
            token: "sk-secret".into(),
            tags: None,
        }
    }

    #[test]
    fn create_then_list() {
        let (svc, _d) = service();
        let acc = svc.create(new_claude("A")).unwrap();
        assert_eq!(acc.name, "A");
        assert_eq!(acc.tool, Tool::Claude);
        assert_eq!(svc.list().unwrap().len(), 1);
    }

    #[test]
    fn create_stores_token_in_keychain() {
        let (svc, _d) = service();
        let acc = svc.create(new_claude("A")).unwrap();
        assert_eq!(svc.get_token(&acc.id).unwrap(), "sk-secret");
    }

    #[test]
    fn create_rejects_invalid_input() {
        let (svc, _d) = service();
        let mut bad = new_claude("");
        assert!(svc.create(bad).is_err());
        bad = new_claude("ok");
        bad.token = "  ".into();
        assert!(svc.create(bad).is_err());
        bad = new_claude("ok");
        bad.base_url = "notaurl".into();
        assert!(svc.create(bad).is_err());
        assert!(svc.list().unwrap().is_empty(), "失败创建不应残留");
    }

    #[test]
    fn get_not_found() {
        let (svc, _d) = service();
        assert!(matches!(svc.get("nope"), Err(AccountError::NotFound(_))));
    }

    #[test]
    fn update_changes_fields() {
        let (svc, _d) = service();
        let acc = svc.create(new_claude("A")).unwrap();
        let upd = AccountUpdate {
            name: Some("renamed".into()),
            base_url: Some("https://new.example.com".into()),
            model: Some(Some("opus".into())),
            ..Default::default()
        };
        let updated = svc.update(&acc.id, upd).unwrap();
        assert_eq!(updated.name, "renamed");
        assert_eq!(updated.base_url, "https://new.example.com");
        assert_eq!(updated.model.as_deref(), Some("opus"));
    }

    #[test]
    fn update_token_updates_keychain() {
        let (svc, _d) = service();
        let acc = svc.create(new_claude("A")).unwrap();
        let upd = AccountUpdate {
            token: Some("sk-rotated".into()),
            ..Default::default()
        };
        svc.update(&acc.id, upd).unwrap();
        assert_eq!(svc.get_token(&acc.id).unwrap(), "sk-rotated");
    }

    #[test]
    fn delete_removes_account_and_token() {
        let (svc, _d) = service();
        let acc = svc.create(new_claude("A")).unwrap();
        svc.delete(&acc.id).unwrap();
        assert!(svc.list().unwrap().is_empty());
        assert!(svc.get_token(&acc.id).is_err());
    }

    #[test]
    fn clone_to_copies_to_other_tool() {
        let (svc, _d) = service();
        let src = svc.create(new_claude("A")).unwrap();
        let cloned = svc.clone_to(&src.id, Tool::Codex).unwrap();
        assert_ne!(cloned.id, src.id);
        assert_eq!(cloned.tool, Tool::Codex);
        assert_eq!(cloned.base_url, src.base_url);
        // Token 一并复制
        assert_eq!(svc.get_token(&cloned.id).unwrap(), "sk-secret");
        assert_eq!(svc.list().unwrap().len(), 2);
    }
}
