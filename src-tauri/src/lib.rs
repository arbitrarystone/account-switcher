//! Account Switcher —— Tauri 后端入口。
//!
//! 模块按领域划分（见 docs/specs/account-switcher-v1.md §附录）：
//! - `account`   账号元数据模型、持久化与业务服务（M1）
//! - `adapter`   Claude / Codex 可插拔适配层（M2.1）
//! - `pty`       伪终端会话管理与事件流（M2.2）
//! - `keychain`  系统钥匙串封装
//! - `commands`  Tauri 命令层

mod account;
mod adapter;
mod commands;
mod keychain;
mod pty;

use tauri::Manager;

use account::{AccountService, JsonFileStore};
use keychain::SystemKeychain;
use pty::PtyManager;

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
            // 账号元数据存于应用配置目录下的 accounts.json；Token 走系统钥匙串。
            let config_dir = app.path().app_config_dir()?;
            let store = Box::new(JsonFileStore::new(config_dir.join("accounts.json")));
            let service = AccountService::new(store, Box::new(SystemKeychain));
            app.manage(service);
            app.manage(PtyManager::default());
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
