use std::path::Path;

use super::env_hygiene::clean_base_env;
use super::{LaunchOpts, LaunchSpec, ToolAdapter};
use crate::account::Account;

/// 会话内 provider 标识与 token 环境变量名（按会话隔离，进程间不共享，固定名即可）。
const PROVIDER_ID: &str = "accsw";
const TOKEN_ENV: &str = "ACCSW_CODEX_TOKEN";

/// Codex 适配器：用 `-c` 内联覆盖**新建** provider + token env，
/// 按会话隔离、完全不碰全局 `~/.codex/config.toml`（spec §6）。
pub struct CodexAdapter;

impl ToolAdapter for CodexAdapter {
    fn build_session_launch(
        &self,
        account: &Account,
        token: &str,
        project_dir: &Path,
        opts: &LaunchOpts,
    ) -> LaunchSpec {
        let mut env = clean_base_env();
        env.insert(TOKEN_ENV.to_string(), token.to_string());

        let mut args = vec![
            "-c".to_string(),
            format!("model_providers.{PROVIDER_ID}.name=\"{PROVIDER_ID}\""),
            "-c".to_string(),
            format!(
                "model_providers.{PROVIDER_ID}.base_url=\"{}\"",
                account.base_url
            ),
            "-c".to_string(),
            format!("model_providers.{PROVIDER_ID}.env_key=\"{TOKEN_ENV}\""),
            "-c".to_string(),
            format!("model_provider=\"{PROVIDER_ID}\""),
        ];
        if let Some(model) = &account.model {
            args.push("-c".to_string());
            args.push(format!("model=\"{model}\""));
        }
        if opts.skip_permissions {
            args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
        }
        if let Some(extra) = &account.extra_args {
            args.extend(extra.iter().cloned());
        }

        LaunchSpec {
            program: "codex".to_string(),
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

    fn account() -> Account {
        Account {
            id: "id1".into(),
            name: "C".into(),
            tool: Tool::Codex,
            base_url: "https://relay.example.com/v1".into(),
            model: None,
            token: "sk-tok".into(),
            tags: None,
            extra_args: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn builds_inline_provider_args() {
        let spec = CodexAdapter.build_session_launch(
            &account(),
            "sk-tok",
            Path::new("/proj"),
            &LaunchOpts::default(),
        );
        assert_eq!(spec.program, "codex");
        // token 走 env，不进命令行
        assert_eq!(spec.env.get(TOKEN_ENV).unwrap(), "sk-tok");
        assert!(!spec.args.join(" ").contains("sk-tok"));
        // 关键 -c 覆盖存在
        let joined = spec.args.join(" ");
        assert!(joined.contains("model_provider=\"accsw\""));
        assert!(joined.contains("model_providers.accsw.base_url=\"https://relay.example.com/v1\""));
        assert!(joined.contains("model_providers.accsw.env_key=\"ACCSW_CODEX_TOKEN\""));
    }

    #[test]
    fn appends_model_override_when_present() {
        let mut acc = account();
        acc.model = Some("gpt-5-codex".into());
        let spec =
            CodexAdapter.build_session_launch(&acc, "t", Path::new("/p"), &LaunchOpts::default());
        assert!(spec.args.join(" ").contains("model=\"gpt-5-codex\""));
    }

    #[test]
    fn skip_permissions_appends_bypass_flag() {
        let opts = LaunchOpts {
            skip_permissions: true,
        };
        let spec = CodexAdapter.build_session_launch(&account(), "t", Path::new("/p"), &opts);
        assert!(spec
            .args
            .contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
    }
}
