//! `torizon fleets` — manage fleets and their device membership.

use std::io::Write;

use anyhow::{bail, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;

use super::Ctx;
use crate::client::ApiClient;
use crate::output::{self, Format};

#[derive(Debug, Subcommand)]
pub enum FleetsCmd {
    /// List fleets in your repository.
    List(ListArgs),
    /// Show a single fleet.
    Get {
        /// Fleet ID.
        fleet_id: String,
    },
    /// Create a new fleet.
    Create(CreateArgs),
    /// Delete a fleet.
    Delete {
        /// Fleet ID.
        fleet_id: String,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// List the devices belonging to a fleet.
    Devices {
        /// Fleet ID.
        fleet_id: String,
    },
    /// Add devices to a (static) fleet.
    AddDevices {
        /// Fleet ID.
        fleet_id: String,
        /// Device UUID (repeatable).
        #[arg(long = "device", required = true)]
        devices: Vec<String>,
    },
    /// Remove devices from a (static) fleet.
    RemoveDevices {
        /// Fleet ID.
        fleet_id: String,
        /// Device UUID (repeatable).
        #[arg(long = "device", required = true)]
        devices: Vec<String>,
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    limit: Option<u32>,
    #[arg(long)]
    offset: Option<u32>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FleetType {
    Static,
    Dynamic,
}

impl FleetType {
    fn as_str(self) -> &'static str {
        match self {
            FleetType::Static => "static",
            FleetType::Dynamic => "dynamic",
        }
    }
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Fleet name.
    #[arg(long)]
    name: String,
    /// Fleet type.
    #[arg(long = "type", value_enum, default_value_t = FleetType::Static)]
    fleet_type: FleetType,
    /// Membership expression (required for dynamic fleets).
    #[arg(long)]
    expression: Option<String>,
}

pub fn run(ctx: &Ctx, cmd: FleetsCmd) -> Result<()> {
    let client = ctx.client()?;
    match cmd {
        FleetsCmd::List(args) => list(ctx, &client, args),
        FleetsCmd::Get { fleet_id } => {
            let v = client.get(&format!("/fleets/{fleet_id}"), &[])?;
            output::print_json(&v);
            Ok(())
        }
        FleetsCmd::Create(args) => create(ctx, &client, args),
        FleetsCmd::Delete { fleet_id, yes } => {
            if !yes && !confirm(&format!("Delete fleet {fleet_id}?"))? {
                println!("Aborted.");
                return Ok(());
            }
            client.delete(&format!("/fleets/{fleet_id}"))?;
            output::report_status(
                ctx.format,
                &format!("Fleet {fleet_id} deleted."),
                &json!({ "status": "deleted", "fleetId": fleet_id }),
            );
            Ok(())
        }
        FleetsCmd::Devices { fleet_id } => devices(ctx, &client, &fleet_id),
        FleetsCmd::AddDevices { fleet_id, devices } => {
            client.post_json(&format!("/fleets/{fleet_id}/devices"), &json!(devices))?;
            output::report_status(
                ctx.format,
                &format!("Added {} device(s) to fleet {fleet_id}.", devices.len()),
                &json!({ "status": "added", "fleetId": fleet_id, "devices": devices }),
            );
            Ok(())
        }
        FleetsCmd::RemoveDevices {
            fleet_id,
            devices,
            yes,
        } => {
            if !yes
                && !confirm(&format!(
                    "Remove {} device(s) from fleet {fleet_id}?",
                    devices.len()
                ))?
            {
                println!("Aborted.");
                return Ok(());
            }
            client.delete_json(&format!("/fleets/{fleet_id}/devices"), &json!(devices))?;
            output::report_status(
                ctx.format,
                &format!("Removed {} device(s) from fleet {fleet_id}.", devices.len()),
                &json!({ "status": "removed", "fleetId": fleet_id, "devices": devices }),
            );
            Ok(())
        }
    }
}

fn list(ctx: &Ctx, client: &ApiClient, args: ListArgs) -> Result<()> {
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(v) = args.limit {
        query.push(("limit", v.to_string()));
    }
    if let Some(v) = args.offset {
        query.push(("offset", v.to_string()));
    }
    let resp = client.get("/fleets", &query)?;
    match ctx.format {
        Format::Json => output::print_json(&resp),
        Format::Human => output::print_table(
            &output::paginated_values(&resp),
            &[
                ("id", "ID"),
                ("name", "NAME"),
                ("fleetType", "TYPE"),
                ("expression", "EXPRESSION"),
                ("createdAt", "CREATED-AT"),
            ],
        ),
    }
    Ok(())
}

fn create(ctx: &Ctx, client: &ApiClient, args: CreateArgs) -> Result<()> {
    if matches!(args.fleet_type, FleetType::Dynamic) && args.expression.is_none() {
        bail!("dynamic fleets require --expression");
    }
    let mut body = json!({ "name": args.name, "fleetType": args.fleet_type.as_str() });
    if let Some(expr) = args.expression {
        body["expression"] = json!(expr);
    }
    let resp = client.post_json("/fleets", &body)?;
    output::report_data(ctx.format, "Fleet created.", &resp);
    Ok(())
}

fn devices(ctx: &Ctx, client: &ApiClient, fleet_id: &str) -> Result<()> {
    let resp = client.get(&format!("/fleets/{fleet_id}/devices"), &[])?;
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
            ],
        ),
    }
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
