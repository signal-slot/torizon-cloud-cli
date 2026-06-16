//! Unofficial command-line interface for the Torizon Cloud (Torizon OTA v2) API.

mod auth;
mod client;
mod commands;
mod config;
mod output;

use std::io;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use commands::Ctx;
use commands::{
    devices::DevicesCmd, fleets::FleetsCmd, lockboxes::LockboxesCmd, login::LoginArgs,
    metrics::MetricsCmd, packages::PackagesCmd, remote_access::RemoteAccessCmd,
    updates::UpdatesCmd,
};
use output::Format;

#[derive(Debug, Parser)]
#[command(
    name = "torizon",
    version,
    about = "Unofficial CLI for the Torizon Cloud API",
    propagate_version = true
)]
struct Cli {
    /// Output raw JSON instead of human-readable tables.
    #[arg(long, global = true)]
    json: bool,
    /// Credentials profile to use (defaults to the configured default profile).
    #[arg(long, global = true)]
    profile: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Store and verify API-client credentials.
    Login(LoginArgs),
    /// Manage devices.
    Devices {
        #[command(subcommand)]
        cmd: DevicesCmd,
    },
    /// Manage packages.
    Packages {
        #[command(subcommand)]
        cmd: PackagesCmd,
    },
    /// Launch and cancel updates.
    Updates {
        #[command(subcommand)]
        cmd: UpdatesCmd,
    },
    /// Manage fleets.
    Fleets {
        #[command(subcommand)]
        cmd: FleetsCmd,
    },
    /// Read device and fleet metrics.
    Metrics {
        #[command(subcommand)]
        cmd: MetricsCmd,
    },
    /// Manage lockboxes (offline updates).
    Lockboxes {
        #[command(subcommand)]
        cmd: LockboxesCmd,
    },
    /// Manage remote access (sessions, SSH keys, IP accept-list).
    RemoteAccess {
        #[command(subcommand)]
        cmd: RemoteAccessCmd,
    },
    /// Generate a shell completion script (bash, zsh, fish, ...).
    Completions {
        /// Target shell.
        shell: Shell,
    },
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = dispatch(cli) {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn dispatch(cli: Cli) -> anyhow::Result<()> {
    // The completions command needs no credentials, so handle it before building Ctx.
    if let Command::Completions { shell } = cli.command {
        let mut cmd = Cli::command();
        let name = cmd.get_name().to_string();
        clap_complete::generate(shell, &mut cmd, name, &mut io::stdout());
        return Ok(());
    }

    let ctx = Ctx {
        format: if cli.json {
            Format::Json
        } else {
            Format::Human
        },
        profile: cli.profile,
    };

    match cli.command {
        Command::Login(args) => commands::login::run(args),
        Command::Devices { cmd } => commands::devices::run(&ctx, cmd),
        Command::Packages { cmd } => commands::packages::run(&ctx, cmd),
        Command::Updates { cmd } => commands::updates::run(&ctx, cmd),
        Command::Fleets { cmd } => commands::fleets::run(&ctx, cmd),
        Command::Metrics { cmd } => commands::metrics::run(&ctx, cmd),
        Command::Lockboxes { cmd } => commands::lockboxes::run(&ctx, cmd),
        Command::RemoteAccess { cmd } => commands::remote_access::run(&ctx, cmd),
        Command::Completions { .. } => unreachable!("handled above"),
    }
}
