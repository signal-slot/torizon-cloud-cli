//! `torizon login` — store API-client credentials and verify them.

use std::io::Write;

use anyhow::{Context, Result};
use clap::Args;

use crate::auth;
use crate::config::{Config, Profile};

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// API client ID. Prompted for interactively if omitted.
    #[arg(long)]
    pub client_id: Option<String>,
    /// API client secret. Prompted for interactively if omitted.
    #[arg(long)]
    pub client_secret: Option<String>,
    /// Name to store these credentials under.
    #[arg(long, default_value = "default")]
    pub profile: String,
    /// Override the API base URL for this profile.
    #[arg(long)]
    pub api_base: Option<String>,
    /// Do not mark this profile as the default.
    #[arg(long)]
    pub no_default: bool,
}

pub fn run(args: LoginArgs) -> Result<()> {
    let client_id = match args.client_id {
        Some(v) => v,
        None => prompt("Client ID: ")?,
    };
    let client_secret = match args.client_secret {
        Some(v) => v,
        None => prompt("Client secret: ")?,
    };

    let profile = Profile {
        client_id,
        client_secret,
        api_base: args.api_base,
        token_url: None,
    };

    // Verify the credentials before persisting them.
    let http = reqwest::blocking::Client::builder()
        .user_agent(concat!("torizon-cloud-cli/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("building HTTP client")?;
    auth::request_token(&http, &profile).context("verifying credentials")?;

    let mut config = Config::load()?;
    config.profiles.insert(args.profile.clone(), profile);
    if !args.no_default || config.default.is_none() {
        config.default = Some(args.profile.clone());
    }
    config.save()?;

    println!(
        "Credentials verified and saved to profile `{}` in {}",
        args.profile,
        crate::config::credentials_path()?.display()
    );
    Ok(())
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("reading from stdin")?;
    Ok(line.trim().to_string())
}
