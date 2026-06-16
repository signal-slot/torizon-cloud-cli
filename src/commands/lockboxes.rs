//! `torizon lockboxes` — manage lockbox (offline-update) definitions.

use std::io::Write;

use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::json;

use super::Ctx;
use crate::output::{self, Format};

#[derive(Debug, Subcommand)]
pub enum LockboxesCmd {
    /// List all lockboxes.
    List,
    /// List all lockboxes with their detailed contents.
    Details,
    /// Show the raw Uptane metadata for a lockbox.
    Get { name: String },
    /// Define or update a lockbox.
    Set(SetArgs),
    /// Delete a lockbox.
    Delete {
        name: String,
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Debug, Args)]
pub struct SetArgs {
    /// Lockbox name.
    name: String,
    /// Package ID to include (repeatable, at least one required).
    #[arg(long = "package", required = true)]
    packages: Vec<String>,
    /// Expiry timestamp (RFC3339, e.g. 2026-12-31T00:00:00Z).
    #[arg(long)]
    expires_at: Option<String>,
}

pub fn run(ctx: &Ctx, cmd: LockboxesCmd) -> Result<()> {
    let client = ctx.client()?;
    match cmd {
        LockboxesCmd::List => {
            let resp = client.get("/lockboxes", &[])?;
            match ctx.format {
                Format::Json => output::print_json(&resp),
                Format::Human => {
                    for v in output::paginated_values(&resp) {
                        match v.as_str() {
                            Some(s) => println!("{s}"),
                            None => println!("{v}"),
                        }
                    }
                }
            }
            Ok(())
        }
        LockboxesCmd::Details => {
            output::print_json(&client.get("/lockbox-details", &[])?);
            Ok(())
        }
        LockboxesCmd::Get { name } => {
            output::print_json(&client.get(&format!("/lockboxes/{name}"), &[])?);
            Ok(())
        }
        LockboxesCmd::Set(args) => {
            let mut body = json!({ "packageIds": args.packages });
            if let Some(e) = args.expires_at {
                body["expiresAt"] = json!(e);
            }
            let resp = client.post_json(&format!("/lockboxes/{}", args.name), &body)?;
            output::report_data(
                ctx.format,
                &format!("Lockbox `{}` saved.", args.name),
                &resp,
            );
            Ok(())
        }
        LockboxesCmd::Delete { name, yes } => {
            if !yes && !confirm(&format!("Delete lockbox {name}?"))? {
                println!("Aborted.");
                return Ok(());
            }
            client.delete(&format!("/lockboxes/{name}"))?;
            output::report_status(
                ctx.format,
                &format!("Lockbox {name} deleted."),
                &json!({"status":"deleted","name": name}),
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
