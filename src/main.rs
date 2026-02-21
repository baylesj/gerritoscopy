mod gerrit;
mod render;
mod stats;

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::Parser;

use gerrit::{ChangeQuery, ChangeStatus, GerritClient};
use render::{fmt_count, heatmap_body, heatmap_header};
use stats::{Heatmap, Stats};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "gerritoscopy",
    about = "Fetch Gerrit contribution stats and render a profile heatmap"
)]
struct Args {
    /// Gerrit base URL.
    #[arg(long, default_value = "https://chromium-review.googlesource.com")]
    host: String,

    /// Account to query — email address, username, or `self`.
    #[arg(long)]
    owner: String,

    /// Only include changes submitted on or after this date (YYYY-MM-DD).
    #[arg(long)]
    after: Option<String>,

    /// HTTP Basic Auth username (for private Gerrit instances).
    #[arg(long)]
    username: Option<String>,

    /// Gerrit HTTP password (paired with --username).
    #[arg(long)]
    password: Option<String>,

    /// Write a markdown report to this file.
    #[arg(long)]
    output_md: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let client = build_client(&args)?;
    let query = build_query(&args)?;

    eprintln!("fetching changes for {} from {} …", args.owner, args.host);
    let changes = client.fetch_changes(&query).await?;
    eprintln!("  {} CLs fetched", changes.len());

    let stats = stats::compute(&changes, chrono::Utc::now());
    print_report(&args.owner, &stats);

    if let Some(ref path) = args.output_md {
        let md = render::markdown::render(&args.owner, &args.host, &stats)?;
        std::fs::write(path, &md)
            .with_context(|| format!("writing {}", path.display()))?;
        eprintln!("wrote {}", path.display());
    }

    Ok(())
}

fn build_client(args: &Args) -> Result<GerritClient> {
    let c = GerritClient::new(&args.host)?;
    Ok(match (&args.username, &args.password) {
        (Some(u), Some(p)) => c.with_auth(u, p),
        _ => c,
    })
}

fn build_query(args: &Args) -> Result<ChangeQuery> {
    let mut q = ChangeQuery::new(&args.owner).with_status(ChangeStatus::Merged);
    if let Some(ref s) = args.after {
        let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .with_context(|| format!("--after value {s:?} is not YYYY-MM-DD"))?;
        q = q.with_after(date);
    }
    Ok(q)
}

// ---------------------------------------------------------------------------
// Terminal report
// ---------------------------------------------------------------------------

fn print_report(owner: &str, s: &Stats) {
    let width = 60;
    let bar = "─".repeat(width);

    println!();
    println!("┌{bar}┐");
    println!("│  gerritoscopy · {owner:<width$}│", width = width - 17);
    println!("└{bar}┘");

    print_heatmap(&s.heatmap);

    println!();
    println!("  Merged CLs     {:>7}  (all time)", fmt_count(s.total_merged as i64));
    println!("  Last 90 days   {:>7}", fmt_count(s.recent_merged_90d as i64));
    println!(
        "  Lines changed  +{} / -{}",
        fmt_count(s.total_insertions),
        fmt_count(s.total_deletions),
    );
    println!(
        "  Streak         current {} wk  ·  longest {} wk",
        s.heatmap.current_streak(),
        s.heatmap.longest_streak(),
    );

    if !s.top_projects.is_empty() {
        println!();
        println!("  Top projects");
        for p in &s.top_projects {
            println!(
                "    {:<36} {:>5} CLs  +{} / -{}",
                truncate(&p.name, 36),
                fmt_count(p.merged as i64),
                fmt_count(p.insertions),
                fmt_count(p.deletions),
            );
        }
    }

    println!();
}

fn print_heatmap(h: &Heatmap) {
    println!();
    println!("  {}", heatmap_header(h));
    println!("  [{}]", heatmap_body(h));
    println!("  peak: {} CLs/week", h.max_count);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        format!("{}…", &s[..max - 1])
    }
}
