use std::path::Path;

use serde_json::json;

use super::env_hygiene::clean_base_env;
use super::{LaunchOpts, LaunchSpec, ToolAdapter};
use crate::account::Account;

/// Claude Code 适配器：通过 env 注入中转 BASE_URL + AUTH_TOKEN（按会话隔离）。
pub struct ClaudeAdapter;

/// 构造 `--settings` 内联覆盖：实测 `claude` 2.1.158 中 `~/.claude/settings.json`
/// 的 `env` 块（含残留的全局 `ANTHROPIC_API_KEY`）会作为 `x-api-key` 头一并发出，
/// 优先于本进程注入的 `ANTHROPIC_AUTH_TOKEN`（Bearer 头）被中转服务端采信 ——
/// 导致「切了账号却仍按全局默认账号扣量」。`--settings` 覆盖优先级高于
/// settings.json 与继承的进程 env（已实测验证），故在此显式清空 API_KEY 并
/// 复述本会话的 BASE_URL/AUTH_TOKEN，双保险覆盖任何全局残留。
fn settings_override(account: &Account, token: &str) -> String {
    let mut env = serde_json::Map::new();
    env.insert("ANTHROPIC_BASE_URL".into(), json!(account.base_url));
    env.insert("ANTHROPIC_AUTH_TOKEN".into(), json!(token));
    env.insert("ANTHROPIC_API_KEY".into(), json!(""));
    if let Some(model) = &account.model {
        env.insert("ANTHROPIC_MODEL".into(), json!(model));
    }
    json!({ "env": env }).to_string()
}

impl ToolAdapter for ClaudeAdapter {
    fn build_session_launch(
        &self,
        account: &Account,
        token: &str,
        project_dir: &Path,
        opts: &LaunchOpts,
    ) -> LaunchSpec {
        let mut env = clean_base_env();
        env.insert("ANTHROPIC_BASE_URL".to_string(), account.base_url.clone());
        env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), token.to_string());
        if let Some(model) = &account.model {
            env.insert("ANTHROPIC_MODEL".to_string(), model.clone());
        }

        let mut args = vec!["--settings".to_string(), settings_override(account, token)];
        if opts.skip_permissions {
            args.push("--dangerously-skip-permissions".to_string());
        }
        if opts.resume {
            args.push("--continue".to_string());
        }
        if let Some(extra) = &account.extra_args {
            args.extend(extra.iter().cloned());
        }

        LaunchSpec {
            program: "claude".to_string(),
            args,
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
            token: "sk-tok".into(),
            tags: None,
            extra_args: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    /// 从 spec.args 中取出 `--settings` 后紧跟的 JSON 载荷并解析。
    fn settings_payload(spec: &LaunchSpec) -> serde_json::Value {
        let idx = spec
            .args
            .iter()
            .position(|a| a == "--settings")
            .expect("missing --settings flag");
        serde_json::from_str(&spec.args[idx + 1]).expect("--settings payload must be valid JSON")
    }

    #[test]
    fn injects_base_url_and_token() {
        let spec = ClaudeAdapter.build_session_launch(
            &account(),
            "sk-tok",
            Path::new("/proj"),
            &LaunchOpts::default(),
        );
        assert_eq!(spec.program, "claude");
        assert_eq!(
            spec.env.get("ANTHROPIC_BASE_URL").unwrap(),
            "https://relay.example.com"
        );
        assert_eq!(spec.env.get("ANTHROPIC_AUTH_TOKEN").unwrap(), "sk-tok");
        assert_eq!(spec.env.get("ANTHROPIC_MODEL").unwrap(), "claude-opus-4");
        assert_eq!(spec.cwd, PathBuf::from("/proj"));
    }

    #[test]
    fn settings_override_blocks_global_api_key_leak() {
        // 实测 claude 2.1.158：~/.claude/settings.json 残留的全局 ANTHROPIC_API_KEY
        // 会作为 x-api-key 头随每次请求发出，被中转服务端优先采信，导致切号不生效。
        // --settings 覆盖优先级高于 settings.json 与继承的进程 env，须显式清空。
        let spec = ClaudeAdapter.build_session_launch(
            &account(),
            "sk-tok",
            Path::new("/proj"),
            &LaunchOpts::default(),
        );
        let payload = settings_payload(&spec);
        assert_eq!(payload["env"]["ANTHROPIC_BASE_URL"], "https://relay.example.com");
        assert_eq!(payload["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-tok");
        assert_eq!(payload["env"]["ANTHROPIC_API_KEY"], "");
        assert_eq!(payload["env"]["ANTHROPIC_MODEL"], "claude-opus-4");
    }

    #[test]
    fn settings_override_omits_model_when_absent() {
        let mut acc = account();
        acc.model = None;
        let spec =
            ClaudeAdapter.build_session_launch(&acc, "t", Path::new("/p"), &LaunchOpts::default());
        let payload = settings_payload(&spec);
        assert!(payload["env"].get("ANTHROPIC_MODEL").is_none());
    }

    #[test]
    fn omits_model_when_absent() {
        let mut acc = account();
        acc.model = None;
        let spec =
            ClaudeAdapter.build_session_launch(&acc, "t", Path::new("/p"), &LaunchOpts::default());
        assert!(!spec.env.contains_key("ANTHROPIC_MODEL"));
    }

    #[test]
    fn does_not_leak_dirty_auth_vars() {
        let spec = ClaudeAdapter.build_session_launch(
            &account(),
            "t",
            Path::new("/p"),
            &LaunchOpts::default(),
        );
        assert!(!spec.env.contains_key("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn skip_permissions_and_extra_args_appended() {
        let mut acc = account();
        acc.extra_args = Some(vec!["--verbose".to_string()]);
        let opts = LaunchOpts {
            skip_permissions: true,
            ..Default::default()
        };
        let spec = ClaudeAdapter.build_session_launch(&acc, "t", Path::new("/p"), &opts);
        // --settings <json> 固定居前，之后依次是 skip_permissions / extra_args。
        assert_eq!(
            &spec.args[2..],
            &["--dangerously-skip-permissions", "--verbose"]
        );
    }
}
