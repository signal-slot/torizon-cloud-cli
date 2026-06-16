//! Credential / profile storage for the Torizon Cloud CLI.
//!
//! Credentials live in `~/.config/torizon/credentials.toml`. The file is only
//! ever created by the user running `torizon login`; commands merely read it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// Default Torizon Platform API base (no trailing slash).
pub const DEFAULT_API_BASE: &str = "https://app.torizon.io/api/v2";
/// Default Keycloak token endpoint for the `ota-users` realm.
pub const DEFAULT_TOKEN_URL: &str =
    "https://kc.torizon.io/auth/realms/ota-users/protocol/openid-connect/token";

/// A single named set of API-client credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub client_id: String,
    pub client_secret: String,
    /// Override the API base URL; falls back to [`DEFAULT_API_BASE`].
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub api_base: Option<String>,
    /// Override the OAuth2 token endpoint; falls back to [`DEFAULT_TOKEN_URL`].
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub token_url: Option<String>,
}

/// On-disk credentials file: an optional default profile name plus a map of
/// profile name -> credentials.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

/// `~/.config/torizon` (honours `XDG_CONFIG_HOME` via the `dirs` crate).
pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir()
        .ok_or_else(|| anyhow!("could not determine the user config directory"))?;
    Ok(base.join("torizon"))
}

/// Path to the credentials file.
pub fn credentials_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("credentials.toml"))
}

impl Config {
    /// Load the credentials file, returning an empty [`Config`] if it does not
    /// exist yet.
    pub fn load() -> Result<Self> {
        let path = credentials_path()?;
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Persist the credentials file, creating `~/.config/torizon` if needed.
    /// On Unix the file is written with mode 0600 to protect the secret.
    pub fn save(&self) -> Result<()> {
        let dir = config_dir()?;
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = credentials_path()?;
        let text = toml::to_string_pretty(self).context("serialising credentials")?;
        std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
        restrict_permissions(&path)?;
        Ok(())
    }

    /// Resolve the profile to use: the explicit `name` if given, otherwise the
    /// configured `default`, otherwise the sole profile if there is exactly one.
    pub fn resolve(&self, name: Option<&str>) -> Result<&Profile> {
        let key = match name.or(self.default.as_deref()) {
            Some(k) => k.to_string(),
            None if self.profiles.len() == 1 => self.profiles.keys().next().unwrap().clone(),
            None => {
                return Err(anyhow!(
                    "no profile specified and no default set; run `torizon login` first"
                ))
            }
        };
        self.profiles.get(&key).ok_or_else(|| {
            anyhow!(
                "profile `{key}` not found in {}",
                credentials_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            )
        })
    }
}

impl Profile {
    pub fn api_base(&self) -> &str {
        self.api_base.as_deref().unwrap_or(DEFAULT_API_BASE)
    }
    pub fn token_url(&self) -> &str {
        self.token_url.as_deref().unwrap_or(DEFAULT_TOKEN_URL)
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("setting permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str) -> Profile {
        Profile {
            client_id: id.to_string(),
            client_secret: "secret".to_string(),
            api_base: None,
            token_url: None,
        }
    }

    #[test]
    fn defaults_fall_back_to_constants() {
        let p = profile("a");
        assert_eq!(p.api_base(), DEFAULT_API_BASE);
        assert_eq!(p.token_url(), DEFAULT_TOKEN_URL);
    }

    #[test]
    fn overrides_take_precedence() {
        let p = Profile {
            api_base: Some("https://example.test/api".to_string()),
            ..profile("a")
        };
        assert_eq!(p.api_base(), "https://example.test/api");
    }

    #[test]
    fn resolve_uses_explicit_name() {
        let mut cfg = Config::default();
        cfg.profiles.insert("staging".into(), profile("s"));
        cfg.profiles.insert("prod".into(), profile("p"));
        assert_eq!(cfg.resolve(Some("prod")).unwrap().client_id, "p");
    }

    #[test]
    fn resolve_uses_default_then_sole_profile() {
        let mut cfg = Config::default();
        cfg.profiles.insert("only".into(), profile("o"));
        // sole profile is used when no name and no default
        assert_eq!(cfg.resolve(None).unwrap().client_id, "o");
        // explicit default wins even with several profiles
        cfg.profiles.insert("other".into(), profile("x"));
        cfg.default = Some("other".into());
        assert_eq!(cfg.resolve(None).unwrap().client_id, "x");
    }

    #[test]
    fn resolve_errors_when_ambiguous() {
        let mut cfg = Config::default();
        cfg.profiles.insert("a".into(), profile("a"));
        cfg.profiles.insert("b".into(), profile("b"));
        assert!(cfg.resolve(None).is_err());
        assert!(cfg.resolve(Some("missing")).is_err());
    }

    #[test]
    fn toml_round_trips() {
        let mut cfg = Config {
            default: Some("default".into()),
            ..Default::default()
        };
        cfg.profiles.insert("default".into(), profile("cid"));
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.default.as_deref(), Some("default"));
        assert_eq!(back.profiles["default"].client_id, "cid");
        // optional fields stay absent
        assert!(back.profiles["default"].api_base.is_none());
    }
}
