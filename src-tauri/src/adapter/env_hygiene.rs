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
pub fn clean_base_env() -> BTreeMap<String, String> {
    sanitize(std::env::vars())
}

#[cfg(test)]
mod tests {
    use super::*;

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
