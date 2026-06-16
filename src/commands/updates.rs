//! `torizon updates` — launch, cancel, list, and watch updates.

use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use serde_json::{json, Value};

use super::Ctx;
use crate::client::ApiClient;
use crate::output::{self, Format};

#[derive(Debug, Subcommand)]
pub enum UpdatesCmd {
    /// Launch an update to one or more devices or fleets.
    Launch(LaunchArgs),
    /// Cancel a pending update.
    Cancel {
        /// Update ID to cancel.
        update_id: String,
    },
    /// List the update history for a device.
    List {
        /// Device UUID.
        device_uuid: String,
    },
    /// Show the most recent update for a device.
    Status {
        /// Device UUID.
        device_uuid: String,
    },
    /// Poll a device until its current update finishes (or times out).
    Watch(WatchArgs),
}

#[derive(Debug, Args)]
pub struct LaunchArgs {
    /// Package ID to deploy (repeatable, at least one required).
    #[arg(long = "package", required = true)]
    packages: Vec<String>,
    /// Target device UUID (repeatable).
    #[arg(long = "device")]
    devices: Vec<String>,
    /// Target fleet ID (repeatable).
    #[arg(long = "fleet")]
    fleets: Vec<String>,
}

#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Device UUID.
    device_uuid: String,
    /// Seconds between polls.
    #[arg(long, default_value_t = 30)]
    interval: u64,
    /// Give up after this many seconds.
    #[arg(long, default_value_t = 3600)]
    timeout: u64,
}

pub fn run(ctx: &Ctx, cmd: UpdatesCmd) -> Result<()> {
    let client = ctx.client()?;
    match cmd {
        UpdatesCmd::Launch(args) => launch(ctx, &client, args),
        UpdatesCmd::Cancel { update_id } => {
            client.patch(&format!("/updates/{update_id}"), &[])?;
            output::report_status(
                ctx.format,
                &format!("Update {update_id} cancelled."),
                &json!({ "status": "cancelled", "updateId": update_id }),
            );
            Ok(())
        }
        UpdatesCmd::List { device_uuid } => list(ctx, &client, &device_uuid),
        UpdatesCmd::Status { device_uuid } => {
            let latest = latest_update(&client, &device_uuid)?;
            match latest {
                Some(u) => output::print_object(ctx.format, &flatten_update(&u)),
                None => println!("(no updates for this device)"),
            }
            Ok(())
        }
        UpdatesCmd::Watch(args) => watch(&client, args),
    }
}

fn launch(ctx: &Ctx, client: &ApiClient, args: LaunchArgs) -> Result<()> {
    if args.devices.is_empty() && args.fleets.is_empty() {
        bail!("specify at least one --device or --fleet target");
    }
    let mut body = json!({ "packageIds": args.packages });
    if !args.devices.is_empty() {
        body["devices"] = json!(args.devices);
    }
    if !args.fleets.is_empty() {
        body["fleets"] = json!(args.fleets);
    }
    let resp = client.post_json("/updates", &body)?;
    output::report_data(ctx.format, "Update launched.", &resp);
    Ok(())
}

fn list(ctx: &Ctx, client: &ApiClient, device_uuid: &str) -> Result<()> {
    let resp = client.get(&format!("/updates/devices/{device_uuid}"), &[])?;
    match ctx.format {
        Format::Json => output::print_json(&resp),
        Format::Human => {
            let rows: Vec<Value> = output::paginated_values(&resp)
                .iter()
                .map(flatten_update)
                .collect();
            output::print_table(
                &rows,
                &[
                    ("updateId", "UPDATE-ID"),
                    ("status", "STATUS"),
                    ("result", "RESULT"),
                    ("createdAt", "CREATED-AT"),
                    ("completedAt", "COMPLETED-AT"),
                    ("packages", "PACKAGES"),
                ],
            );
        }
    }
    Ok(())
}

/// Flatten an update record's nested `deviceResult`/`packages` into scalar
/// fields suitable for tables and key/value display.
fn flatten_update(u: &Value) -> Value {
    let result = u
        .get("deviceResult")
        .and_then(|d| d.as_object())
        .map(|d| {
            let code = d.get("resultCode").and_then(Value::as_str).unwrap_or("");
            let ok = d.get("success").and_then(Value::as_bool).unwrap_or(false);
            format!("{} {}", if ok { "OK" } else { "FAIL" }, code)
                .trim()
                .to_string()
        })
        .unwrap_or_default();
    let packages = u
        .get("packages")
        .and_then(|p| p.as_object())
        .map(|m| {
            m.values()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    json!({
        "updateId": u.get("updateId").cloned().unwrap_or(Value::Null),
        "status": u.get("status").cloned().unwrap_or(Value::Null),
        "result": result,
        "createdAt": u.get("createdAt").cloned().unwrap_or(Value::Null),
        "completedAt": u.get("completedAt").cloned().unwrap_or(Value::Null),
        "packages": packages,
    })
}

fn latest_update(client: &ApiClient, device_uuid: &str) -> Result<Option<Value>> {
    let resp = client.get(&format!("/updates/devices/{device_uuid}"), &[])?;
    let mut vals = output::paginated_values(&resp);
    vals.sort_by(|a, b| {
        a.get("createdAt")
            .and_then(Value::as_str)
            .cmp(&b.get("createdAt").and_then(Value::as_str))
    });
    Ok(vals.into_iter().last())
}

fn watch(client: &ApiClient, args: WatchArgs) -> Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(args.timeout);
    loop {
        let dev = client.get(&format!("/devices/{}", args.device_uuid), &[])?;
        let dev_status = dev
            .get("deviceStatus")
            .and_then(Value::as_str)
            .unwrap_or("?");
        let last_seen = dev.get("lastSeen").and_then(Value::as_str).unwrap_or("?");
        let latest = latest_update(client, &args.device_uuid)?;
        let (upd_status, result, has_result) = match &latest {
            Some(u) => {
                let s = u
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string();
                let dr = u.get("deviceResult");
                let has = dr.map(|v| !v.is_null()).unwrap_or(false);
                let r = dr
                    .and_then(|d| d.as_object())
                    .map(|d| {
                        format!(
                            "{} {}",
                            if d.get("success").and_then(Value::as_bool).unwrap_or(false) {
                                "OK"
                            } else {
                                "FAIL"
                            },
                            d.get("resultCode").and_then(Value::as_str).unwrap_or("")
                        )
                        .trim()
                        .to_string()
                    })
                    .unwrap_or_default();
                (s, r, has)
            }
            None => ("(none)".to_string(), String::new(), false),
        };

        let elapsed = start.elapsed().as_secs();
        println!("[{elapsed:>4}s] device={dev_status} update={upd_status} result={result} lastSeen={last_seen}");

        if dev_status == "UpToDate" {
            println!("Done: device is UpToDate.");
            return Ok(());
        }
        if has_result {
            let ok = latest
                .as_ref()
                .and_then(|u| u.get("deviceResult"))
                .and_then(|d| d.get("success"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if ok {
                println!("Done: update succeeded ({result}).");
                return Ok(());
            } else {
                bail!("update failed: {result}");
            }
        }
        if start.elapsed() >= timeout {
            bail!(
                "timed out after {}s; last device status: {dev_status}",
                args.timeout
            );
        }
        sleep(Duration::from_secs(args.interval));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_marks_success() {
        let raw = json!({
            "updateId": "u1",
            "status": "Completed",
            "createdAt": "2026-06-15T12:00:00Z",
            "completedAt": "2026-06-15T12:05:00Z",
            "deviceResult": { "resultCode": "OK", "success": true },
            "packages": { "verdin-imx8mp": "astra-os-04", "docker-compose": "astra-demo-04" },
        });
        let f = flatten_update(&raw);
        assert_eq!(f["updateId"], json!("u1"));
        assert_eq!(f["result"], json!("OK OK"));
        let pkgs = f["packages"].as_str().unwrap();
        assert!(pkgs.contains("astra-os-04") && pkgs.contains("astra-demo-04"));
    }

    #[test]
    fn flatten_marks_failure() {
        let raw = json!({
            "updateId": "u2",
            "status": "Completed",
            "deviceResult": { "resultCode": "verdin-imx8mp:INSTALL_FAILED", "success": false },
        });
        let f = flatten_update(&raw);
        assert_eq!(f["result"], json!("FAIL verdin-imx8mp:INSTALL_FAILED"));
    }

    #[test]
    fn flatten_handles_pending_without_result() {
        let raw = json!({ "updateId": "u3", "status": "Seen", "deviceResult": null });
        let f = flatten_update(&raw);
        assert_eq!(f["result"], json!(""));
        assert_eq!(f["status"], json!("Seen"));
    }
}
