//! 从 Codex 本地会话记录（`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`）
//! 里反查某次 account-switcher 会话的真实 token 用量。
//!
//! Codex 的 `session_meta.model_provider` 就是 [`crate::adapter::codex::PROVIDER_ID`]
//! （account-switcher 每次按会话启动都固定用这个 provider id），比 Claude 那边只能
//! 靠 cwd + 时间窗猜要可靠得多——外部手动跑的 codex 会话不会用这个 provider。
//!
//! `token_count` 事件里的 `total_token_usage` 是**整个文件累计值**（不是增量），
//! 取文件里最后一条即为该次会话的总量；`input_tokens`/`output_tokens` 已经把
//! `cached_input_tokens`/`reasoning_output_tokens` 计入在内（二者是子集不是叠加），
//! 所以这里不额外填 cache_read/cache_write，避免 [`super::TokenCounts::total`] 重复计数。

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, FixedOffset};
use serde_json::Value;

use super::TokenCounts;
use crate::adapter::codex::PROVIDER_ID;

/// 匹配时对时间窗放宽的容差：账号切换器每次都是全新会话（无续接），
/// session_meta 的时间通常和 started_at 相差在几秒内，留足量余量应对慢中转。
const TOLERANCE_SECS: i64 = 120;

fn parse_ts(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(s).ok()
}

/// 只看 started_at / ended_at 所在的日期文件夹，不做无界扫描；
/// 极端跨天的长会话只覆盖首尾两天，够用且有界。
fn day_folders(home_dir: &Path, start: DateTime<FixedOffset>, end: DateTime<FixedOffset>) -> Vec<PathBuf> {
    let base = home_dir.join(".codex").join("sessions");
    let day_path = |d: chrono::NaiveDate| {
        base.join(format!("{:04}", d.year()))
            .join(format!("{:02}", d.month()))
            .join(format!("{:02}", d.day()))
    };
    let start_date = start.date_naive();
    let end_date = end.date_naive();
    if start_date == end_date {
        vec![day_path(start_date)]
    } else {
        vec![day_path(start_date), day_path(end_date)]
    }
}

struct SessionMeta {
    cwd: String,
    model_provider: String,
    timestamp: DateTime<FixedOffset>,
}

/// `session_meta` 通常是文件第一行，稳妥起见往后多看几行再放弃。
fn read_session_meta(path: &Path) -> Option<SessionMeta> {
    let file = File::open(path).ok()?;
    for line in BufReader::new(file).lines().take(5) {
        let line = line.ok()?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else { continue };
        if entry.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }
        let payload = entry.get("payload")?;
        return Some(SessionMeta {
            cwd: payload.get("cwd").and_then(Value::as_str)?.to_string(),
            model_provider: payload
                .get("model_provider")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            timestamp: payload.get("timestamp").and_then(Value::as_str).and_then(parse_ts)?,
        });
    }
    None
}

/// 扫全文件找最后一条 `token_count` 事件的 `total_token_usage`。
fn last_token_count(path: &Path) -> Option<TokenCounts> {
    let file = File::open(path).ok()?;
    let mut last = None;
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else { continue };
        if entry.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let Some(payload) = entry.get("payload") else { continue };
        if payload.get("type").and_then(Value::as_str) != Some("token_count") {
            continue;
        }
        let Some(usage) = payload.get("info").and_then(|i| i.get("total_token_usage")) else {
            continue;
        };
        last = Some(TokenCounts {
            input_tokens: usage.get("input_tokens").and_then(Value::as_i64).unwrap_or(0),
            output_tokens: usage.get("output_tokens").and_then(Value::as_i64).unwrap_or(0),
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        });
    }
    last
}

/// `home_dir` 显式传入（而非内部解析）以便单元测试注入临时目录。
pub fn scan(home_dir: &Path, project_dir: &Path, started_at: &str, ended_at: &str) -> Option<TokenCounts> {
    let start = parse_ts(started_at)?;
    let end = parse_ts(ended_at)?;
    let window_start = start - chrono::Duration::seconds(TOLERANCE_SECS);
    let window_end = end + chrono::Duration::seconds(TOLERANCE_SECS);
    let project_str = project_dir.to_string_lossy();

    for dir in day_folders(home_dir, start, end) {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(meta) = read_session_meta(&path) else { continue };
            if meta.model_provider != PROVIDER_ID || meta.cwd != project_str {
                continue;
            }
            if meta.timestamp < window_start || meta.timestamp > window_end {
                continue;
            }
            return Some(last_token_count(&path).unwrap_or_default());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_lines(path: &Path, lines: &[&str]) {
        let mut f = File::create(path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
    }

    fn session_meta_line(cwd: &str, provider: &str, ts: &str) -> String {
        format!(
            r#"{{"timestamp":"{ts}","type":"session_meta","payload":{{"cwd":"{cwd}","model_provider":"{provider}","timestamp":"{ts}"}}}}"#
        )
    }

    fn token_count_line(input: i64, output: i64) -> String {
        format!(
            r#"{{"timestamp":"2026-06-04T12:00:00Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"output_tokens":{output},"cached_input_tokens":0,"reasoning_output_tokens":0,"total_tokens":{}}}}}}}}}"#,
            input + output
        )
    }

    #[test]
    fn matches_by_provider_cwd_and_time_takes_last_token_count() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".codex/sessions/2026/06/04");
        std::fs::create_dir_all(&dir).unwrap();

        write_lines(
            &dir.join("rollout-a.jsonl"),
            &[
                &session_meta_line("/proj", "accsw", "2026-06-04T12:00:00Z"),
                &token_count_line(100, 10),
                &token_count_line(150, 20), // 累计值，取最后一条
            ],
        );

        let counts = scan(
            home.path(),
            Path::new("/proj"),
            "2026-06-04T12:00:00Z",
            "2026-06-04T12:05:00Z",
        )
        .expect("应匹配到");
        assert_eq!(counts.input_tokens, 150);
        assert_eq!(counts.output_tokens, 20);
        assert_eq!(counts.cache_read_tokens, 0);
    }

    #[test]
    fn ignores_files_with_other_provider() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".codex/sessions/2026/06/04");
        std::fs::create_dir_all(&dir).unwrap();

        write_lines(
            &dir.join("rollout-external.jsonl"),
            &[
                &session_meta_line("/proj", "openai", "2026-06-04T12:00:00Z"),
                &token_count_line(9999, 9999),
            ],
        );

        let counts = scan(
            home.path(),
            Path::new("/proj"),
            "2026-06-04T12:00:00Z",
            "2026-06-04T12:05:00Z",
        );
        assert!(counts.is_none(), "外部 provider 的会话不该被匹配到");
    }

    #[test]
    fn ignores_files_outside_time_tolerance() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".codex/sessions/2026/06/04");
        std::fs::create_dir_all(&dir).unwrap();

        write_lines(
            &dir.join("rollout-stale.jsonl"),
            &[
                &session_meta_line("/proj", "accsw", "2026-06-04T08:00:00Z"),
                &token_count_line(5, 5),
            ],
        );

        let counts = scan(
            home.path(),
            Path::new("/proj"),
            "2026-06-04T12:00:00Z",
            "2026-06-04T12:05:00Z",
        );
        assert!(counts.is_none(), "时间窗外的旧会话不该被匹配到");
    }

    #[test]
    fn returns_none_when_no_day_folder() {
        let home = tempfile::tempdir().unwrap();
        let counts = scan(
            home.path(),
            Path::new("/proj"),
            "2026-06-04T12:00:00Z",
            "2026-06-04T12:05:00Z",
        );
        assert!(counts.is_none());
    }
}
