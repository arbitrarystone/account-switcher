use serde::Serialize;

/// 账号领域错误。
///
/// 实现 `Serialize`，以便直接作为 Tauri 命令的错误返回给前端，
/// 前端可依据 `kind` 字段区分错误类型并展示 `message`。
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AccountError {
    #[error("账号不存在: {0}")]
    NotFound(String),

    #[error("校验失败: {0}")]
    Validation(String),

    #[error("存储失败: {0}")]
    Storage(String),
}

pub type Result<T> = std::result::Result<T, AccountError>;
