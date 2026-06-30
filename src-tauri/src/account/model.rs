use serde::{Deserialize, Serialize};

use super::error::{AccountError, Result};

/// 目标工具。账号绑定单一工具（同一中转两边都支持时建两个账号 + 克隆）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Claude,
    Codex,
}

impl Tool {
    pub fn as_str(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
        }
    }
}

/// 账号元数据 —— **不含 Token**。Token 单独存系统钥匙串，此处仅留 `token_ref` 引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub tool: Tool,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 钥匙串键名（M1 取值等于 `id`）。
    pub token_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// 额外启动参数（如 `--dangerously-skip-permissions`），起任务时追加到命令行。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_args: Option<Vec<String>>,
    pub created_at: String,
    pub updated_at: String,
}

/// 新建账号输入 —— 含明文 Token，仅在创建瞬间存在，存入钥匙串后即丢弃。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAccount {
    pub name: String,
    pub tool: Tool,
    pub base_url: String,
    #[serde(default)]
    pub model: Option<String>,
    pub token: String,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub extra_args: Option<Vec<String>>,
}

/// 更新账号输入。每个字段 `None` 表示「保持不变」。
///
/// `model` / `tags` 用双层 `Option`：外层 `Some` 表示要改、内层表示新值（可清空）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<Option<String>>,
    /// 提供则更新钥匙串中的 Token；`None` 表示不动 Token。
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub tags: Option<Option<Vec<String>>>,
    #[serde(default)]
    pub extra_args: Option<Option<Vec<String>>>,
}

/// 当前 UTC 时间（RFC3339 字符串）。
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ── 输入校验（系统边界，fail fast）────────────────────────────

pub fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(AccountError::Validation("账号名称不能为空".into()));
    }
    Ok(())
}

pub fn validate_token(token: &str) -> Result<()> {
    if token.trim().is_empty() {
        return Err(AccountError::Validation("Token 不能为空".into()));
    }
    Ok(())
}

pub fn validate_base_url(raw: &str) -> Result<()> {
    let u = raw.trim();
    if u.is_empty() {
        return Err(AccountError::Validation("BASE_URL 不能为空".into()));
    }
    if !(u.starts_with("http://") || u.starts_with("https://")) {
        return Err(AccountError::Validation(
            "BASE_URL 必须以 http:// 或 https:// 开头".into(),
        ));
    }
    let host = u.split_once("://").map(|x| x.1).unwrap_or("");
    if host.is_empty() || host.starts_with('/') {
        return Err(AccountError::Validation("BASE_URL 缺少主机名".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_as_str_maps_correctly() {
        assert_eq!(Tool::Claude.as_str(), "claude");
        assert_eq!(Tool::Codex.as_str(), "codex");
    }

    #[test]
    fn tool_serializes_lowercase() {
        let j = serde_json::to_string(&Tool::Claude).unwrap();
        assert_eq!(j, "\"claude\"");
    }

    #[test]
    fn rejects_empty_name() {
        assert!(validate_name("  ").is_err());
        assert!(validate_name("ok").is_ok());
    }

    #[test]
    fn rejects_empty_token() {
        assert!(validate_token("").is_err());
        assert!(validate_token("sk-xxx").is_ok());
    }

    #[test]
    fn validates_base_url_scheme_and_host() {
        assert!(validate_base_url("https://relay.example.com").is_ok());
        assert!(validate_base_url("http://127.0.0.1:8080").is_ok());
        assert!(validate_base_url("relay.example.com").is_err()); // 无 scheme
        assert!(validate_base_url("https://").is_err()); // 无 host
        assert!(validate_base_url("ftp://x.com").is_err()); // 错误 scheme
        assert!(validate_base_url("  ").is_err()); // 空
    }
}
