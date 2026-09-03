//! AI 实体配置敏感字段（`api_key`）的加解密器，与 Python sidecar 的
//! `ConfigEncryptor`（services/ai-sidecar/.../ai_config_crypto.py）逐字节兼容。
//!
//! 兼容契约：
//!
//! * 密钥来自环境变量 `AI_CONFIG_ENCRYPTION_KEY`。若其为合法 Fernet key
//!   （urlsafe base64 解码后恰好 32 字节）则直接使用；否则对其 UTF-8 字节做
//!   SHA-256 派生后作为 32 字节 key 材料（兼容 Python 的历史回退行为）。
//! * 密文为标准 Fernet v1 token：`base64url(0x80 || ts_be_u64 || iv[16] ||
//!   AES-128-CBC/PKCS7(plaintext) || HMAC-SHA256(signing_key, 前述字节))`，
//!   其中 signing key 为 32 字节 key 的前 16 字节，加密 key 为后 16 字节。
//! * 加密递归作用于配置文档中所有非空字符串 `api_key` 字段，并在顶层写入
//!   `_key_encrypted=true` / `_key_encryption="fernet_v1"` 标记；解密后移除
//!   `_key_encrypted` / `_key_encoded` / `_key_encryption` 三个内部标记。
//! * fail-closed：配置含非空 `api_key` 而未配置密钥时，仅当显式设置
//!   `AI_CONFIG_ALLOW_INSECURE_DEV_BASE64=true` 才退回 base64 编码
//!   （`_key_encoded=true`，仅限本地开发），否则报错，避免明文落库。
//! * 解密时若遇加密配置但密钥不可用/解密失败，镜像 Python 行为：记录警告并
//!   将该 `api_key` 置为空串，绝不向上层返回密文。

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::engine::general_purpose::{STANDARD as B64_STANDARD, URL_SAFE as B64_URLSAFE};
use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use fms_domain::error::DomainError;

const ENV_ENCRYPTION_KEY: &str = "AI_CONFIG_ENCRYPTION_KEY";
const ENV_ALLOW_INSECURE_DEV_BASE64: &str = "AI_CONFIG_ALLOW_INSECURE_DEV_BASE64";

const FERNET_VERSION: u8 = 0x80;
const FERNET_OVERHEAD: usize = 1 + 8 + 16 + 32; // version + timestamp + iv + hmac
const MARKER_ENCRYPTED: &str = "_key_encrypted";
const MARKER_ENCODED: &str = "_key_encoded";
const MARKER_ENCRYPTION: &str = "_key_encryption";
const ENCRYPTION_SCHEME: &str = "fernet_v1";

type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// AI 配置 api_key 加解密器。`key` 为 32 字节 Fernet key 材料；
/// `None` 表示未配置密钥（加密 fail-closed，解密按标记尽力而为）。
#[derive(Debug, Clone)]
pub struct AiConfigCrypto {
    key: Option<[u8; 32]>,
    allow_insecure_dev_base64: bool,
}

impl AiConfigCrypto {
    /// 从进程环境构建（`AI_CONFIG_ENCRYPTION_KEY` / `AI_CONFIG_ALLOW_INSECURE_DEV_BASE64`）。
    pub fn from_env() -> Self {
        let raw = std::env::var(ENV_ENCRYPTION_KEY).unwrap_or_default();
        let allow_insecure = std::env::var(ENV_ALLOW_INSECURE_DEV_BASE64)
            .map(|value| is_truthy(&value))
            .unwrap_or(false);
        let crypto = Self::new(&raw, allow_insecure);
        if crypto.key.is_some() {
            tracing::info!("AiConfigCrypto: API key encryption enabled (fernet)");
        } else if allow_insecure {
            tracing::warn!(
                "AiConfigCrypto: encryption key missing; insecure base64 fallback active because \
                 AI_CONFIG_ALLOW_INSECURE_DEV_BASE64=true. Never use this in production."
            );
        } else {
            tracing::warn!(
                "AiConfigCrypto: {ENV_ENCRYPTION_KEY} not configured; saving a config with an \
                 api_key will fail closed until the key is set."
            );
        }
        crypto
    }

    /// 以显式密钥字符串构建（测试注入用）。语义与 Python `_init_fernet` 一致。
    pub fn new(raw_key: &str, allow_insecure_dev_base64: bool) -> Self {
        Self {
            key: derive_key(raw_key.trim()),
            allow_insecure_dev_base64,
        }
    }

    pub fn encryption_enabled(&self) -> bool {
        self.key.is_some()
    }

    /// 加密文档中所有非空 `api_key`，并写入存储标记（顶层）。
    pub fn encrypt_config(&self, config: &mut serde_json::Value) -> Result<(), DomainError> {
        if !has_api_key(config) {
            return Ok(());
        }
        if self.key.is_some() {
            self.transform_api_keys(config, |value| self.fernet_encrypt(value))?;
            set_marker(config, MARKER_ENCRYPTED, true);
            set_marker(config, MARKER_ENCRYPTION, ENCRYPTION_SCHEME);
            remove_marker(config, MARKER_ENCODED);
            return Ok(());
        }
        if self.allow_insecure_dev_base64 {
            self.transform_api_keys(config, |value| Ok(B64_STANDARD.encode(value.as_bytes())))?;
            set_marker(config, MARKER_ENCODED, true);
            remove_marker(config, MARKER_ENCRYPTED);
            remove_marker(config, MARKER_ENCRYPTION);
            return Ok(());
        }
        Err(DomainError::Internal(format!(
            "Encrypted AI config is required but fernet is unavailable. Set {ENV_ENCRYPTION_KEY} \
             (or {ENV_ALLOW_INSECURE_DEV_BASE64}=true for local development only)."
        )))
    }

    /// 解密文档中的 `api_key` 并移除内部存储标记（顶层）。
    ///
    /// 解密失败时镜像 Python 行为：记录警告并把该字段置为空串。
    pub fn decrypt_config(&self, config: &mut serde_json::Value) {
        let encrypted = marker_flag(config, MARKER_ENCRYPTED);
        let encoded = marker_flag(config, MARKER_ENCODED);
        if encrypted {
            // 加密标记下解密失败不应中断读取；失败的字段置空串（Python 同款语义）。
            let _ = self.transform_api_keys(config, |value| Ok(self.fernet_decrypt_lenient(value)));
        } else if encoded {
            let _ = self.transform_api_keys(config, |value| {
                Ok(B64_STANDARD
                    .decode(value.as_bytes())
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .unwrap_or_else(|| {
                        tracing::warn!("base64_api_key_decode_failed");
                        String::new()
                    }))
            });
        }
        if let Some(object) = config.as_object_mut() {
            object.remove(MARKER_ENCRYPTED);
            object.remove(MARKER_ENCODED);
            object.remove(MARKER_ENCRYPTION);
        }
    }

    /// Fernet v1 加密，输出 urlsafe base64 token（与 Python `Fernet.encrypt` 相同格式）。
    fn fernet_encrypt(&self, plaintext: &str) -> Result<String, DomainError> {
        let key = self
            .key
            .ok_or_else(|| DomainError::Internal("fernet key unavailable".to_string()))?;
        let (signing_key, encryption_key) = key.split_at(16);

        let mut iv = [0u8; 16];
        rand::Rng::fill(&mut rand::thread_rng(), &mut iv);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| DomainError::Internal(error.to_string()))?
            .as_secs();

        let ciphertext =
            Aes128CbcEnc::new(encryption_key.into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(plaintext.as_bytes());

        let mut body = Vec::with_capacity(1 + 8 + 16 + ciphertext.len());
        body.push(FERNET_VERSION);
        body.extend_from_slice(&timestamp.to_be_bytes());
        body.extend_from_slice(&iv);
        body.extend_from_slice(&ciphertext);

        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(signing_key)
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        mac.update(&body);
        let tag = mac.finalize().into_bytes();

        let mut token = body;
        token.extend_from_slice(&tag);
        Ok(B64_URLSAFE.encode(token))
    }

    /// Fernet v1 解密。任何一步失败（格式、HMAC、padding、UTF-8）都返回 `None`。
    fn fernet_decrypt(&self, token: &str) -> Option<String> {
        let key = self.key?;
        let (signing_key, encryption_key) = key.split_at(16);

        let data = B64_URLSAFE.decode(token.as_bytes()).ok()?;
        if data.len() < FERNET_OVERHEAD + 16 || data[0] != FERNET_VERSION {
            return None;
        }
        let (body, tag) = data.split_at(data.len() - 32);

        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(signing_key).ok()?;
        mac.update(body);
        mac.verify_slice(tag).ok()?;

        let iv: &[u8; 16] = body.get(9..25)?.try_into().ok()?;
        let ciphertext = body.get(25..)?;
        if ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
            return None;
        }
        let plaintext = Aes128CbcDec::new(encryption_key.into(), iv.into())
            .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
            .ok()?;
        String::from_utf8(plaintext).ok()
    }

    /// 解密失败时置空串并告警（镜像 Python `_decrypt_value`）。
    fn fernet_decrypt_lenient(&self, token: &str) -> String {
        if self.key.is_none() {
            tracing::error!("Cannot decrypt AI API key: config is encrypted but no fernet key is configured");
            return String::new();
        }
        self.fernet_decrypt(token).unwrap_or_else(|| {
            tracing::warn!("fernet_api_key_decrypt_failed");
            String::new()
        })
    }

    /// 递归对所有非空字符串 `api_key` 字段应用 `transform`（Python `_transform_api_keys` 同款遍历）。
    fn transform_api_keys(
        &self,
        value: &mut serde_json::Value,
        transform: impl Fn(&str) -> Result<String, DomainError> + Copy,
    ) -> Result<(), DomainError> {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::String(api_key)) = map.get("api_key") {
                    if !api_key.is_empty() {
                        let encrypted = transform(api_key)?;
                        map.insert("api_key".to_string(), serde_json::Value::String(encrypted));
                    }
                }
                for (key, child) in map.iter_mut() {
                    if key == "api_key" {
                        continue;
                    }
                    self.transform_api_keys(child, transform)?;
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    self.transform_api_keys(item, transform)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// 与 Python `ConfigEncryptor._init_fernet` 一致的密钥解析：
/// 合法 Fernet key（urlsafe base64 → 32 字节）直接用；否则 SHA-256 派生。
fn derive_key(raw_key: &str) -> Option<[u8; 32]> {
    if raw_key.is_empty() {
        return None;
    }
    if let Ok(decoded) = B64_URLSAFE.decode(raw_key.as_bytes()) {
        if decoded.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            return Some(key);
        }
    }
    let digest = Sha256::digest(raw_key.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    Some(key)
}

fn has_api_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            if matches!(map.get("api_key"), Some(serde_json::Value::String(key)) if !key.is_empty()) {
                return true;
            }
            map.values().any(has_api_key)
        }
        serde_json::Value::Array(items) => items.iter().any(has_api_key),
        _ => false,
    }
}

fn marker_flag(config: &serde_json::Value, name: &str) -> bool {
    config.get(name).and_then(serde_json::Value::as_bool).unwrap_or(false)
}

fn set_marker(config: &mut serde_json::Value, name: &str, value: impl Into<serde_json::Value>) {
    if let Some(object) = config.as_object_mut() {
        object.insert(name.to_string(), value.into());
    }
}

fn remove_marker(config: &mut serde_json::Value, name: &str) {
    if let Some(object) = config.as_object_mut() {
        object.remove(name);
    }
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on" | "enabled"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Python cryptography 48.0.0 生成的真实 Fernet 密文向量（跨语言兼容 fixture）：
    //   key1 = Fernet.generate_key(); Fernet(key1).encrypt(b"sk-live-test-key-12345")
    const PY_DIRECT_KEY: &str = "RTjoixuwMBJzdQgIM5ZMFSajhfzt_bnd_Yhv_aogHkI=";
    const PY_DIRECT_TOKEN: &str = "gAAAAABqmapMObte7dfTN8fD9cxBu2Z2NbQzTs9_ddRuxvXACpYQUuXWh-3pUZEN4EJ19eo4sfJ824veiQ-5KhTfV5GAFEhCo_4TxwlnASgV-Efx5D-vWT0=";
    //   raw = "test-passphrase-for-ai-config"
    //   derived = urlsafe_b64encode(sha256(raw).digest()); Fernet(derived).encrypt("sk-派生密钥-测试")
    const PY_RAW_PASSPHRASE: &str = "test-passphrase-for-ai-config";
    const PY_DERIVED_TOKEN: &str = "gAAAAABqmapMYgNwGT3bX-6CrXRwPbHyfZSZHFY3rnBUteReFGxKFnLmOjoB16-B8BWGzEjc9uga9Lnw4v4qDdlGKRe7zn1GGtcCDRC1NbXbK5hlC359AWA=";

    #[test]
    fn decrypts_python_generated_token_with_direct_key() {
        let crypto = AiConfigCrypto::new(PY_DIRECT_KEY, false);
        assert_eq!(
            crypto.fernet_decrypt(PY_DIRECT_TOKEN).as_deref(),
            Some("sk-live-test-key-12345")
        );
    }

    #[test]
    fn decrypts_python_generated_token_with_sha256_derived_key() {
        let crypto = AiConfigCrypto::new(PY_RAW_PASSPHRASE, false);
        assert_eq!(
            crypto.fernet_decrypt(PY_DERIVED_TOKEN).as_deref(),
            Some("sk-派生密钥-测试")
        );
    }

    #[test]
    fn encrypt_then_decrypt_round_trip() {
        let crypto = AiConfigCrypto::new(PY_DIRECT_KEY, false);
        let token = crypto.fernet_encrypt("sk-round-trip-密钥").expect("encrypt");
        assert!(token.starts_with("gAAAAA")); // fernet v1 token 前缀
        assert_eq!(crypto.fernet_decrypt(&token).as_deref(), Some("sk-round-trip-密钥"));
    }

    #[test]
    fn decrypt_rejects_tampered_token() {
        let crypto = AiConfigCrypto::new(PY_DIRECT_KEY, false);
        let token = crypto.fernet_encrypt("sk-secret").expect("encrypt");
        let tampered = format!("{}x", &token[..token.len() - 1]);
        assert!(crypto.fernet_decrypt(&tampered).is_none());
        assert!(crypto.fernet_decrypt("not-a-fernet-token").is_none());
    }

    #[test]
    fn encrypt_config_marks_and_encrypts_nested_api_keys() {
        let crypto = AiConfigCrypto::new(PY_DIRECT_KEY, false);
        let mut config = serde_json::json!({
            "providers": {
                "default": {"api_key": "sk-default", "base_url": "https://x"},
                "asr": {"api_key": "sk-asr"}
            },
            "api_key": "",
            "tools": [{"config": {"api_key": "sk-in-array"}}]
        });
        crypto.encrypt_config(&mut config).expect("encrypt");

        assert_eq!(
            config.get("_key_encrypted").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            config.get("_key_encryption").and_then(serde_json::Value::as_str),
            Some("fernet_v1")
        );
        assert!(config.get("_key_encoded").is_none());
        let default_key = config
            .pointer("/providers/default/api_key")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        assert!(default_key.starts_with("gAAAAA"));
        // 空 api_key 保持不变（Python 只变换非空字符串）
        assert_eq!(config.get("api_key").and_then(serde_json::Value::as_str), Some(""));

        crypto.decrypt_config(&mut config);
        assert_eq!(
            config
                .pointer("/providers/default/api_key")
                .and_then(serde_json::Value::as_str),
            Some("sk-default")
        );
        assert_eq!(
            config
                .pointer("/providers/asr/api_key")
                .and_then(serde_json::Value::as_str),
            Some("sk-asr")
        );
        assert_eq!(
            config
                .pointer("/tools/0/config/api_key")
                .and_then(serde_json::Value::as_str),
            Some("sk-in-array")
        );
        assert!(config.get("_key_encrypted").is_none());
        assert!(config.get("_key_encryption").is_none());
        assert!(config.get("_key_encoded").is_none());
    }

    #[test]
    fn decrypt_config_handles_python_written_document() {
        let crypto = AiConfigCrypto::new(PY_DIRECT_KEY, false);
        let mut config = serde_json::json!({
            "providers": {"default": {"api_key": PY_DIRECT_TOKEN, "base_url": "https://x"}},
            "_key_encrypted": true,
            "_key_encryption": "fernet_v1"
        });
        crypto.decrypt_config(&mut config);
        assert_eq!(
            config
                .pointer("/providers/default/api_key")
                .and_then(serde_json::Value::as_str),
            Some("sk-live-test-key-12345")
        );
        assert!(config.get("_key_encrypted").is_none());
        assert!(config.get("_key_encryption").is_none());
    }

    #[test]
    fn decrypt_config_passes_through_plaintext_without_markers() {
        let crypto = AiConfigCrypto::new(PY_DIRECT_KEY, false);
        let mut config = serde_json::json!({"providers": {"default": {"api_key": "sk-plain"}}});
        crypto.decrypt_config(&mut config);
        assert_eq!(
            config
                .pointer("/providers/default/api_key")
                .and_then(serde_json::Value::as_str),
            Some("sk-plain")
        );
    }

    #[test]
    fn decrypt_config_supports_legacy_base64_marker() {
        let crypto = AiConfigCrypto::new(PY_DIRECT_KEY, false);
        let mut config = serde_json::json!({
            "api_key": B64_STANDARD.encode("sk-legacy"),
            "_key_encoded": true
        });
        crypto.decrypt_config(&mut config);
        assert_eq!(
            config.get("api_key").and_then(serde_json::Value::as_str),
            Some("sk-legacy")
        );
        assert!(config.get("_key_encoded").is_none());
    }

    #[test]
    fn decrypt_config_blanks_key_when_fernet_unavailable() {
        let crypto = AiConfigCrypto::new("", false);
        let mut config = serde_json::json!({
            "api_key": PY_DIRECT_TOKEN,
            "_key_encrypted": true
        });
        crypto.decrypt_config(&mut config);
        assert_eq!(config.get("api_key").and_then(serde_json::Value::as_str), Some(""));
        assert!(config.get("_key_encrypted").is_none());
    }

    #[test]
    fn encrypt_config_without_key_fails_closed() {
        let crypto = AiConfigCrypto::new("", false);
        let mut config = serde_json::json!({"api_key": "sk-secret"});
        assert!(crypto.encrypt_config(&mut config).is_err());
        // 无 api_key 的文档即使未配置密钥也原样放行（种子即此场景）
        let mut seed = serde_json::json!({"api_key": ""});
        crypto.encrypt_config(&mut seed).expect("no api_key -> no-op");
        assert!(seed.get("_key_encrypted").is_none());
    }

    #[test]
    fn encrypt_config_without_key_uses_base64_only_with_insecure_opt_in() {
        let crypto = AiConfigCrypto::new("", true);
        let mut config = serde_json::json!({"api_key": "sk-dev"});
        crypto.encrypt_config(&mut config).expect("base64 fallback");
        assert_eq!(
            config.get("_key_encoded").and_then(serde_json::Value::as_bool),
            Some(true)
        );
        crypto.decrypt_config(&mut config);
        assert_eq!(
            config.get("api_key").and_then(serde_json::Value::as_str),
            Some("sk-dev")
        );
    }
}
