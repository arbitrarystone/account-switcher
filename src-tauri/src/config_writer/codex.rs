use std::fs;
use std::path::Path;

use toml::{Table, Value};

use super::{atomic_write, ConfigError, Result};

fn read_toml(path: &Path) -> Result<Table> {
    if !path.exists() {
        return Ok(Table::new());
    }
    let s = fs::read_to_string(path).map_err(|e| ConfigError::Read(e.to_string()))?;
    if s.trim().is_empty() {
        return Ok(Table::new());
    }
    s.parse::<Table>()
        .map_err(|e| ConfigError::Parse(e.to_string()))
}

fn write_toml(path: &Path, doc: &Table) -> Result<()> {
    let s = toml::to_string_pretty(doc).map_err(|e| ConfigError::Write(e.to_string()))?;
    atomic_write(path, &s)
}

/// 写 `<codex_dir>/config.toml` 的 model_provider + [model_providers.<id>]（合并保留其他）。
///
/// 注意：Codex 的 token 走 `env_key` 指向的环境变量，不落入 config.toml。
/// 因此 app 外的终端使用该全局默认时，需自行设置对应环境变量。
pub fn write_codex_default(
    codex_dir: &Path,
    provider_id: &str,
    base_url: &str,
    env_key: &str,
    model: Option<&str>,
) -> Result<()> {
    let path = codex_dir.join("config.toml");
    let mut doc = read_toml(&path)?;
    doc.insert(
        "model_provider".to_string(),
        Value::String(provider_id.to_string()),
    );
    if let Some(m) = model {
        doc.insert("model".to_string(), Value::String(m.to_string()));
    }
    if !doc
        .get("model_providers")
        .map(|v| v.is_table())
        .unwrap_or(false)
    {
        doc.insert("model_providers".to_string(), Value::Table(Table::new()));
    }
    let providers = doc
        .get_mut("model_providers")
        .unwrap()
        .as_table_mut()
        .unwrap();
    let mut entry = Table::new();
    entry.insert("name".to_string(), Value::String(provider_id.to_string()));
    entry.insert("base_url".to_string(), Value::String(base_url.to_string()));
    entry.insert("env_key".to_string(), Value::String(env_key.to_string()));
    providers.insert(provider_id.to_string(), Value::Table(entry));
    write_toml(&path, &doc)
}

/// 清除 Codex 全局默认（移除 model_provider 指针与对应 provider 条目）。
pub fn clear_codex_default(codex_dir: &Path, provider_id: &str) -> Result<()> {
    let path = codex_dir.join("config.toml");
    if !path.exists() {
        return Ok(());
    }
    let mut doc = read_toml(&path)?;
    doc.remove("model_provider");
    if let Some(providers) = doc
        .get_mut("model_providers")
        .and_then(|v| v.as_table_mut())
    {
        providers.remove(provider_id);
    }
    write_toml(&path, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_provider_preserving_others() {
        let dir = tempfile::tempdir().unwrap();
        let cdir = dir.path();
        fs::write(
            cdir.join("config.toml"),
            "approval_policy = \"on-request\"\n",
        )
        .unwrap();
        write_codex_default(
            cdir,
            "accsw",
            "https://relay.example.com/v1",
            "ACCSW_CODEX_TOKEN",
            Some("gpt-5"),
        )
        .unwrap();

        let parsed: Table = fs::read_to_string(cdir.join("config.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(parsed["approval_policy"].as_str(), Some("on-request"));
        assert_eq!(parsed["model_provider"].as_str(), Some("accsw"));
        assert_eq!(parsed["model"].as_str(), Some("gpt-5"));
        let p = &parsed["model_providers"]["accsw"];
        assert_eq!(p["base_url"].as_str(), Some("https://relay.example.com/v1"));
        assert_eq!(p["env_key"].as_str(), Some("ACCSW_CODEX_TOKEN"));
    }

    #[test]
    fn clear_removes_provider_and_pointer() {
        let dir = tempfile::tempdir().unwrap();
        let cdir = dir.path();
        write_codex_default(cdir, "accsw", "https://x/v1", "K", None).unwrap();
        clear_codex_default(cdir, "accsw").unwrap();

        let parsed: Table = fs::read_to_string(cdir.join("config.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(parsed.get("model_provider").is_none());
        let still_has = parsed
            .get("model_providers")
            .and_then(|v| v.as_table())
            .map(|t| t.contains_key("accsw"))
            .unwrap_or(false);
        assert!(!still_has);
    }
}
