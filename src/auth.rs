//! OAuth2 `client_credentials` token acquisition with a small on-disk cache.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{config_dir, Profile};

/// Refresh a token this many seconds before it actually expires.
const EXPIRY_MARGIN_SECS: u64 = 30;

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedToken {
    access_token: String,
    /// UNIX epoch seconds at which the token must no longer be used.
    expires_at: u64,
}

type Cache = BTreeMap<String, CachedToken>;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("token-cache.json"))
}

fn load_cache() -> Cache {
    cache_path()
        .ok()
        .filter(|p| p.exists())
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn store_cache(cache: &Cache) -> Result<()> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = cache_path()?;
    let text = serde_json::to_string_pretty(cache)?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    restrict_permissions(&path)?;
    Ok(())
}

/// Return a valid access token for `profile`, using the cache when possible and
/// otherwise requesting a fresh one from the token endpoint.
pub fn access_token(client: &reqwest::blocking::Client, profile: &Profile) -> Result<String> {
    let mut cache = load_cache();
    if let Some(tok) = cache.get(&profile.client_id) {
        if tok.expires_at > now_secs() + EXPIRY_MARGIN_SECS {
            return Ok(tok.access_token.clone());
        }
    }

    let (token, expires_in) = request_token(client, profile)?;
    cache.insert(
        profile.client_id.clone(),
        CachedToken {
            access_token: token.clone(),
            expires_at: now_secs() + expires_in,
        },
    );
    // A cache write failure must not break the command; the token is still valid.
    let _ = store_cache(&cache);
    Ok(token)
}

/// Perform the `client_credentials` grant. Returns the access token together
/// with its lifetime in seconds. Used both by [`access_token`] and by
/// `torizon login` to verify credentials.
pub fn request_token(
    client: &reqwest::blocking::Client,
    profile: &Profile,
) -> Result<(String, u64)> {
    let resp = client
        .post(profile.token_url())
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", profile.client_id.as_str()),
            ("client_secret", profile.client_secret.as_str()),
        ])
        .send()
        .context("requesting OAuth2 token")?;

    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!(
            "token request failed (HTTP {}): {}",
            status.as_u16(),
            body.trim()
        ));
    }
    let parsed: TokenResponse = serde_json::from_str(&body).context("parsing token response")?;
    Ok((parsed.access_token, parsed.expires_in))
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}
