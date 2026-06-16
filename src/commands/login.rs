//! `torizon login` — store and verify API-client credentials.
//!
//! The client secret is never accepted as a plain command-line value (that
//! would leak it into shell history and the process list). It is read, in
//! order, from stdin (`--client-secret-stdin`), the `TORIZON_CLIENT_SECRET`
//! environment variable, or an interactive no-echo prompt.

use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use clap::Args;

use crate::auth;
use crate::config::{Config, Profile};

const SECRET_ENV: &str = "TORIZON_CLIENT_SECRET";

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// API client ID. Prompted for interactively if omitted.
    #[arg(long, env = "TORIZON_CLIENT_ID")]
    pub client_id: Option<String>,
    /// Read the client secret from stdin (one line) instead of prompting.
    #[arg(long)]
    pub client_secret_stdin: bool,
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
        None => prompt_line("Client ID: ")?,
    };
    let client_secret = read_secret(args.client_secret_stdin)?;
    if client_secret.is_empty() {
        anyhow::bail!("client secret must not be empty");
    }

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

/// Resolve the client secret without ever taking it from argv.
fn read_secret(from_stdin: bool) -> Result<String> {
    if from_stdin {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .context("reading client secret from stdin")?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_string());
    }
    if let Ok(v) = std::env::var(SECRET_ENV) {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    // Interactive no-echo prompt (reads from the terminal, not stdin).
    rpassword::prompt_password("Client secret: ").context("reading client secret")
}

fn prompt_line(label: &str) -> Result<String> {
    print!("{label}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("reading from stdin")?;
    Ok(line.trim().to_string())
}
