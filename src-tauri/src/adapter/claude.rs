use std::path::Path;

use super::env_hygiene::clean_base_env;
use super::{LaunchSpec, ToolAdapter};
use crate::account::Account;

/// Claude Code 适配器：通过 env 注入中转 BASE_URL + AUTH_TOKEN（按会话隔离）。
pub struct ClaudeAdapter;

impl ToolAdapter for ClaudeAdapter {
    fn build_session_launch(
        &self,
        account: &Account,
        token: &str,
        project_dir: &Path,
    ) -> LaunchSpec {
        let mut env = clean_base_env();
        env.insert("ANTHROPIC_BASE_URL".to_string(), account.base_url.clone());
        env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), token.to_string());
        if let Some(model) = &account.model {
            env.insert("ANTHROPIC_MODEL".to_string(), model.clone());
        }
        LaunchSpec {
            program: "claude".to_string(),
            args: Vec::new(),
            env,
            cwd: project_dir.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Tool;
    use std::path::PathBuf;

    fn account() -> Account {
        Account {
            id: "id1".into(),
            name: "A".into(),
            tool: Tool::Claude,
            base_url: "https://relay.example.com".into(),
            model: Some("claude-opus-4".into()),
            token_ref: "id1".into(),
            tags: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn injects_base_url_and_token() {
        let spec = ClaudeAdapter.build_session_launch(&account(), "sk-tok", Path::new("/proj"));
        assert_eq!(spec.program, "claude");
        assert!(spec.args.is_empty());
        assert_eq!(
            spec.env.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://relay.example.com"
        );
        assert_eq!(spec.env.get("ANTHROPIC_AUTH_TOKEN").unwrap(), "sk-tok");
        assert_eq!(spec.env.get("ANTHROPIC_MODEL").unwrap(), "claude-opus-4");
        assert_eq!(spec.cwd, PathBuf::from("/proj"));
    }

    #[test]
    fn omits_model_when_absent() {
        let mut acc = account();
        acc.model = None;
        let spec = ClaudeAdapter.build_session_launch(&acc, "t", Path::new("/p"));
        assert!(!spec.env.contains_key("ANTHROPIC_MODEL"));
    }

    #[test]
    fn does_not_leak_dirty_auth_vars() {
        // clean_base_env 应已剔除脏变量；这里仅断言注入后不含残留 API_KEY
        let spec = ClaudeAdapter.build_session_launch(&account(), "t", Path::new("/p"));
        assert!(!spec.env.contains_key("ANTHROPIC_API_KEY"));
    }
}
