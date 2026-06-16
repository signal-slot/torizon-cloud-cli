//! Subcommand implementations.

pub mod devices;
pub mod fleets;
pub mod lockboxes;
pub mod login;
pub mod metrics;
pub mod packages;
pub mod remote_access;
pub mod updates;

use anyhow::Result;

use crate::client::ApiClient;
use crate::config::{Config, Profile};
use crate::output::Format;

/// Shared context passed to every subcommand.
pub struct Ctx {
    pub format: Format,
    /// Profile name from `--profile`, if any.
    pub profile: Option<String>,
}

impl Ctx {
    /// Build an authenticated API client for the selected profile.
    pub fn client(&self) -> Result<ApiClient> {
        ApiClient::new(self.resolve_profile()?)
    }

    /// Resolve credentials. Unless an explicit `--profile` is given, credentials
    /// in the `TORIZON_CLIENT_ID` / `TORIZON_CLIENT_SECRET` environment variables
    /// are used directly (no `torizon login` or credentials file required). This
    /// is the convenient path for CI and automation. Otherwise the saved
    /// credentials file is used.
    fn resolve_profile(&self) -> Result<Profile> {
        if self.profile.is_none() {
            if let Some(profile) = env_profile() {
                return Ok(profile);
            }
        }
        let config = Config::load()?;
        Ok(config.resolve(self.profile.as_deref())?.clone())
    }
}

/// Build a profile from environment variables, if both ID and secret are set.
fn env_profile() -> Option<Profile> {
    let client_id = non_empty("TORIZON_CLIENT_ID")?;
    let client_secret = non_empty("TORIZON_CLIENT_SECRET")?;
    Some(Profile {
        client_id,
        client_secret,
        api_base: non_empty("TORIZON_API_BASE"),
        token_url: None,
    })
}

fn non_empty(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|v| !v.is_empty())
}
