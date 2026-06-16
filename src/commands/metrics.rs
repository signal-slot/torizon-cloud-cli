//! `torizon metrics` — read device and fleet metrics.

use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::json;

use super::Ctx;
use crate::client::{encode_segment as enc, ApiClient};
use crate::output::{self, Format};

#[derive(Debug, Subcommand)]
pub enum MetricsCmd {
    /// List the metric names available in your repository.
    Names,
    /// Get metrics time series for a single device.
    Device {
        device_uuid: String,
        #[command(flatten)]
        range: RangeArgs,
    },
    /// Get detailed metrics for a single device.
    Detailed {
        device_uuid: String,
        #[command(flatten)]
        range: RangeArgs,
    },
    /// Get aggregated metrics for a fleet.
    Fleet {
        fleet_id: String,
        #[command(flatten)]
        range: RangeArgs,
    },
    /// Get outlier metrics for a fleet.
    Outliers {
        fleet_id: String,
        #[command(flatten)]
        range: RangeArgs,
    },
    /// Get a metrics report for a fleet.
    Report {
        fleet_id: String,
        #[command(flatten)]
        range: RangeArgs,
    },
}

#[derive(Debug, Args)]
pub struct RangeArgs {
    /// Metric name to fetch (repeatable).
    #[arg(long = "metric")]
    metrics: Vec<String>,
    /// Range start, UNIX epoch seconds.
    #[arg(long)]
    from: i64,
    /// Range end, UNIX epoch seconds.
    #[arg(long)]
    to: i64,
    /// Resolution in seconds (time-series queries only).
    #[arg(long)]
    resolution: Option<i64>,
}

impl RangeArgs {
    fn to_query(&self) -> Vec<(&str, String)> {
        let mut q: Vec<(&str, String)> =
            vec![("from", self.from.to_string()), ("to", self.to.to_string())];
        if let Some(r) = self.resolution {
            q.push(("resolution", r.to_string()));
        }
        for m in &self.metrics {
            q.push(("metric", m.clone()));
        }
        q
    }
}

pub fn run(ctx: &Ctx, cmd: MetricsCmd) -> Result<()> {
    let client = ctx.client()?;
    match cmd {
        MetricsCmd::Names => names(ctx, &client),
        MetricsCmd::Device { device_uuid, range } => {
            output::print_json(&client.get(
                &format!("/device-data/devices/{}/metrics", enc(&device_uuid)),
                &range.to_query(),
            )?);
            Ok(())
        }
        MetricsCmd::Detailed { device_uuid, range } => {
            let mut q: Vec<(&str, String)> = vec![
                ("from", range.from.to_string()),
                ("to", range.to.to_string()),
            ];
            for m in &range.metrics {
                q.push(("metrics", m.clone()));
            }
            output::print_json(&client.get(
                &format!(
                    "/device-data/devices/{}/detailed-metrics",
                    enc(&device_uuid)
                ),
                &q,
            )?);
            Ok(())
        }
        MetricsCmd::Fleet { fleet_id, range } => {
            output::print_json(&client.get(
                &format!("/device-data/fleets/{}/metrics", enc(&fleet_id)),
                &range.to_query(),
            )?);
            Ok(())
        }
        MetricsCmd::Outliers { fleet_id, range } => {
            let body = json!({ "metrics": range.metrics, "from": range.from, "to": range.to });
            output::print_json(&client.post_json(
                &format!("/device-data/fleets/{}/metrics/outliers", enc(&fleet_id)),
                &body,
            )?);
            Ok(())
        }
        MetricsCmd::Report { fleet_id, range } => {
            let body = json!({ "metrics": range.metrics, "from": range.from, "to": range.to });
            output::print_json(&client.post_json(
                &format!("/device-data/fleets/{}/metrics/report", enc(&fleet_id)),
                &body,
            )?);
            Ok(())
        }
    }
}

fn names(ctx: &Ctx, client: &ApiClient) -> Result<()> {
    let resp = client.get("/device-data/metric-names", &[])?;
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
