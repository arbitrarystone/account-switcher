//! 系统钥匙串封装。
//!
//! 通过 `KeychainStore` trait 抽象，生产用 [`SystemKeychain`]（keyring crate，
//! 落到 macOS Keychain / Windows Credential Manager），测试用 [`InMemoryKeychain`]
//! 以避免触碰真实系统钥匙串。

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Mutex;

/// 钥匙串服务标识（所有条目共用，username 用各账号的 `token_ref`）。
const SERVICE: &str = "com.shixusheng.account-switcher";

#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    #[error("钥匙串后端错误: {0}")]
    Backend(String),
    #[error("未找到凭据: {0}")]
    NotFound(String),
}

/// Token 存取抽象。
pub trait KeychainStore: Send + Sync {
    fn set_token(&self, key: &str, token: &str) -> Result<(), KeychainError>;
    fn get_token(&self, key: &str) -> Result<String, KeychainError>;
    /// 删除条目；不存在视为成功（幂等）。
    fn delete_token(&self, key: &str) -> Result<(), KeychainError>;
}

/// 生产实现：系统钥匙串。
pub struct SystemKeychain;

impl KeychainStore for SystemKeychain {
    fn set_token(&self, key: &str, token: &str) -> Result<(), KeychainError> {
        let entry =
            keyring::Entry::new(SERVICE, key).map_err(|e| KeychainError::Backend(e.to_string()))?;
        entry
            .set_password(token)
            .map_err(|e| KeychainError::Backend(e.to_string()))
    }

    fn get_token(&self, key: &str) -> Result<String, KeychainError> {
        let entry =
            keyring::Entry::new(SERVICE, key).map_err(|e| KeychainError::Backend(e.to_string()))?;
        match entry.get_password() {
            Ok(p) => Ok(p),
            Err(keyring::Error::NoEntry) => Err(KeychainError::NotFound(key.to_string())),
            Err(e) => Err(KeychainError::Backend(e.to_string())),
        }
    }

    fn delete_token(&self, key: &str) -> Result<(), KeychainError> {
        let entry =
            keyring::Entry::new(SERVICE, key).map_err(|e| KeychainError::Backend(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(KeychainError::Backend(e.to_string())),
        }
    }
}

/// 测试实现：进程内存（不触碰系统钥匙串）。
#[cfg(test)]
#[derive(Default)]
pub struct InMemoryKeychain {
    map: Mutex<HashMap<String, String>>,
}

#[cfg(test)]
impl KeychainStore for InMemoryKeychain {
    fn set_token(&self, key: &str, token: &str) -> Result<(), KeychainError> {
        self.map
            .lock()
            .unwrap()
            .insert(key.to_string(), token.to_string());
        Ok(())
    }

    fn get_token(&self, key: &str) -> Result<String, KeychainError> {
        self.map
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| KeychainError::NotFound(key.to_string()))
    }

    fn delete_token(&self, key: &str) -> Result<(), KeychainError> {
        self.map.lock().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_set_get_delete_roundtrip() {
        let kc = InMemoryKeychain::default();
        kc.set_token("k1", "secret").unwrap();
        assert_eq!(kc.get_token("k1").unwrap(), "secret");
        kc.delete_token("k1").unwrap();
        assert!(matches!(
            kc.get_token("k1"),
            Err(KeychainError::NotFound(_))
        ));
    }

    #[test]
    fn delete_is_idempotent() {
        let kc = InMemoryKeychain::default();
        assert!(kc.delete_token("missing").is_ok());
    }
}
