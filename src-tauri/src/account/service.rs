use super::error::{AccountError, Result};
use super::model::{
    now_rfc3339, validate_base_url, validate_name, validate_token, Account, AccountUpdate,
    NewAccount, Tool,
};
use super::store::AccountStore;

/// 账号业务逻辑（CRUD）。
///
/// Token 以**明文**随元数据一起存储（用户选择便利优先：可在 UI 查看明文）。
pub struct AccountService {
    store: Box<dyn AccountStore>,
}

impl AccountService {
    pub fn new(store: Box<dyn AccountStore>) -> Self {
        Self { store }
    }

    /// 创建账号：校验后入库。
    pub fn create(&self, input: NewAccount) -> Result<Account> {
        validate_name(&input.name)?;
        validate_base_url(&input.base_url)?;
        validate_token(&input.token)?;

        let now = now_rfc3339();
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            name: input.name.trim().to_string(),
            tool: input.tool,
            base_url: input.base_url.trim().to_string(),
            model: normalize_model(input.model),
            token: input.token,
            tags: input.tags,
            extra_args: input.extra_args,
            created_at: now.clone(),
            updated_at: now,
        };

        let mut all = self.store.list()?;
        all.push(account.clone());
        self.store.save_all(&all)?;
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

    /// 更新账号。各字段 `None` 表示保持不变。
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
        if let Some(extra_args) = upd.extra_args {
            acc.extra_args = extra_args;
        }
        if let Some(token) = upd.token {
            validate_token(&token)?;
            acc.token = token;
        }
        acc.updated_at = now_rfc3339();

        all[pos] = acc.clone();
        self.store.save_all(&all)?;
        Ok(acc)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut all = self.store.list()?;
        let pos = all
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| AccountError::NotFound(id.to_string()))?;
        all.remove(pos);
        self.store.save_all(&all)
    }

    /// 克隆账号到另一个工具（复制 BASE_URL / 模型 / Token，生成新 id）。
    pub fn clone_to(&self, id: &str, target: Tool) -> Result<Account> {
        let src = self.get(id)?;
        self.create(NewAccount {
            name: format!("{} (副本)", src.name),
            tool: target,
            base_url: src.base_url,
            model: src.model,
            token: src.token,
            tags: src.tags,
            extra_args: src.extra_args,
        })
    }

    /// 取账号 Token（供起任务时注入 env 使用）。
    pub fn get_token(&self, id: &str) -> Result<String> {
        Ok(self.get(id)?.token)
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

    fn service() -> (AccountService, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Box::new(JsonFileStore::new(dir.path().join("accounts.json")));
        (AccountService::new(store), dir)
    }

    fn new_claude(name: &str) -> NewAccount {
        NewAccount {
            name: name.into(),
            tool: Tool::Claude,
            base_url: "https://relay.example.com".into(),
            model: None,
            token: "sk-secret".into(),
            tags: None,
            extra_args: None,
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
    fn create_stores_token_in_plaintext() {
        let (svc, _d) = service();
        let acc = svc.create(new_claude("A")).unwrap();
        assert_eq!(acc.token, "sk-secret");
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
    fn update_token_changes_plaintext() {
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
    fn delete_removes_account() {
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
        assert_eq!(svc.get_token(&cloned.id).unwrap(), "sk-secret");
        assert_eq!(svc.list().unwrap().len(), 2);
    }
}
