//! `torizon remote-access` — manage remote-access sessions, SSH keys, and the
//! IP accept-list.

use std::io::Write;

use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use super::Ctx;
use crate::client::encode_segment as enc;
use crate::output;

#[derive(Debug, Subcommand)]
pub enum RemoteAccessCmd {
    /// Show remote-access info for a device.
    Device { device_uuid: String },
    /// Show remote-access sessions for a device.
    Sessions { device_uuid: String },
    /// Create a remote-access session for a device.
    CreateSession {
        device_uuid: String,
        /// SSH public key to authorise (repeatable, at least one required).
        #[arg(long = "public-key", required = true)]
        public_keys: Vec<String>,
        /// Session duration (e.g. "30m", "1h").
        #[arg(long, default_value = "30m")]
        duration: String,
    },
    /// Delete the remote-access session for a device.
    DeleteSession {
        device_uuid: String,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// List all of your remote-access sessions.
    UserSessions,
    /// List your registered SSH public keys.
    Keys,
    /// Register an SSH public key.
    AddKey {
        /// The public key contents.
        #[arg(long)]
        pubkey: String,
    },
    /// Remove an SSH public key by ID.
    RemoveKey { key_id: String },
    /// Show the IP accept-list.
    IpList,
    /// Add an IP to the accept-list.
    AddIp { ip: String },
    /// Remove an IP from the accept-list.
    RemoveIp { ip: String },
}

pub fn run(ctx: &Ctx, cmd: RemoteAccessCmd) -> Result<()> {
    let client = ctx.client()?;
    match cmd {
        RemoteAccessCmd::Device { device_uuid } => {
            output::print_json(
                &client.get(&format!("/remote-access/device/{}", enc(&device_uuid)), &[])?,
            );
            Ok(())
        }
        RemoteAccessCmd::Sessions { device_uuid } => {
            output::print_json(&client.get(
                &format!("/remote-access/device/{}/sessions", enc(&device_uuid)),
                &[],
            )?);
            Ok(())
        }
        RemoteAccessCmd::CreateSession {
            device_uuid,
            public_keys,
            duration,
        } => {
            let body = json!({ "publicKeys": public_keys, "sessionDuration": duration });
            let resp = client.post_json(
                &format!("/remote-access/device/{}/sessions", enc(&device_uuid)),
                &body,
            )?;
            output::report_data(ctx.format, "Session created.", &resp);
            Ok(())
        }
        RemoteAccessCmd::DeleteSession { device_uuid, yes } => {
            if !yes && !confirm(&format!("Delete remote-access session for {device_uuid}?"))? {
                println!("Aborted.");
                return Ok(());
            }
            client.delete(&format!(
                "/remote-access/device/{}/sessions",
                enc(&device_uuid)
            ))?;
            output::report_status(
                ctx.format,
                "Session deleted.",
                &json!({"status":"deleted","deviceUuid": device_uuid}),
            );
            Ok(())
        }
        RemoteAccessCmd::UserSessions => {
            output::print_json(&client.get("/remote-access/user/sessions", &[])?);
            Ok(())
        }
        RemoteAccessCmd::Keys => {
            output::print_json(&client.get("/remote-access/user/public-keys", &[])?);
            Ok(())
        }
        RemoteAccessCmd::AddKey { pubkey } => {
            let resp = client.post_json(
                "/remote-access/user/public-keys",
                &json!({ "pubkey": pubkey }),
            )?;
            output::report_data(ctx.format, "Key registered.", &resp);
            Ok(())
        }
        RemoteAccessCmd::RemoveKey { key_id } => {
            client.delete(&format!("/remote-access/user/public-keys/{}", enc(&key_id)))?;
            output::report_status(
                ctx.format,
                &format!("Key {key_id} removed."),
                &json!({"status":"removed","keyId": key_id}),
            );
            Ok(())
        }
        RemoteAccessCmd::IpList => {
            output::print_json(&client.get("/remote-access/user/ip-accept-list", &[])?);
            Ok(())
        }
        RemoteAccessCmd::AddIp { ip } => {
            let resp =
                client.post_json("/remote-access/user/ip-accept-list", &json!({ "ip": ip }))?;
            output::report_data(ctx.format, &format!("IP {ip} added."), &resp);
            Ok(())
        }
        RemoteAccessCmd::RemoveIp { ip } => {
            client.delete(&format!("/remote-access/user/ip-accept-list/{}", enc(&ip)))?;
            output::report_status(
                ctx.format,
                &format!("IP {ip} removed."),
                &json!({"status":"removed","ip": ip}),
            );
            Ok(())
        }
    }
}

fn confirm(question: &str) -> Result<bool> {
    print!("{question} [y/N] ");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
