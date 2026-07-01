//! 从本地 CLI 会话记录反查 token 用量（account-switcher 的终端只是 PTY
//! 透传，看不到 API 流量本身，只能事后按项目目录 + 时间窗跟 claude/codex
//! 自己写的本地日志对账）。按工具分发到各自的日志格式解析器。

mod claude_transcripts;
mod codex_transcripts;

use std::path::Path;

use crate::account::Tool;

/// 一次会话的 token 用量汇总。`total()` 供聚合展示使用。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCounts {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

/// 尝试为一次会话（`project_dir` + `[started_at, ended_at]` 时间窗）匹配真实
/// token 用量。`None` 表示没找到可关联的本地日志（目录不存在 / 中转未返回
/// 标准 usage 字段等）——调用方应记为「未匹配」而非「确认为 0」。
/// `home_dir` 显式传入以便测试注入临时目录，生产调用处传 `app.path().home_dir()`。
pub fn scan(
    tool: Tool,
    home_dir: &Path,
    project_dir: &Path,
    started_at: &str,
    ended_at: &str,
) -> Option<TokenCounts> {
    match tool {
        Tool::Claude => claude_transcripts::scan(home_dir, project_dir, started_at, ended_at),
        Tool::Codex => codex_transcripts::scan(home_dir, project_dir, started_at, ended_at),
    }
}
