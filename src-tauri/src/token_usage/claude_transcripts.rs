//! 从 Claude Code 本地会话记录（`~/.claude/projects/<sanitized-cwd>/*.jsonl`）
//! 里反查某次 account-switcher 会话的真实 token 用量。
//!
//! account-switcher 的终端只是 PTY 透传，看不到 API 流量本身；但 claude CLI
//! 会把每条 assistant 回复的 `usage`（含 input/output/cache token 数）落到本地
//! jsonl。按 `cwd` 定位到目录、按每条消息自己的 `timestamp` 落在
//! `[started_at, ended_at]` 窗口内过滤并累加——这样天然兼容 `--continue`
//! 续接同一份文件的情况（旧消息时间戳在窗口外，不会被重复计入）。

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use chrono::{DateTime, FixedOffset};
use serde_json::Value;

use super::TokenCounts;

/// Claude Code 把项目目录名里的 `/`（以及 Windows 的 `\`）替换成 `-` 作为
/// 会话记录的子目录名（已用真实 claude 2.1.158 验证过 macOS 行为）。
fn sanitize_project_dir(project_dir: &Path) -> String {
    project_dir.to_string_lossy().replace(['\\', '/'], "-")
}

fn parse_ts(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(s).ok()
}

/// 累加一行 assistant 消息里的 usage 字段（缺失字段按 0 处理）。
fn accumulate(counts: &mut TokenCounts, usage: &Value) {
    let get = |key: &str| usage.get(key).and_then(Value::as_i64).unwrap_or(0);
    counts.input_tokens += get("input_tokens");
    counts.output_tokens += get("output_tokens");
    counts.cache_read_tokens += get("cache_read_input_tokens");
    counts.cache_write_tokens += get("cache_creation_input_tokens");
}

/// 扫描单个 jsonl 文件，把落在时间窗内的 assistant usage 累加进 `counts`。
/// 单行解析失败（文件正在被写、或格式不认识）直接跳过，不中断整体扫描。
fn scan_file(path: &Path, start: DateTime<FixedOffset>, end: DateTime<FixedOffset>, counts: &mut TokenCounts) {
    let Ok(file) = File::open(path) else { return };
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else { continue };
        if entry.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(ts) = entry.get("timestamp").and_then(Value::as_str).and_then(parse_ts) else {
            continue;
        };
        if ts < start || ts > end {
            continue;
        }
        let Some(usage) = entry.get("message").and_then(|m| m.get("usage")) else {
            continue;
        };
        accumulate(counts, usage);
    }
}

/// `home_dir` 显式传入（而非内部解析）以便单元测试注入临时目录。
pub fn scan(home_dir: &Path, project_dir: &Path, started_at: &str, ended_at: &str) -> Option<TokenCounts> {
    let dir = home_dir
        .join(".claude")
        .join("projects")
        .join(sanitize_project_dir(project_dir));
    let entries = std::fs::read_dir(&dir).ok()?;

    let start = parse_ts(started_at)?;
    let end = parse_ts(ended_at)?;

    let mut counts = TokenCounts::default();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            scan_file(&path, start, end, &mut counts);
        }
    }
    Some(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) {
        let mut f = File::create(dir.join(name)).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
    }

    fn assistant_line(timestamp: &str, input: i64, output: i64, cache_read: i64, cache_write: i64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{timestamp}","message":{{"usage":{{"input_tokens":{input},"output_tokens":{output},"cache_read_input_tokens":{cache_read},"cache_creation_input_tokens":{cache_write}}}}}}}"#
        )
    }

    #[test]
    fn returns_none_when_project_dir_missing() {
        let home = tempfile::tempdir().unwrap();
        let result = scan(
            home.path(),
            Path::new("/no/such/project"),
            "2026-06-30T00:00:00Z",
            "2026-06-30T01:00:00Z",
        );
        assert!(result.is_none());
    }

    #[test]
    fn sums_usage_within_time_window_across_files() {
        let home = tempfile::tempdir().unwrap();
        let project = Path::new("/Users/dev/myproj");
        let dir = home.path().join(".claude/projects/-Users-dev-myproj");
        std::fs::create_dir_all(&dir).unwrap();

        write_jsonl(
            &dir,
            "a.jsonl",
            &[
                &assistant_line("2026-06-30T00:00:10Z", 100, 10, 0, 0),
                &assistant_line("2026-06-30T00:00:20Z", 50, 5, 20, 0),
                // 窗口外，不应计入
                &assistant_line("2026-06-30T02:00:00Z", 9999, 9999, 0, 0),
            ],
        );
        write_jsonl(
            &dir,
            "b.jsonl",
            &[&assistant_line("2026-06-30T00:00:30Z", 30, 3, 0, 7)],
        );

        let counts = scan(
            home.path(),
            project,
            "2026-06-30T00:00:00Z",
            "2026-06-30T01:00:00Z",
        )
        .expect("project dir 存在应返回 Some");

        assert_eq!(counts.input_tokens, 180);
        assert_eq!(counts.output_tokens, 18);
        assert_eq!(counts.cache_read_tokens, 20);
        assert_eq!(counts.cache_write_tokens, 7);
    }

    #[test]
    fn skips_malformed_and_non_assistant_lines() {
        let home = tempfile::tempdir().unwrap();
        let project = Path::new("/p");
        let dir = home.path().join(".claude/projects/-p");
        std::fs::create_dir_all(&dir).unwrap();

        write_jsonl(
            &dir,
            "a.jsonl",
            &[
                "not json at all {{{",
                r#"{"type":"user","timestamp":"2026-06-30T00:00:05Z"}"#,
                &assistant_line("2026-06-30T00:00:10Z", 10, 1, 0, 0),
                "",
            ],
        );

        let counts = scan(home.path(), project, "2026-06-30T00:00:00Z", "2026-06-30T01:00:00Z")
            .unwrap();
        assert_eq!(counts.input_tokens, 10);
        assert_eq!(counts.output_tokens, 1);
    }

    #[test]
    fn returns_some_zero_when_dir_exists_but_no_lines_match() {
        let home = tempfile::tempdir().unwrap();
        let project = Path::new("/empty");
        let dir = home.path().join(".claude/projects/-empty");
        std::fs::create_dir_all(&dir).unwrap();
        write_jsonl(&dir, "a.jsonl", &[]);

        let counts = scan(home.path(), project, "2026-06-30T00:00:00Z", "2026-06-30T01:00:00Z")
            .unwrap();
        assert_eq!(counts, TokenCounts::default());
    }
}
