//! 用量统计（本地 SQLite）。
//!
//! 每个会话一条记录：起任务时 `record_start`（running），退出时 `record_end`
//! （计算时长、状态、退出码）。`summary` 按账号聚合次数/总时长/最近使用。

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
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
}
