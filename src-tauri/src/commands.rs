//! Tauri 命令层（薄封装，转发到服务 / 适配器 / PTY）。
//!
//! 命令参数遵循 Tauri 约定：前端传 camelCase，Rust 端用 snake_case，自动转换。

use std::path::Path;

use tauri::{AppHandle, State};

use crate::account::{Account, AccountError, AccountService, AccountUpdate, NewAccount, Tool};
use crate::adapter::adapter_for;
use crate::pty::PtyManager;

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
/// 返回新会话 id（前端用它订阅 `pty://output` / 回写输入）。
#[tauri::command]
pub fn launch_session(
    app: AppHandle,
    service: State<AccountService>,
    pty: State<PtyManager>,
    account_id: String,
    project_dir: String,
    rows: u16,
    cols: u16,
) -> Result<String, String> {
    let account = service.get(&account_id).map_err(|e| e.to_string())?;
    let token = service.get_token(&account_id).map_err(|e| e.to_string())?;
    let spec =
        adapter_for(account.tool).build_session_launch(&account, &token, Path::new(&project_dir));
    let session_id = uuid::Uuid::new_v4().to_string();
    pty.spawn(&app, session_id.clone(), spec, rows, cols)
        .map_err(|e| e.to_string())?;
    Ok(session_id)
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
