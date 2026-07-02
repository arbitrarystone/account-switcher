//! 伪终端会话管理。
//!
//! 每个会话 = 一对 PTY + 子进程 + 读线程 + 等待线程。
//! 输出/退出经 [`OutputSink`] 抽象转发：生产用 [`TauriSink`]（推 Tauri 事件），
//! 测试用内存收集器（无需 Tauri runtime 即可验证 env 隔离）。
//!
//! 多会话由 [`PtyManager`] 统一持有 —— 这是 M3 并发隔离的基础。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::account::Tool;
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

/// 会话输出/退出的接收端。解耦具体传输，便于测试。
pub trait OutputSink: Send + Sync + 'static {
    fn output(&self, session_id: &str, data: &str);
    fn exit(&self, session_id: &str, code: i32);
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

/// 生产实现：把会话输出/退出转发为 Tauri 前端事件，并在退出时记录用量。
pub struct TauriSink {
    app: AppHandle,
    usage: crate::usage::UsageStore,
}

impl TauriSink {
    pub fn new(app: AppHandle, usage: crate::usage::UsageStore) -> Self {
        Self { app, usage }
    }
}

impl OutputSink for TauriSink {
    fn output(&self, session_id: &str, data: &str) {
        let _ = self.app.emit(
            "pty://output",
            PtyOutput {
                session_id: session_id.to_string(),
                data: data.to_string(),
            },
        );
    }

    fn exit(&self, session_id: &str, code: i32) {
        let ended_at = chrono::Utc::now().to_rfc3339();
        let _ = self.usage.record_end(session_id, &ended_at, code);
        record_token_usage_for_session(&self.app, &self.usage, session_id);
        let _ = self.app.emit(
            "pty://exit",
            PtyExit {
                session_id: session_id.to_string(),
                code,
            },
        );
    }
}

/// 会话结束后反查本地 CLI 日志、记一次 token 用量。找不到账号/工具/项目目录
/// 匹配的日志时记为「未匹配」（`matched=false`）——不是确认为 0，只是没查到。
/// 供 [`TauriSink::exit`] 和启动时的历史回填（[`backfill_token_usage`]）共用。
pub fn record_token_usage_for_session(
    app: &AppHandle,
    usage: &crate::usage::UsageStore,
    session_id: &str,
) {
    let Ok(Some(info)) = usage.session_info(session_id) else {
        return;
    };
    let Some(tool) = Tool::parse(&info.tool) else {
        return;
    };
    let Ok(home_dir) = app.path().home_dir() else {
        return;
    };
    let counts = crate::token_usage::scan(
        tool,
        &home_dir,
        std::path::Path::new(&info.project_dir),
        &info.started_at,
        &info.ended_at,
    );
    let _ = match counts {
        Some(c) => usage.record_token_usage(
            session_id,
            c.input_tokens,
            c.output_tokens,
            c.cache_read_tokens,
            c.cache_write_tokens,
            true,
        ),
        None => usage.record_token_usage(session_id, 0, 0, 0, 0, false),
    };
}

/// 启动时一次性回填：给这个功能上线前已经跑过的历史会话补齐 token_usage，
/// 让用量页面一打开就有历史数据可看。在后台线程跑，不阻塞启动。
pub fn backfill_token_usage(app: AppHandle, usage: crate::usage::UsageStore) {
    std::thread::spawn(move || {
        let Ok(missing) = usage.sessions_missing_token_usage() else {
            return;
        };
        for info in missing {
            record_token_usage_for_session(&app, &usage, &info.session_id);
        }
    });
}

/// Windows 上 npm 装的 claude/codex 是 `claude.cmd` 批处理垫片，且同目录还有
/// 一个**无扩展名**的 `claude`（Git Bash 用的 sh 脚本）。portable-pty 的 PATH
/// 搜索会先命中无扩展名文件，CreateProcessW 对非 PE 文件报「%1 不是有效的
/// Win32 应用程序」（os error 193）。故拉起前按 PATHEXT 语义自行解析出真正
/// 可执行的完整路径（.cmd/.bat 由 CreateProcess 隐式经 cmd.exe 执行）。
/// 带路径分隔符或已带可执行扩展名的输入原样返回；找不到时也原样返回，
/// 让后续报错仍指向原始命令名。逻辑跨平台可测，仅在 Windows 构建时启用。
/// 把错误文本里出现的敏感 env 值（Token/Key）替换为 `***`。
/// CreateProcessW 失败时 portable-pty 的报错会附带完整命令行——其中
/// `--settings` JSON 里就有明文 token，直接透传会把密钥打到前端状态栏
/// （spec §8：Token 绝不进日志/错误信息）。
fn redact_secrets(msg: &str, env: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = msg.to_string();
    for (k, v) in env {
        let upper = k.to_ascii_uppercase();
        if (upper.contains("TOKEN") || upper.contains("KEY")) && !v.is_empty() {
            out = out.replace(v.as_str(), "***");
        }
    }
    out
}

#[cfg_attr(not(windows), allow(dead_code))]
fn resolve_windows_program(program: &str, path_value: Option<&str>) -> String {
    const EXTS: [&str; 4] = [".com", ".exe", ".bat", ".cmd"];
    let lower = program.to_ascii_lowercase();
    if program.contains('/') || program.contains('\\') || EXTS.iter().any(|e| lower.ends_with(e))
    {
        return program.to_string();
    }
    let Some(path_value) = path_value else {
        return program.to_string();
    };
    for dir in std::env::split_paths(path_value) {
        // 同目录内按 PATHEXT 顺序优先，且**跳过**无扩展名的同名文件。
        for ext in EXTS {
            let candidate = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }
    program.to_string()
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
        sink: Arc<dyn OutputSink>,
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
        // Windows：先解析出 .cmd/.exe 真身，避免命中无扩展名的 sh 垫片（os error 193）。
        // 注：Windows 上环境变量名大小写不敏感，实际常存作 "Path"，需忽略大小写查找。
        #[cfg(windows)]
        let program = {
            let path_value = spec
                .env
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
                .map(|(_, v)| v.as_str());
            resolve_windows_program(&spec.program, path_value)
        };
        #[cfg(not(windows))]
        let program = spec.program.clone();
        let mut cmd = CommandBuilder::new(&program);
        cmd.args(&spec.args);
        cmd.cwd(&spec.cwd);
        cmd.env_clear();
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        // PTY 由 xterm.js 渲染：显式声明终端能力，否则 GUI app 启动时无 TERM，
        // claude/codex 检测不到彩色终端而退化为无颜色输出。
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            // CreateProcessW 类错误会回显完整命令行（含 --settings 里的 token），脱敏后再抛
            .map_err(|e| PtyError::Spawn(redact_secrets(&e.to_string(), &spec.env)))?;
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

        // 读线程：PTY 输出 → sink。
        // 维护跨读取的 pending 缓冲，避免多字节 UTF-8 字符在 8192 边界被切断成乱码。
        let sink_read = Arc::clone(&sink);
        let sid_read = session_id.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut pending: Vec<u8> = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.extend_from_slice(&buf[..n]);
                        // 只解码到最后一个完整 UTF-8 边界，残余不完整字节留到下次。
                        let valid_up_to = match std::str::from_utf8(&pending) {
                            Ok(s) => s.len(),
                            Err(e) => e.valid_up_to(),
                        };
                        if valid_up_to > 0 {
                            let data =
                                String::from_utf8_lossy(&pending[..valid_up_to]).into_owned();
                            sink_read.output(&sid_read, &data);
                            pending.drain(..valid_up_to);
                        }
                        // 防御：残余超过 UTF-8 单字符最大长度仍无效，必是坏字节，lossy 冲刷。
                        if pending.len() > 4 {
                            let data = String::from_utf8_lossy(&pending).into_owned();
                            sink_read.output(&sid_read, &data);
                            pending.clear();
                        }
                    }
                }
            }
        });

        // 等待线程：子进程退出 → sink
        let sid_exit = session_id;
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
            sink.exit(&sid_exit, code);
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::Duration;

    /// 测试用内存收集器。
    #[derive(Default)]
    struct VecSink {
        outputs: Mutex<Vec<(String, String)>>,
        exits: Mutex<Vec<(String, i32)>>,
    }

    impl VecSink {
        fn output_for(&self, sid: &str) -> String {
            self.outputs
                .lock()
                .unwrap()
                .iter()
                .filter(|(s, _)| s == sid)
                .map(|(_, d)| d.clone())
                .collect()
        }
        fn exit_count(&self) -> usize {
            self.exits.lock().unwrap().len()
        }
    }

    impl OutputSink for VecSink {
        fn output(&self, session_id: &str, data: &str) {
            self.outputs
                .lock()
                .unwrap()
                .push((session_id.to_string(), data.to_string()));
        }
        fn exit(&self, session_id: &str, code: i32) {
            self.exits
                .lock()
                .unwrap()
                .push((session_id.to_string(), code));
        }
    }

    /// 构造一个回显两个鉴权变量的「假 CLI」启动规格。
    fn echo_env_spec(base: &str, token: &str) -> LaunchSpec {
        let mut env = BTreeMap::new();
        env.insert("ANTHROPIC_BASE_URL".to_string(), base.to_string());
        env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), token.to_string());
        LaunchSpec {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'BASE=%s TOK=%s' \"$ANTHROPIC_BASE_URL\" \"$ANTHROPIC_AUTH_TOKEN\""
                    .to_string(),
            ],
            env,
            cwd: std::env::temp_dir(),
        }
    }

    fn wait_exits(sink: &VecSink, n: usize) {
        for _ in 0..50 {
            if sink.exit_count() >= n {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("超时：会话未在预期时间内退出");
    }

    #[test]
    fn concurrent_sessions_get_isolated_env() {
        let mgr = PtyManager::default();
        let sink = Arc::new(VecSink::default());

        // 两个账号并发起会话
        mgr.spawn(
            sink.clone(),
            "sa".to_string(),
            echo_env_spec("https://a.example.com", "tok-a"),
            24,
            80,
        )
        .unwrap();
        mgr.spawn(
            sink.clone(),
            "sb".to_string(),
            echo_env_spec("https://b.example.com", "tok-b"),
            24,
            80,
        )
        .unwrap();

        wait_exits(&sink, 2);

        let out_a = sink.output_for("sa");
        let out_b = sink.output_for("sb");

        // 各自拿到正确的注入值
        assert!(
            out_a.contains("BASE=https://a.example.com"),
            "A 实际: {out_a:?}"
        );
        assert!(out_a.contains("TOK=tok-a"), "A 实际: {out_a:?}");
        assert!(
            out_b.contains("BASE=https://b.example.com"),
            "B 实际: {out_b:?}"
        );
        assert!(out_b.contains("TOK=tok-b"), "B 实际: {out_b:?}");

        // 真隔离：彼此不串号
        assert!(!out_a.contains("tok-b"), "A 串入了 B 的 token");
        assert!(!out_b.contains("tok-a"), "B 串入了 A 的 token");
    }

    #[test]
    fn env_clear_strips_inherited_vars() {
        // 父进程设一个脏变量，子进程不应继承（env_clear 生效）
        std::env::set_var("ACCSW_TEST_LEAK", "should-not-appear");
        let mgr = PtyManager::default();
        let sink = Arc::new(VecSink::default());
        let mut spec = echo_env_spec("https://x.example.com", "tok-x");
        spec.args = vec![
            "-c".to_string(),
            "printf 'LEAK=[%s]' \"$ACCSW_TEST_LEAK\"".to_string(),
        ];
        mgr.spawn(sink.clone(), "sx".to_string(), spec, 24, 80)
            .unwrap();
        wait_exits(&sink, 1);
        let out = sink.output_for("sx");
        assert!(out.contains("LEAK=[]"), "继承了未注入的父进程变量: {out:?}");
        std::env::remove_var("ACCSW_TEST_LEAK");
    }

    #[test]
    fn redact_secrets_strips_token_values_from_error_text() {
        // Windows CreateProcessW 报错会回显完整命令行（含 --settings 里的明文 token）
        let mut env = std::collections::BTreeMap::new();
        env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), "fe_oa_secret123".to_string());
        env.insert("ANTHROPIC_API_KEY".to_string(), String::new()); // 空值不参与替换
        env.insert("PATH".to_string(), "/usr/bin".to_string()); // 非敏感不替换

        let msg = r#"CreateProcessW "claude --settings {\"ANTHROPIC_AUTH_TOKEN\":\"fe_oa_secret123\"}" in /usr/bin failed"#;
        let out = redact_secrets(msg, &env);
        assert!(!out.contains("fe_oa_secret123"), "token 未被脱敏: {out}");
        assert!(out.contains("***"));
        assert!(out.contains("/usr/bin"), "非敏感值不应被动到");
    }

    // ── Windows 程序名解析（跨平台可测的纯逻辑）────────────

    #[test]
    fn windows_resolver_prefers_cmd_over_extensionless() {
        // 模拟 npm 全局目录：同名的无扩展名 sh 垫片 + claude.cmd 并存，
        // 必须选 .cmd（CreateProcessW 对 sh 脚本报 os error 193）。
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("claude"), "#!/bin/sh\n").unwrap();
        std::fs::write(dir.path().join("claude.cmd"), "@echo off\n").unwrap();

        let resolved = resolve_windows_program("claude", dir.path().to_str());
        assert!(
            resolved.to_ascii_lowercase().ends_with("claude.cmd"),
            "应解析到 .cmd 真身，实际: {resolved}"
        );
    }

    #[test]
    fn windows_resolver_searches_path_dirs_in_order() {
        let dir_a = tempfile::tempdir().unwrap(); // 无匹配
        let dir_b = tempfile::tempdir().unwrap();
        std::fs::write(dir_b.path().join("codex.exe"), "MZ").unwrap();
        let path_value = std::env::join_paths([dir_a.path(), dir_b.path()])
            .unwrap()
            .to_string_lossy()
            .to_string();

        let resolved = resolve_windows_program("codex", Some(&path_value));
        assert!(
            resolved.to_ascii_lowercase().ends_with("codex.exe"),
            "应在第二个 PATH 目录找到 .exe，实际: {resolved}"
        );
    }

    #[test]
    fn windows_resolver_passes_through_paths_and_extensions() {
        // 带路径分隔符 / 已带扩展名 / PATH 里找不到 → 原样返回
        assert_eq!(
            resolve_windows_program("C:\\tools\\claude.cmd", Some("ignored")),
            "C:\\tools\\claude.cmd"
        );
        assert_eq!(
            resolve_windows_program("claude.CMD", Some("ignored")),
            "claude.CMD"
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_windows_program("claude", empty.path().to_str()),
            "claude"
        );
        assert_eq!(resolve_windows_program("claude", None), "claude");
    }
}
