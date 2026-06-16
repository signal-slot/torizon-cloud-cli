//! `torizon devices` — list, inspect, and manage devices.

use std::io::Write;

use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use serde_json::{json, Map, Value};

use super::Ctx;
use crate::client::ApiClient;
use crate::output::{self, Format};

#[derive(Debug, Subcommand)]
pub enum DevicesCmd {
    /// List devices in your repository.
    List(ListArgs),
    /// Show detailed information about a single device.
    Get { device_uuid: String },
    /// Show the currently-assigned update for a device.
    Assignment { device_uuid: String },
    /// Show the software components reported by a device.
    Components { device_uuid: String },
    /// Show the installed packages reported by a device.
    Packages { device_uuid: String },
    /// Show network information (all devices, or one if a UUID is given).
    Network { device_uuid: Option<String> },
    /// Get or set a device's display name.
    Name {
        device_uuid: String,
        /// New display name. Omit to just read the current one.
        #[arg(long)]
        set: Option<String>,
    },
    /// Get or set a device's notes.
    Notes {
        device_uuid: String,
        /// New notes. Omit to just read the current ones.
        #[arg(long)]
        set: Option<String>,
    },
    /// Show a device's tags.
    Tags { device_uuid: String },
    /// Set/update tags on a device (key=value, repeatable).
    SetTags {
        device_uuid: String,
        #[arg(long = "tag", required = true, value_parser = parse_kv)]
        tags: Vec<(String, String)>,
    },
    /// Set the hibernation status of a device.
    Hibernate {
        device_uuid: String,
        /// Hibernate the device.
        #[arg(long, action = clap::ArgAction::SetTrue, conflicts_with = "off")]
        on: bool,
        /// Wake the device.
        #[arg(long, action = clap::ArgAction::SetTrue)]
        off: bool,
    },
    /// Manually create a device.
    Create(CreateArgs),
    /// Delete a device.
    Delete {
        device_uuid: String,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Retrieve a device provisioning token.
    Token,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    limit: Option<u32>,
    #[arg(long)]
    offset: Option<u32>,
    #[arg(long)]
    name_contains: Option<String>,
    #[arg(long = "tag")]
    tags: Vec<String>,
    #[arg(long)]
    hibernated: Option<bool>,
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Device ID (required).
    #[arg(long)]
    device_id: String,
    /// Display name.
    #[arg(long)]
    name: Option<String>,
    /// Fleet ID to join (repeatable).
    #[arg(long = "fleet")]
    fleets: Vec<String>,
    /// Tag key=value (repeatable).
    #[arg(long = "tag", value_parser = parse_kv)]
    tags: Vec<(String, String)>,
}

fn parse_kv(s: &str) -> Result<(String, String), String> {
    match s.split_once('=') {
        Some((k, v)) => Ok((k.to_string(), v.to_string())),
        None => Err(format!("expected key=value, got `{s}`")),
    }
}

pub fn run(ctx: &Ctx, cmd: DevicesCmd) -> Result<()> {
    let client = ctx.client()?;
    match cmd {
        DevicesCmd::List(args) => list(ctx, &client, args),
        DevicesCmd::Get { device_uuid } => {
            output::print_object(
                ctx.format,
                &client.get(&format!("/devices/{device_uuid}"), &[])?,
            );
            Ok(())
        }
        DevicesCmd::Assignment { device_uuid } => {
            output::print_json(
                &client.get(&format!("/devices/uptane/{device_uuid}/assignment"), &[])?,
            );
            Ok(())
        }
        DevicesCmd::Components { device_uuid } => {
            output::print_json(
                &client.get(&format!("/devices/uptane/{device_uuid}/components"), &[])?,
            );
            Ok(())
        }
        DevicesCmd::Packages { device_uuid } => {
            output::print_json(&client.get(&format!("/devices/packages/{device_uuid}"), &[])?);
            Ok(())
        }
        DevicesCmd::Network { device_uuid } => {
            let path = match device_uuid {
                Some(u) => format!("/devices/network/{u}"),
                None => "/devices/network".to_string(),
            };
            output::print_json(&client.get(&path, &[])?);
            Ok(())
        }
        DevicesCmd::Name { device_uuid, set } => match set {
            Some(name) => {
                client.put_json(
                    &format!("/devices/name/{device_uuid}"),
                    &json!({ "name": name }),
                )?;
                output::report_status(
                    ctx.format,
                    &format!("Name set to `{name}`."),
                    &json!({"deviceUuid": device_uuid, "name": name}),
                );
                Ok(())
            }
            None => {
                output::print_json(&client.get(&format!("/devices/name/{device_uuid}"), &[])?);
                Ok(())
            }
        },
        DevicesCmd::Notes { device_uuid, set } => match set {
            Some(notes) => {
                client.put_json(
                    &format!("/devices/notes/{device_uuid}"),
                    &json!({ "notes": notes }),
                )?;
                output::report_status(
                    ctx.format,
                    "Notes updated.",
                    &json!({"deviceUuid": device_uuid, "notes": notes}),
                );
                Ok(())
            }
            None => {
                output::print_json(&client.get(&format!("/devices/notes/{device_uuid}"), &[])?);
                Ok(())
            }
        },
        DevicesCmd::Tags { device_uuid } => {
            output::print_json(&client.get(&format!("/devices/tags/{device_uuid}"), &[])?);
            Ok(())
        }
        DevicesCmd::SetTags { device_uuid, tags } => {
            let mut map = Map::new();
            for (k, v) in tags {
                map.insert(k, json!(v));
            }
            client.patch_json(&format!("/devices/tags/{device_uuid}"), &Value::Object(map))?;
            output::report_status(
                ctx.format,
                "Tags updated.",
                &json!({"status":"updated","deviceUuid": device_uuid}),
            );
            Ok(())
        }
        DevicesCmd::Hibernate {
            device_uuid,
            on,
            off,
        } => {
            if !on && !off {
                bail!("specify --on or --off");
            }
            let status = on; // --on => hibernate(true), --off => false
            client.put_json(
                &format!("/devices/hibernation/{device_uuid}"),
                &json!({ "status": status }),
            )?;
            output::report_status(
                ctx.format,
                &format!("Hibernation set to {status}."),
                &json!({"deviceUuid": device_uuid, "hibernated": status}),
            );
            Ok(())
        }
        DevicesCmd::Create(args) => create(ctx, &client, args),
        DevicesCmd::Delete { device_uuid, yes } => {
            if !yes && !confirm(&format!("Delete device {device_uuid}?"))? {
                println!("Aborted.");
                return Ok(());
            }
            client.delete(&format!("/devices/{device_uuid}"))?;
            output::report_status(
                ctx.format,
                &format!("Device {device_uuid} deleted."),
                &json!({"status":"deleted","deviceUuid": device_uuid}),
            );
            Ok(())
        }
        DevicesCmd::Token => {
            output::print_json(&client.get("/devices/token", &[])?);
            Ok(())
        }
    }
}

fn list(ctx: &Ctx, client: &ApiClient, args: ListArgs) -> Result<()> {
    let mut q: Vec<(&str, String)> = Vec::new();
    if let Some(v) = args.limit {
        q.push(("limit", v.to_string()));
    }
    if let Some(v) = args.offset {
        q.push(("offset", v.to_string()));
    }
    if let Some(v) = args.name_contains {
        q.push(("nameContains", v));
    }
    if let Some(v) = args.hibernated {
        q.push(("hibernated", v.to_string()));
    }
    for tag in &args.tags {
        q.push(("tags", tag.clone()));
    }
    let resp = client.get("/devices", &q)?;
    match ctx.format {
        Format::Json => output::print_json(&resp),
        Format::Human => output::print_table(
            &output::paginated_values(&resp),
            &[
                ("deviceUuid", "UUID"),
                ("deviceName", "NAME"),
                ("deviceId", "DEVICE-ID"),
                ("deviceStatus", "STATUS"),
                ("lastSeen", "LAST-SEEN"),
                ("hibernated", "HIBERNATED"),
            ],
        ),
    }
    Ok(())
}

fn create(ctx: &Ctx, client: &ApiClient, args: CreateArgs) -> Result<()> {
    let mut body = Map::new();
    body.insert("deviceId".into(), json!(args.device_id));
    if let Some(n) = args.name {
        body.insert("deviceName".into(), json!(n));
    }
    if !args.fleets.is_empty() {
        body.insert("fleetIds".into(), json!(args.fleets));
    }
    if !args.tags.is_empty() {
        let mut t = Map::new();
        for (k, v) in args.tags {
            t.insert(k, json!(v));
        }
        body.insert("tags".into(), Value::Object(t));
    }
    let resp = client.post_json("/devices", &Value::Object(body))?;
    output::report_data(ctx.format, "Device created.", &resp);
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::parse_kv;

    #[test]
    fn parse_kv_splits_on_first_equals() {
        assert_eq!(parse_kv("env=prod").unwrap(), ("env".into(), "prod".into()));
        // value may itself contain '='
        assert_eq!(parse_kv("k=a=b").unwrap(), ("k".into(), "a=b".into()));
    }

    #[test]
    fn parse_kv_rejects_missing_equals() {
        assert!(parse_kv("noequals").is_err());
    }
}
