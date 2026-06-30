//! 伪终端会话管理。
//!
//! 每个会话 = 一对 PTY + 子进程 + 读线程 + 等待线程。
//! 读线程把输出经 Tauri 事件 `pty://output` 推给前端；
//! 子进程退出时发 `pty://exit`。前端输入经 [`PtyManager::write`] 回写。
//!
//! 多会话由 [`PtyManager`] 统一持有（M3 并发隔离即基于此）。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::adapter::LaunchSpec;

#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("PTY 启动失败: {0}")]
    Spawn(String),
    #[error("会话不存在: {0}")]
    NotFound(String),
    #[error("写入失败: {0}")]
    Write(String),
}

/// 推给前端的输出事件载荷。
#[derive(Clone, Serialize)]
struct PtyOutput {
    #[serde(rename = "sessionId")]
    session_id: String,
    data: String,
}

/// 子进程退出事件载荷。
#[derive(Clone, Serialize)]
struct PtyExit {
    #[serde(rename = "sessionId")]
    session_id: String,
    code: i32,
}

struct Session {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
}

/// 所有 PTY 会话的持有者，作为 Tauri 托管状态。
#[derive(Default)]
pub struct PtyManager {
    sessions: Mutex<HashMap<String, Session>>,
}

impl PtyManager {
    /// 按 [`LaunchSpec`] 在隔离 env 下拉起子进程，启动读/等待线程。
    pub fn spawn(
        &self,
        app: &AppHandle,
        session_id: String,
        spec: LaunchSpec,
        rows: u16,
        cols: u16,
    ) -> Result<(), PtyError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        // 从干净基底构造命令：env_clear 后只注入本账号的隔离 env。
        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        cmd.cwd(&spec.cwd);
        cmd.env_clear();
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::Spawn(e.to_string()))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::Spawn(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        self.sessions.lock().unwrap().insert(
            session_id.clone(),
            Session {
                writer,
                master: pair.master,
            },
        );

        // 读线程：PTY 输出 → 前端事件
        let app_read = app.clone();
        let sid_read = session_id.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).into_owned();
                        let _ = app_read.emit(
                            "pty://output",
                            PtyOutput {
                                session_id: sid_read.clone(),
                                data,
                            },
                        );
                    }
                }
            }
        });

        // 等待线程：子进程退出 → 退出事件
        let app_exit = app.clone();
        let sid_exit = session_id;
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
            let _ = app_exit.emit(
                "pty://exit",
                PtyExit {
                    session_id: sid_exit,
                    code,
                },
            );
        });

        Ok(())
    }

    /// 把前端输入写入指定会话的 PTY。
    pub fn write(&self, session_id: &str, data: &str) -> Result<(), PtyError> {
        let mut map = self.sessions.lock().unwrap();
        let session = map
            .get_mut(session_id)
            .ok_or_else(|| PtyError::NotFound(session_id.to_string()))?;
        session
            .writer
            .write_all(data.as_bytes())
            .map_err(|e| PtyError::Write(e.to_string()))?;
        session
            .writer
            .flush()
            .map_err(|e| PtyError::Write(e.to_string()))
    }

    /// 调整 PTY 视口尺寸（终端 resize 时调用）。
    pub fn resize(&self, session_id: &str, rows: u16, cols: u16) -> Result<(), PtyError> {
        let map = self.sessions.lock().unwrap();
        let session = map
            .get(session_id)
            .ok_or_else(|| PtyError::NotFound(session_id.to_string()))?;
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Write(e.to_string()))
    }

    /// 关闭会话：移除并 drop（PTY 关闭，子进程收到挂断信号而退出）。
    pub fn close(&self, session_id: &str) {
        self.sessions.lock().unwrap().remove(session_id);
    }
}
