//! API client configuration.

use crate::error::CoreError;

/// Runtime configuration for the API client.
#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub base_url: String,
    pub allow_cleartext: bool,
}

impl ApiConfig {
    /// Create a new config. When `allow_cleartext` is false (release mode),
    /// a non-HTTPS base URL is rejected.
    pub fn new(base_url: impl Into<String>, allow_cleartext: bool) -> Result<Self, CoreError> {
        let base_url = base_url.into();
        let base_url = base_url.trim_end_matches('/').to_string();
        if !allow_cleartext && !base_url.starts_with("https://") {
            return Err(CoreError::InvalidConfig(format!(
                "base_url must be https:// when cleartext is not allowed, got: {base_url}"
            )));
        }
        if allow_cleartext
            && !base_url.starts_with("https://")
            && !base_url.starts_with("http://")
        {
            return Err(CoreError::InvalidConfig(format!(
                "base_url must start with http:// or https://, got: {base_url}"
            )));
        }
        Ok(Self {
            base_url,
            allow_cleartext,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_cleartext_when_not_allowed() {
        let err = ApiConfig::new("http://10.0.2.2:5000", false).unwrap_err();
        assert!(matches!(err, CoreError::InvalidConfig(_)));
    }

    #[test]
    fn accepts_https_when_not_allowed() {
        let cfg = ApiConfig::new("https://api.example.com/", false).unwrap();
        assert_eq!(cfg.base_url, "https://api.example.com");
    }

    #[test]
    fn accepts_cleartext_when_allowed() {
        let cfg = ApiConfig::new("http://10.0.2.2:5000", true).unwrap();
        assert_eq!(cfg.base_url, "http://10.0.2.2:5000");
    }

    #[test]
    fn rejects_garbage_scheme() {
        assert!(ApiConfig::new("ftp://x", true).is_err());
    }
}
