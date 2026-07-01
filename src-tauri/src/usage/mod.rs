//! 用量统计（本地 SQLite）。
//!
//! 每个会话一条记录：起任务时 `record_start`（running），退出时 `record_end`
//! （计算时长、状态、退出码）。`summary` 按账号聚合次数/总时长/最近使用。

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("用量库错误: {0}")]
    Db(String),
}

type Result<T> = std::result::Result<T, UsageError>;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub account_id: String,
    pub sessions: i64,
    pub total_duration_sec: i64,
    pub last_used: Option<String>,
}

/// 一次会话的完整上下文——反查本地 CLI 日志、按天聚合 token 用量都要用到。
#[derive(Debug, Clone, PartialEq)]
pub struct UsageSessionInfo {
    pub session_id: String,
    pub account_id: String,
    pub tool: String,
    pub project_dir: String,
    pub started_at: String,
    pub ended_at: String,
}

/// 按天 + 按账号聚合后的 token 用量点位，供前端画图/明细表用。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsagePoint {
    pub day: String,
    pub account_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_tokens: i64,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS usage (
    session_id   TEXT PRIMARY KEY,
    account_id   TEXT NOT NULL,
    tool         TEXT NOT NULL,
    project_dir  TEXT NOT NULL,
    started_at   TEXT NOT NULL,
    ended_at     TEXT,
    duration_sec INTEGER,
    status       TEXT NOT NULL,
    exit_code    INTEGER
);
CREATE TABLE IF NOT EXISTS token_usage (
    session_id         TEXT PRIMARY KEY REFERENCES usage(session_id),
    input_tokens        INTEGER NOT NULL DEFAULT 0,
    output_tokens       INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens    INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens   INTEGER NOT NULL DEFAULT 0,
    matched              INTEGER NOT NULL DEFAULT 0
);";

/// SQLite 用量库，内部 Arc<Mutex<Connection>> 以便跨线程克隆共享。
#[derive(Clone)]
pub struct UsageStore {
    conn: Arc<Mutex<Connection>>,
}

impl UsageStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| UsageError::Db(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(|e| UsageError::Db(e.to_string()))?;
        conn.execute_batch(SCHEMA)
            .map_err(|e| UsageError::Db(e.to_string()))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn record_start(
        &self,
        session_id: &str,
        account_id: &str,
        tool: &str,
        project_dir: &str,
        started_at: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO usage \
             (session_id, account_id, tool, project_dir, started_at, status) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
            rusqlite::params![session_id, account_id, tool, project_dir, started_at],
        )
        .map_err(|e| UsageError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn record_end(&self, session_id: &str, ended_at: &str, exit_code: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let started: Option<String> = conn
            .query_row(
                "SELECT started_at FROM usage WHERE session_id = ?1",
                [session_id],
                |r| r.get(0),
            )
            .ok();
        let duration = started.as_deref().and_then(|s| duration_secs(s, ended_at));
        let status = if exit_code == 0 { "exited" } else { "error" };
        conn.execute(
            "UPDATE usage SET ended_at = ?1, duration_sec = ?2, status = ?3, exit_code = ?4 \
             WHERE session_id = ?5",
            rusqlite::params![ended_at, duration, status, exit_code, session_id],
        )
        .map_err(|e| UsageError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn summary(&self) -> Result<Vec<UsageSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT account_id, COUNT(*), COALESCE(SUM(duration_sec), 0), MAX(started_at) \
                 FROM usage GROUP BY account_id ORDER BY MAX(started_at) DESC",
            )
            .map_err(|e| UsageError::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(UsageSummary {
                    account_id: r.get(0)?,
                    sessions: r.get(1)?,
                    total_duration_sec: r.get(2)?,
                    last_used: r.get(3)?,
                })
            })
            .map_err(|e| UsageError::Db(e.to_string()))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| UsageError::Db(e.to_string()))
    }

    /// 取一次会话的完整上下文（account_id/tool/project_dir/时间窗），
    /// 供反查本地 CLI 日志时定位要扫哪个目录、哪个时间窗。
    /// 会话仍在 running（ended_at 为空）时返回 `Ok(None)`——此时还不知道时间窗终点，扫不了。
    pub fn session_info(&self, session_id: &str) -> Result<Option<UsageSessionInfo>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT session_id, account_id, tool, project_dir, started_at, ended_at \
             FROM usage WHERE session_id = ?1 AND ended_at IS NOT NULL",
            [session_id],
            |r| {
                Ok(UsageSessionInfo {
                    session_id: r.get(0)?,
                    account_id: r.get(1)?,
                    tool: r.get(2)?,
                    project_dir: r.get(3)?,
                    started_at: r.get(4)?,
                    ended_at: r.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| UsageError::Db(e.to_string()))
    }

    /// 已结束但还没有 token_usage 记录的会话（新功能上线前跑过的历史会话）——
    /// 供启动时后台回填扫描用。
    pub fn sessions_missing_token_usage(&self) -> Result<Vec<UsageSessionInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT u.session_id, u.account_id, u.tool, u.project_dir, u.started_at, u.ended_at \
                 FROM usage u LEFT JOIN token_usage t ON t.session_id = u.session_id \
                 WHERE u.ended_at IS NOT NULL AND t.session_id IS NULL",
            )
            .map_err(|e| UsageError::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(UsageSessionInfo {
                    session_id: r.get(0)?,
                    account_id: r.get(1)?,
                    tool: r.get(2)?,
                    project_dir: r.get(3)?,
                    started_at: r.get(4)?,
                    ended_at: r.get(5)?,
                })
            })
            .map_err(|e| UsageError::Db(e.to_string()))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| UsageError::Db(e.to_string()))
    }

    /// 记一次会话的 token 用量。`matched = false` 表示没能在本地日志里关联到
    /// （目录不存在/中转未返回标准 usage 字段等），四个数值也会是 0——
    /// 前端应展示成「未匹配」而非「确认为 0」。
    #[allow(clippy::too_many_arguments)]
    pub fn record_token_usage(
        &self,
        session_id: &str,
        input_tokens: i64,
        output_tokens: i64,
        cache_read_tokens: i64,
        cache_write_tokens: i64,
        matched: bool,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO token_usage \
             (session_id, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, matched) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                session_id,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                matched as i64
            ],
        )
        .map_err(|e| UsageError::Db(e.to_string()))?;
        Ok(())
    }

    /// 按天 + 账号聚合 token 用量，`start_date`/`end_date` 为 `YYYY-MM-DD`（闭区间）。
    /// `account_id` 为 `None` 时不过滤，返回全部账号。
    pub fn token_usage_series(
        &self,
        start_date: &str,
        end_date: &str,
        account_id: Option<&str>,
    ) -> Result<Vec<TokenUsagePoint>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT substr(u.started_at, 1, 10) AS day, u.account_id, \
                        COALESCE(SUM(t.input_tokens), 0), COALESCE(SUM(t.output_tokens), 0), \
                        COALESCE(SUM(t.cache_read_tokens), 0), COALESCE(SUM(t.cache_write_tokens), 0) \
                 FROM usage u LEFT JOIN token_usage t ON t.session_id = u.session_id \
                 WHERE substr(u.started_at, 1, 10) >= ?1 AND substr(u.started_at, 1, 10) <= ?2 \
                   AND (?3 IS NULL OR u.account_id = ?3) \
                 GROUP BY day, u.account_id \
                 ORDER BY day ASC",
            )
            .map_err(|e| UsageError::Db(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![start_date, end_date, account_id], |r| {
                let input: i64 = r.get(2)?;
                let output: i64 = r.get(3)?;
                let cache_read: i64 = r.get(4)?;
                let cache_write: i64 = r.get(5)?;
                Ok(TokenUsagePoint {
                    day: r.get(0)?,
                    account_id: r.get(1)?,
                    input_tokens: input,
                    output_tokens: output,
                    cache_read_tokens: cache_read,
                    cache_write_tokens: cache_write,
                    total_tokens: input + output + cache_read + cache_write,
                })
            })
            .map_err(|e| UsageError::Db(e.to_string()))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| UsageError::Db(e.to_string()))
    }

    /// 结束所有仍 running 的会话（app 退出时调用）：按 started_at→ended_at 计算时长。
    /// 返回结算的会话数。用于捕获「退出 app 时仍开着的会话」的用量，
    /// 否则等待线程随进程终止被杀、record_end 永不触发，时长永远丢失。
    pub fn end_running_sessions(&self, ended_at: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let running: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare("SELECT session_id, started_at FROM usage WHERE status = 'running'")
                .map_err(|e| UsageError::Db(e.to_string()))?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(|e| UsageError::Db(e.to_string()))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| UsageError::Db(e.to_string()))?
        };
        for (sid, started) in &running {
            let duration = duration_secs(started, ended_at);
            conn.execute(
                "UPDATE usage SET ended_at = ?1, duration_sec = ?2, status = 'exited' \
                 WHERE session_id = ?3 AND status = 'running'",
                rusqlite::params![ended_at, duration, sid],
            )
            .map_err(|e| UsageError::Db(e.to_string()))?;
        }
        Ok(running.len())
    }

    /// 启动时清算上次异常退出（进程被杀/崩溃）残留的 running 会话：
    /// 无法恢复真实结束时间，仅改状态避免永久 running，时长留空。返回清算数。
    pub fn reconcile_orphans(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE usage SET status = 'interrupted' WHERE status = 'running'",
            [],
        )
        .map_err(|e| UsageError::Db(e.to_string()))
    }
}

fn duration_secs(started: &str, ended: &str) -> Option<i64> {
    let s = chrono::DateTime::parse_from_rfc3339(started).ok()?;
    let e = chrono::DateTime::parse_from_rfc3339(ended).ok()?;
    Some((e - s).num_seconds().max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> UsageStore {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        UsageStore {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    #[test]
    fn records_and_aggregates_duration() {
        let s = store();
        s.record_start("s1", "acc1", "claude", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        s.record_end("s1", "2026-06-30T00:01:00Z", 0).unwrap();
        s.record_start("s2", "acc1", "claude", "/p", "2026-06-30T01:00:00Z")
            .unwrap();
        s.record_end("s2", "2026-06-30T01:00:30Z", 0).unwrap();

        let sum = s.summary().unwrap();
        assert_eq!(sum.len(), 1);
        assert_eq!(sum[0].account_id, "acc1");
        assert_eq!(sum[0].sessions, 2);
        assert_eq!(sum[0].total_duration_sec, 90);
        assert_eq!(sum[0].last_used.as_deref(), Some("2026-06-30T01:00:00Z"));
    }

    #[test]
    fn running_session_counts_with_zero_duration() {
        let s = store();
        s.record_start("s1", "acc1", "codex", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        let sum = s.summary().unwrap();
        assert_eq!(sum[0].sessions, 1);
        assert_eq!(sum[0].total_duration_sec, 0);
    }

    #[test]
    fn non_zero_exit_marks_error_status() {
        let s = store();
        s.record_start("s1", "acc1", "claude", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        s.record_end("s1", "2026-06-30T00:00:10Z", 1).unwrap();
        // 仍计入聚合
        let sum = s.summary().unwrap();
        assert_eq!(sum[0].sessions, 1);
        assert_eq!(sum[0].total_duration_sec, 10);
    }

    #[test]
    fn end_running_sessions_settles_open_durations() {
        let s = store();
        s.record_start("s1", "acc1", "claude", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        s.record_start("s2", "acc1", "claude", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        // s2 已正常结束，不应被再次改写
        s.record_end("s2", "2026-06-30T00:00:20Z", 0).unwrap();

        let n = s.end_running_sessions("2026-06-30T00:05:00Z").unwrap();
        assert_eq!(n, 1, "只结算仍 running 的 s1");

        let sum = s.summary().unwrap();
        // s1: 300s（退出时结算） + s2: 20s
        assert_eq!(sum[0].total_duration_sec, 320);
    }

    #[test]
    fn reconcile_orphans_clears_stale_running() {
        let s = store();
        s.record_start("s1", "acc1", "claude", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        let n = s.reconcile_orphans().unwrap();
        assert_eq!(n, 1);
        // 再次清算无残留
        assert_eq!(s.reconcile_orphans().unwrap(), 0);
        // 仍计入会话数、时长为 0（无法恢复）
        let sum = s.summary().unwrap();
        assert_eq!(sum[0].sessions, 1);
        assert_eq!(sum[0].total_duration_sec, 0);
    }

    #[test]
    fn session_info_returns_none_while_running() {
        let s = store();
        s.record_start("s1", "acc1", "claude", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        assert_eq!(s.session_info("s1").unwrap(), None, "还没结束，时间窗终点未知");
        s.record_end("s1", "2026-06-30T00:01:00Z", 0).unwrap();
        let info = s.session_info("s1").unwrap().expect("已结束应能取到");
        assert_eq!(info.account_id, "acc1");
        assert_eq!(info.project_dir, "/p");
        assert_eq!(info.ended_at, "2026-06-30T00:01:00Z");
    }

    #[test]
    fn sessions_missing_token_usage_excludes_running_and_recorded() {
        let s = store();
        s.record_start("s1", "acc1", "claude", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        s.record_end("s1", "2026-06-30T00:01:00Z", 0).unwrap();
        s.record_start("s2", "acc1", "claude", "/p", "2026-06-30T01:00:00Z")
            .unwrap();
        s.record_end("s2", "2026-06-30T01:01:00Z", 0).unwrap();
        s.record_token_usage("s2", 10, 5, 0, 0, true).unwrap();
        // s3 仍 running，不该被当成「缺失」拿去扫（时间窗终点未知）
        s.record_start("s3", "acc1", "claude", "/p", "2026-06-30T02:00:00Z")
            .unwrap();

        let missing = s.sessions_missing_token_usage().unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].session_id, "s1");
    }

    #[test]
    fn token_usage_series_aggregates_by_day_and_account() {
        let s = store();
        s.record_start("s1", "acc1", "claude", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        s.record_end("s1", "2026-06-30T00:01:00Z", 0).unwrap();
        s.record_token_usage("s1", 100, 10, 5, 0, true).unwrap();

        s.record_start("s2", "acc1", "claude", "/p", "2026-06-30T05:00:00Z")
            .unwrap();
        s.record_end("s2", "2026-06-30T05:01:00Z", 0).unwrap();
        s.record_token_usage("s2", 50, 5, 0, 0, true).unwrap();

        s.record_start("s3", "acc2", "claude", "/p", "2026-06-30T00:00:00Z")
            .unwrap();
        s.record_end("s3", "2026-06-30T00:01:00Z", 0).unwrap();
        s.record_token_usage("s3", 7, 1, 0, 0, true).unwrap();

        // 同一天 acc1 的两个 session 应合并成一行
        let all = s
            .token_usage_series("2026-06-30", "2026-06-30", None)
            .unwrap();
        assert_eq!(all.len(), 2, "按天+账号分组：acc1 一行、acc2 一行");
        let acc1 = all.iter().find(|p| p.account_id == "acc1").unwrap();
        assert_eq!(acc1.input_tokens, 150);
        assert_eq!(acc1.output_tokens, 15);
        assert_eq!(acc1.cache_read_tokens, 5);
        assert_eq!(acc1.total_tokens, 170);

        let filtered = s
            .token_usage_series("2026-06-30", "2026-06-30", Some("acc2"))
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].account_id, "acc2");
        assert_eq!(filtered[0].total_tokens, 8);
    }

    #[test]
    fn token_usage_series_excludes_out_of_range_days() {
        let s = store();
        s.record_start("s1", "acc1", "claude", "/p", "2026-06-29T00:00:00Z")
            .unwrap();
        s.record_end("s1", "2026-06-29T00:01:00Z", 0).unwrap();
        s.record_token_usage("s1", 100, 0, 0, 0, true).unwrap();

        let series = s
            .token_usage_series("2026-06-30", "2026-07-01", None)
            .unwrap();
        assert!(series.is_empty(), "范围外的会话不该出现");
    }
}
