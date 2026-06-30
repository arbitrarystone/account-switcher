//! 全局默认配置写入器：原子写 + 备份，安全改写工具的全局配置文件（spec §8）。

mod claude;
mod codex;

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub use claude::{clear_claude_default, write_claude_default};
pub use codex::{clear_codex_default, write_codex_default};

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum ConfigError {
    #[error("读取配置失败: {0}")]
    Read(String),
    #[error("解析配置失败: {0}")]
    Parse(String),
    #[error("写入配置失败: {0}")]
    Write(String),
}

type Result<T> = std::result::Result<T, ConfigError>;

/// 在原路径基础上追加后缀（如 settings.json -> settings.json.bak）。
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// 原子写：先备份原文件（.bak），写临时文件（.tmp）再 rename 覆盖。
/// 权限/IO 失败时只报错，不破坏现有文件。
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| ConfigError::Write(e.to_string()))?;
    }
    if path.exists() {
        let backup = with_suffix(path, ".bak");
        fs::copy(path, &backup).map_err(|e| ConfigError::Write(e.to_string()))?;
    }
    let tmp = with_suffix(path, ".tmp");
    fs::write(&tmp, content).map_err(|e| ConfigError::Write(e.to_string()))?;
    fs::rename(&tmp, path).map_err(|e| ConfigError::Write(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_backs_up_no_tmp_left() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/conf.txt");
        atomic_write(&path, "v1").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "v1");

        atomic_write(&path, "v2").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "v2");
        assert_eq!(
            fs::read_to_string(with_suffix(&path, ".bak")).unwrap(),
            "v1"
        );
        assert!(!with_suffix(&path, ".tmp").exists());
    }
}
