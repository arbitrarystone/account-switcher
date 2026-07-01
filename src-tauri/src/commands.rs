//! Tauri 命令层（薄封装，转发到服务 / 适配器 / PTY / 配置写入器 / 用量）。
//!
//! 命令参数遵循 Tauri 约定：前端传 camelCase，Rust 端用 snake_case，自动转换。

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use crate::account::{Account, AccountError, AccountService, AccountUpdate, NewAccount, Tool};
use crate::adapter::{adapter_for, LaunchOpts};
use crate::config_writer;
use crate::prefs::PrefsStore;
use crate::pty::{PtyManager, TauriSink};
use crate::session::{SessionRecord, SessionStore};
use crate::usage::{UsageStore, UsageSummary};

// ── 账号 CRUD ───────────────────────────────────────────

#[tauri::command]
pub fn account_list(service: State<AccountService>) -> Result<Vec<Account>, AccountError> {
    service.list()
}

#[tauri::command]
pub fn account_create(
    service: State<AccountService>,
    input: NewAccount,
) -> Result<Account, AccountError> {
    service.create(input)
}

#[tauri::command]
pub fn account_get(service: State<AccountService>, id: String) -> Result<Account, AccountError> {
    service.get(&id)
}

#[tauri::command]
pub fn account_update(
    service: State<AccountService>,
    id: String,
    patch: AccountUpdate,
) -> Result<Account, AccountError> {
    service.update(&id, patch)
}

#[tauri::command]
pub fn account_delete(service: State<AccountService>, id: String) -> Result<(), AccountError> {
    service.delete(&id)
}

#[tauri::command]
pub fn account_clone(
    service: State<AccountService>,
    id: String,
    target_tool: Tool,
) -> Result<Account, AccountError> {
    service.clone_to(&id, target_tool)
}

// ── 起任务 / PTY 会话 ────────────────────────────────────

/// 起一个隔离终端会话：取账号 + Token → 适配器构造隔离启动规格 → PTY 拉起子进程。
/// 同时记录用量（start）与「项目上次账号」记忆。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn launch_session(
    app: AppHandle,
    service: State<AccountService>,
    pty: State<PtyManager>,
    usage: State<UsageStore>,
    prefs: State<PrefsStore>,
    session: State<SessionStore>,
    account_id: String,
    project_dir: String,
    skip_permissions: bool,
    resume: bool,
    rows: u16,
    cols: u16,
) -> Result<String, String> {
    let account = service.get(&account_id).map_err(|e| e.to_string())?;
    let token = service.get_token(&account_id).map_err(|e| e.to_string())?;
    // 空 Token 拦截：否则注入 ANTHROPIC_AUTH_TOKEN="" 会静默落到全局配置的 key，
    // 表现为「切了账号却没换 token」。旧钥匙串账号迁明文后 token 为空，需重新填入。
    if token.trim().is_empty() {
        return Err(format!(
            "账号「{}」的 Token 为空，请先编辑该账号填入 Token 再起任务。",
            account.name
        ));
    }
    let opts = LaunchOpts {
        skip_permissions,
        resume,
    };
    let spec = adapter_for(account.tool).build_session_launch(
        &account,
        &token,
        Path::new(&project_dir),
        &opts,
    );
    let session_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().to_rfc3339();

    usage
        .record_start(
            &session_id,
            &account_id,
            account.tool.as_str(),
            &project_dir,
            &started_at,
        )
        .map_err(|e| e.to_string())?;
    let _ = prefs.set_last(&project_dir, account.tool, &account_id);
    let title = format!("{} · {}", account.name, project_basename(&project_dir));
    let _ = session.record_open(
        &account_id,
        account.tool,
        &project_dir,
        &title,
        skip_permissions,
        &started_at,
    );

    let sink = Arc::new(TauriSink::new(app, (*usage).clone()));
    pty.spawn(sink, session_id.clone(), spec, rows, cols)
        .map_err(|e| e.to_string())?;
    Ok(session_id)
}

/// 取项目目录的最后一段作为简短标题。
fn project_basename(dir: &str) -> String {
    Path::new(dir)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.to_string())
}

// ── 会话管理（持久化 + 恢复）──────────────────────────────

#[tauri::command]
pub fn get_sessions(session: State<SessionStore>) -> Vec<SessionRecord> {
    session.list()
}

#[tauri::command]
pub fn get_open_sessions(session: State<SessionStore>) -> Vec<SessionRecord> {
    session.open_sessions()
}

#[tauri::command]
pub fn session_closed(
    session: State<SessionStore>,
    account_id: String,
    project_dir: String,
) -> Result<(), String> {
    session.mark_closed(&account_id, &project_dir)
}

#[tauri::command]
pub fn remove_session(
    session: State<SessionStore>,
    account_id: String,
    project_dir: String,
) -> Result<(), String> {
    session.remove(&account_id, &project_dir)
}

#[tauri::command]
pub fn pty_write(pty: State<PtyManager>, session_id: String, data: String) -> Result<(), String> {
    pty.write(&session_id, &data).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_resize(
    pty: State<PtyManager>,
    session_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    pty.resize(&session_id, rows, cols)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_close(pty: State<PtyManager>, session_id: String) {
    pty.close(&session_id);
}

// ── 全局默认（M4）────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct Defaults {
    claude: Option<String>,
    codex: Option<String>,
}

/// 设全局默认账号：写工具的全局配置文件 + 记录到 prefs。
/// ⚠️ Claude 的 Token 会以明文写入 settings.json（前端需先确认）。
#[tauri::command]
pub fn set_default(
    app: AppHandle,
    service: State<AccountService>,
    prefs: State<PrefsStore>,
    tool: Tool,
    account_id: String,
) -> Result<(), String> {
    let account = service.get(&account_id).map_err(|e| e.to_string())?;
    if account.tool != tool {
        return Err("账号与所选工具不匹配".to_string());
    }
    let token = service.get_token(&account_id).map_err(|e| e.to_string())?;
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    match tool {
        Tool::Claude => config_writer::write_claude_default(
            &home.join(".claude"),
            &account.base_url,
            &token,
            account.model.as_deref(),
        )
        .map_err(|e| e.to_string())?,
        Tool::Codex => config_writer::write_codex_default(
            &home.join(".codex"),
            "accsw",
            &account.base_url,
            "ACCSW_CODEX_TOKEN",
            account.model.as_deref(),
        )
        .map_err(|e| e.to_string())?,
    }
    prefs.set_default(tool, &account_id)?;
    Ok(())
}

#[tauri::command]
pub fn clear_default(app: AppHandle, prefs: State<PrefsStore>, tool: Tool) -> Result<(), String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    match tool {
        Tool::Claude => {
            config_writer::clear_claude_default(&home.join(".claude")).map_err(|e| e.to_string())?
        }
        Tool::Codex => config_writer::clear_codex_default(&home.join(".codex"), "accsw")
            .map_err(|e| e.to_string())?,
    }
    prefs.clear_default(tool)?;
    Ok(())
}

#[tauri::command]
pub fn get_defaults(prefs: State<PrefsStore>) -> Defaults {
    let p = prefs.snapshot();
    Defaults {
        claude: p.default_account_by_tool.get("claude").cloned(),
        codex: p.default_account_by_tool.get("codex").cloned(),
    }
}

// ── 用量 / 记忆（M5）─────────────────────────────────────

#[tauri::command]
pub fn get_usage_summary(usage: State<UsageStore>) -> Result<Vec<UsageSummary>, String> {
    usage.summary().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_last_account(
    prefs: State<PrefsStore>,
    project_dir: String,
    tool: Tool,
) -> Option<String> {
    prefs.get_last(&project_dir, tool)
}
