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
use crate::config::Config;
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
        let config = Config::load()?;
        let profile = config.resolve(self.profile.as_deref())?.clone();
        ApiClient::new(profile)
    }
}
