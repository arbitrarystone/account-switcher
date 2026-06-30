//! env 卫生：从干净基底构造子进程环境，剔除可能劫持鉴权的脏变量。
//!
//! 承接调研里的「鉴权优先级阶梯」坑（spec §8）：若继承了更高优先级的脏变量
//! （如云厂商凭证、旧 API_KEY），注入的中转账号可能被劫持。故拉起子进程前
//! 从干净基底重建 env，只保留无关变量 + 本账号变量。

use std::collections::BTreeMap;

/// 精确匹配需剔除的变量名。
const DIRTY_KEYS: &[&str] = &[
    // Claude Code
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    // Codex / OpenAI
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_API_BASE",
];

/// 前缀匹配需剔除的变量（云厂商凭证）。
const DIRTY_PREFIXES: &[&str] = &["AWS_", "GOOGLE_", "GCP_", "AZURE_"];

fn is_dirty(key: &str) -> bool {
    DIRTY_KEYS.contains(&key) || DIRTY_PREFIXES.iter().any(|p| key.starts_with(p))
}

/// 过滤脏变量，返回干净的 env 基底。
pub fn sanitize<I>(vars: I) -> BTreeMap<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    vars.into_iter().filter(|(k, _)| !is_dirty(k)).collect()
}

/// 取当前进程 env 并清洗，作为子进程的干净基底。
///
/// 额外补全 PATH：macOS GUI app 从 Finder/Dock 启动时只继承最小 PATH
/// （`/usr/bin:/bin:...`），找不到用户经 nvm / npm 等安装的 claude/codex。
/// 故用登录 shell 的完整 PATH 合并补全。
pub fn clean_base_env() -> BTreeMap<String, String> {
    let mut env = sanitize(std::env::vars());
    if let Some(login_path) = login_shell_path() {
        let merged = merge_paths(&login_path, env.get("PATH").map(String::as_str));
        env.insert("PATH".to_string(), merged);
    }
    env
}

/// 合并登录 PATH 与现有 PATH：登录 PATH 优先，去重保序。
fn merge_paths(login: &str, current: Option<&str>) -> String {
    let mut seen = std::collections::HashSet::new();
    login
        .split(':')
        .chain(current.unwrap_or("").split(':'))
        .filter(|p| !p.is_empty() && seen.insert(p.to_string()))
        .collect::<Vec<_>>()
        .join(":")
}

/// 获取登录交互 shell 的完整 PATH（仅执行一次并缓存）。
/// 失败（如非类 Unix 或无 SHELL）时返回 None，调用方退回当前 PATH。
fn login_shell_path() -> Option<String> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let shell = std::env::var("SHELL").ok()?;
            // -l 加载 profile、-i 加载 rc（PATH 常在 .zshrc/.bashrc 里设置）
            let output = std::process::Command::new(&shell)
                .args(["-lic", "echo \"__ACCSW__${PATH}__ACCSW__\""])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let out = String::from_utf8_lossy(&output.stdout);
            let inner = out.split("__ACCSW__").nth(1)?.trim().to_string();
            (!inner.is_empty()).then_some(inner)
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_paths_dedups_login_first() {
        assert_eq!(merge_paths("/a:/b", Some("/b:/usr/bin")), "/a:/b:/usr/bin");
        assert_eq!(merge_paths("/a:/b", None), "/a:/b");
        assert_eq!(merge_paths("/a", Some("")), "/a");
    }

    #[test]
    fn removes_exact_dirty_keys() {
        let input = vec![
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("ANTHROPIC_API_KEY".to_string(), "sk-dirty".to_string()),
            ("ANTHROPIC_AUTH_TOKEN".to_string(), "old-token".to_string()),
            ("CLAUDE_CODE_USE_BEDROCK".to_string(), "1".to_string()),
        ];
        let out = sanitize(input);
        assert!(out.contains_key("PATH"));
        assert!(!out.contains_key("ANTHROPIC_API_KEY"));
        assert!(!out.contains_key("ANTHROPIC_AUTH_TOKEN"));
        assert!(!out.contains_key("CLAUDE_CODE_USE_BEDROCK"));
    }

    #[test]
    fn removes_prefixed_cloud_creds() {
        let input = vec![
            ("AWS_ACCESS_KEY_ID".to_string(), "x".to_string()),
            (
                "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
                "y".to_string(),
            ),
            ("HOME".to_string(), "/home/u".to_string()),
        ];
        let out = sanitize(input);
        assert!(!out.contains_key("AWS_ACCESS_KEY_ID"));
        assert!(!out.contains_key("GOOGLE_APPLICATION_CREDENTIALS"));
        assert!(out.contains_key("HOME"));
    }

    #[test]
    fn keeps_clean_vars_untouched() {
        let input = vec![
            ("PATH".to_string(), "/bin".to_string()),
            ("LANG".to_string(), "en_US.UTF-8".to_string()),
        ];
        let out = sanitize(input);
        assert_eq!(out.len(), 2);
        assert_eq!(out.get("LANG").unwrap(), "en_US.UTF-8");
    }
}
