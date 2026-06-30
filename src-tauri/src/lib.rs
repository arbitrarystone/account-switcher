//! Account Switcher —— Tauri 后端入口。
//!
//! 模块按领域划分（见 docs/specs/account-switcher-v1.md §附录）：
//! - `account`        账号元数据 + 明文 Token 存储与业务服务（M1）
//! - `adapter`        Claude / Codex 可插拔适配层（M2.1）
//! - `pty`            伪终端会话管理与事件流（M2.2 / M3）
//! - `config_writer`  全局默认配置原子写入器（M4）
//! - `prefs`          偏好与记忆规则（M4 / M5）
//! - `usage`          用量统计 SQLite（M5）
//! - `commands`       Tauri 命令层

mod account;
mod adapter;
mod commands;
mod config_writer;
mod prefs;
mod pty;
mod session;
mod usage;

use tauri::Manager;

use account::{AccountService, JsonFileStore};
use prefs::PrefsStore;
use pty::PtyManager;
use session::SessionStore;
use usage::UsageStore;

/// 返回应用版本（占位命令，用于前后端连通性自检）。
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 账号元数据（含明文 Token）/ 偏好 / 用量库存于应用配置目录。
            let config_dir = app.path().app_config_dir()?;
            let store = Box::new(JsonFileStore::new(config_dir.join("accounts.json")));
            app.manage(AccountService::new(store));
            app.manage(PtyManager::default());
            app.manage(PrefsStore::load(config_dir.join("prefs.json")));
            app.manage(SessionStore::load(config_dir.join("sessions.json")));
            app.manage(UsageStore::open(&config_dir.join("usage.db"))?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::account_list,
            commands::account_create,
            commands::account_get,
            commands::account_update,
            commands::account_delete,
            commands::account_clone,
            commands::launch_session,
            commands::pty_write,
            commands::pty_resize,
            commands::pty_close,
            commands::set_default,
            commands::clear_default,
            commands::get_defaults,
            commands::get_usage_summary,
            commands::get_last_account,
            commands::get_sessions,
            commands::get_open_sessions,
            commands::session_closed,
            commands::remove_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
