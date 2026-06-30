//! 可插拔工具适配层：为每个工具构造「按会话隔离」的启动规格（不碰全局配置）。

mod claude;
mod codex;
mod env_hygiene;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::account::{Account, Tool};

pub use claude::ClaudeAdapter;
pub use codex::CodexAdapter;

/// 子进程启动规格：program + args + 隔离 env + 工作目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: PathBuf,
}

/// 起任务的运行期选项（不持久化，来自起任务条）。
#[derive(Debug, Default, Clone)]
pub struct LaunchOpts {
    /// 跳过权限确认：Claude 加 `--dangerously-skip-permissions`，
    /// Codex 加 `--dangerously-bypass-approvals-and-sandbox`。
    pub skip_permissions: bool,
}

/// 工具适配器：按会话隔离地构造启动规格。
pub trait ToolAdapter {
    fn build_session_launch(
        &self,
        account: &Account,
        token: &str,
        project_dir: &Path,
        opts: &LaunchOpts,
    ) -> LaunchSpec;
}

/// 按工具返回对应适配器（M2.4 launch_session 命令接入时使用）。
pub fn adapter_for(tool: Tool) -> Box<dyn ToolAdapter> {
    match tool {
        Tool::Claude => Box::new(ClaudeAdapter),
        Tool::Codex => Box::new(CodexAdapter),
    }
}
