use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::{atomic_write, ConfigError, Result};

fn read_json(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let s = fs::read_to_string(path).map_err(|e| ConfigError::Read(e.to_string()))?;
    if s.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&s).map_err(|e| ConfigError::Parse(e.to_string()))
}

fn write_json(path: &Path, root: &Value) -> Result<()> {
    let s = serde_json::to_string_pretty(root).map_err(|e| ConfigError::Write(e.to_string()))?;
    atomic_write(path, &s)
}

/// 写 `<claude_dir>/settings.json` 的 env 块（合并，保留其他字段）。
pub fn write_claude_default(
    claude_dir: &Path,
    base_url: &str,
    token: &str,
    model: Option<&str>,
) -> Result<()> {
    let path = claude_dir.join("settings.json");
    let mut root = read_json(&path)?;
    if !root.is_object() {
        root = json!({});
    }
    let obj = root.as_object_mut().unwrap();
    let env = obj.entry("env").or_insert_with(|| json!({}));
    if !env.is_object() {
        *env = json!({});
    }
    let env_obj = env.as_object_mut().unwrap();
    env_obj.insert("ANTHROPIC_BASE_URL".into(), json!(base_url));
    env_obj.insert("ANTHROPIC_AUTH_TOKEN".into(), json!(token));
    match model {
        Some(m) => {
            env_obj.insert("ANTHROPIC_MODEL".into(), json!(m));
        }
        None => {
            env_obj.remove("ANTHROPIC_MODEL");
        }
    }
    write_json(&path, &root)
}

/// 清除 Claude 全局默认（仅移除本应用注入的键，保留用户其他 env）。
pub fn clear_claude_default(claude_dir: &Path) -> Result<()> {
    let path = claude_dir.join("settings.json");
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json(&path)?;
    if let Some(env_obj) = root
        .as_object_mut()
        .and_then(|o| o.get_mut("env"))
        .and_then(|e| e.as_object_mut())
    {
        env_obj.remove("ANTHROPIC_BASE_URL");
        env_obj.remove("ANTHROPIC_AUTH_TOKEN");
        env_obj.remove("ANTHROPIC_MODEL");
    }
    write_json(&path, &root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_env_block_preserving_others() {
        let dir = tempfile::tempdir().unwrap();
        let cdir = dir.path();
        fs::write(
            cdir.join("settings.json"),
            r#"{"theme":"dark","env":{"FOO":"bar"}}"#,
        )
        .unwrap();
        write_claude_default(cdir, "https://relay.example.com", "tok", Some("opus")).unwrap();

        let v: Value =
            serde_json::from_str(&fs::read_to_string(cdir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["env"]["FOO"], "bar");
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "https://relay.example.com");
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "tok");
        assert_eq!(v["env"]["ANTHROPIC_MODEL"], "opus");
    }

    #[test]
    fn clear_removes_only_injected_keys() {
        let dir = tempfile::tempdir().unwrap();
        let cdir = dir.path();
        write_claude_default(cdir, "https://x.com", "tok", None).unwrap();
        let mut v: Value =
            serde_json::from_str(&fs::read_to_string(cdir.join("settings.json")).unwrap()).unwrap();
        v["env"]["USER_VAR"] = json!("keep");
        fs::write(
            cdir.join("settings.json"),
            serde_json::to_string(&v).unwrap(),
        )
        .unwrap();

        clear_claude_default(cdir).unwrap();
        let after: Value =
            serde_json::from_str(&fs::read_to_string(cdir.join("settings.json")).unwrap()).unwrap();
        assert!(after["env"].get("ANTHROPIC_BASE_URL").is_none());
        assert_eq!(after["env"]["USER_VAR"], "keep");
    }

    #[test]
    fn creates_file_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        write_claude_default(dir.path(), "https://x.com", "t", None).unwrap();
        assert!(dir.path().join("settings.json").exists());
    }
}
