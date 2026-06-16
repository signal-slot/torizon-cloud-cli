//! `torizon packages` — list, inspect, upload, edit, and delete packages.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{json, Map, Value};

/// Package sort field (maps to the API's exact enum values).
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SortBy {
    Filename,
    CreatedAt,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SortDir {
    Asc,
    Desc,
}

impl SortBy {
    fn api(self) -> &'static str {
        match self {
            SortBy::Filename => "Filename",
            SortBy::CreatedAt => "CreatedAt",
        }
    }
}

impl SortDir {
    fn api(self) -> &'static str {
        match self {
            SortDir::Asc => "Asc",
            SortDir::Desc => "Desc",
        }
    }
}

use super::Ctx;
use crate::client::ApiClient;
use crate::output::{self, Format};

#[derive(Debug, Subcommand)]
pub enum PackagesCmd {
    /// List packages in your repository.
    List(ListArgs),
    /// Show metadata for a single package.
    Get {
        /// Package ID.
        package_id: String,
    },
    /// Upload a new package.
    Upload(UploadArgs),
    /// Edit metadata (comment / hardware IDs) of a package.
    Edit(EditArgs),
    /// Delete a package by ID.
    Delete(DeleteArgs),
    /// List packages from external sources.
    External,
}

#[derive(Debug, Args)]
#[command(disable_version_flag = true)]
pub struct ListArgs {
    #[arg(long)]
    limit: Option<u32>,
    #[arg(long)]
    offset: Option<u32>,
    /// Only packages whose name contains this substring.
    #[arg(long)]
    name_contains: Option<String>,
    /// Only packages whose ID contains this substring.
    #[arg(long)]
    id_contains: Option<String>,
    /// Filter by exact version.
    #[arg(long)]
    version: Option<String>,
    /// Filter by compatible hardware ID (repeatable).
    #[arg(long = "hardware-id")]
    hardware_ids: Vec<String>,
    /// Sort field.
    #[arg(long, value_enum)]
    sort_by: Option<SortBy>,
    /// Sort direction.
    #[arg(long, value_enum)]
    sort_direction: Option<SortDir>,
}

#[derive(Debug, Args)]
#[command(disable_version_flag = true)]
pub struct UploadArgs {
    /// Package name.
    #[arg(long)]
    name: String,
    /// Package version.
    #[arg(long)]
    version: String,
    /// Compatible hardware ID (repeatable, at least one required).
    #[arg(long = "hardware-id", required = true)]
    hardware_id: Vec<String>,
    /// Target format (e.g. "OSTREE" or "BINARY").
    #[arg(long)]
    format: String,
    /// Path to the package file to upload.
    #[arg(long)]
    file: PathBuf,
}

#[derive(Debug, Args)]
pub struct EditArgs {
    /// Package ID.
    package_id: String,
    /// Replacement comment.
    #[arg(long)]
    comment: Option<String>,
    /// Replacement hardware ID (repeatable). Replaces the existing set.
    #[arg(long = "hardware-id")]
    hardware_ids: Vec<String>,
}

#[derive(Debug, Args)]
pub struct DeleteArgs {
    /// Package ID to delete.
    package_id: String,
    /// Skip the confirmation prompt.
    #[arg(long, short = 'y')]
    yes: bool,
}

pub fn run(ctx: &Ctx, cmd: PackagesCmd) -> Result<()> {
    let client = ctx.client()?;
    match cmd {
        PackagesCmd::List(args) => list(ctx, &client, args),
        PackagesCmd::Get { package_id } => {
            let v = client.get(&format!("/packages/{package_id}"), &[])?;
            output::print_object(ctx.format, &v);
            Ok(())
        }
        PackagesCmd::Upload(args) => upload(ctx, &client, args),
        PackagesCmd::Edit(args) => edit(ctx, &client, args),
        PackagesCmd::Delete(args) => delete(ctx, &client, args),
        PackagesCmd::External => {
            let resp = client.get("/packages_external", &[])?;
            match ctx.format {
                Format::Json => output::print_json(&resp),
                Format::Human => output::print_table(
                    &output::paginated_values(&resp),
                    &[
                        ("packageId", "ID"),
                        ("name", "NAME"),
                        ("version", "VERSION"),
                        ("packageSource", "SOURCE"),
                    ],
                ),
            }
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
    if let Some(v) = args.id_contains {
        q.push(("idContains", v));
    }
    if let Some(v) = args.version {
        q.push(("version", v));
    }
    for hw in &args.hardware_ids {
        q.push(("hardwareIds", hw.clone()));
    }
    if let Some(v) = args.sort_by {
        q.push(("sortBy", v.api().to_string()));
    }
    if let Some(v) = args.sort_direction {
        q.push(("sortDirection", v.api().to_string()));
    }

    let resp = client.get("/packages", &q)?;
    match ctx.format {
        Format::Json => output::print_json(&resp),
        Format::Human => output::print_table(
            &output::paginated_values(&resp),
            &[
                ("packageId", "ID"),
                ("name", "NAME"),
                ("version", "VERSION"),
                ("pkgType", "TYPE"),
                ("hardwareIds", "HARDWARE-IDS"),
                ("size", "SIZE"),
                ("createdAt", "CREATED-AT"),
            ],
        ),
    }
    Ok(())
}

fn upload(ctx: &Ctx, client: &ApiClient, args: UploadArgs) -> Result<()> {
    let bytes =
        std::fs::read(&args.file).with_context(|| format!("reading {}", args.file.display()))?;
    let mut q: Vec<(&str, String)> = vec![
        ("name", args.name),
        ("version", args.version),
        ("targetFormat", args.format),
    ];
    for hw in args.hardware_id {
        q.push(("hardwareId", hw));
    }
    let resp = client.post_octet_stream("/packages", &q, bytes)?;
    output::report_data(ctx.format, "Package uploaded.", &resp);
    Ok(())
}

fn edit(ctx: &Ctx, client: &ApiClient, args: EditArgs) -> Result<()> {
    let mut body = Map::new();
    if let Some(c) = args.comment {
        body.insert("comment".into(), json!(c));
    }
    if !args.hardware_ids.is_empty() {
        body.insert("hardwareIds".into(), json!(args.hardware_ids));
    }
    if body.is_empty() {
        anyhow::bail!("nothing to edit; pass --comment and/or --hardware-id");
    }
    let resp = client.patch_json(
        &format!("/packages/{}", args.package_id),
        &Value::Object(body),
    )?;
    output::report_data(ctx.format, "Package updated.", &resp);
    Ok(())
}

fn delete(ctx: &Ctx, client: &ApiClient, args: DeleteArgs) -> Result<()> {
    if !args.yes && !confirm(&format!("Delete package {}?", args.package_id))? {
        println!("Aborted.");
        return Ok(());
    }
    client.delete(&format!("/packages/{}", args.package_id))?;
    output::report_status(
        ctx.format,
        &format!("Package {} deleted.", args.package_id),
        &json!({ "status": "deleted", "packageId": args.package_id }),
    );
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
