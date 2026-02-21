mod gerrit;
mod stats;

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use clap::Parser;

use gerrit::{ChangeQuery, ChangeStatus, GerritClient};
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
    /// Useful for limiting fetch time on accounts with long histories.
    #[arg(long)]
    after: Option<String>,

    /// HTTP Basic Auth username (for private Gerrit instances).
    #[arg(long)]
    username: Option<String>,

    /// Gerrit HTTP password (paired with --username).
    #[arg(long)]
    password: Option<String>,
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
// Text report
// ---------------------------------------------------------------------------

fn print_report(owner: &str, s: &Stats) {
    let width = 60;
    let bar = "─".repeat(width);

    println!();
    println!("┌{bar}┐");
    println!("│  gerritoscopy · {owner:<width$}│", width = width - 18);
    println!("└{bar}┘");

    print_heatmap(&s.heatmap);

    println!();
    println!("  Merged CLs     {:>7}  (all time)", fmt_i(s.total_merged as i64));
    println!("  Last 90 days   {:>7}", fmt_i(s.recent_merged_90d as i64));
    println!(
        "  Lines changed  +{} / -{}",
        fmt_i(s.total_insertions),
        fmt_i(s.total_deletions),
    );
    println!(
        "  Streak         current {:} wk  ·  longest {:} wk",
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
                fmt_i(p.merged as i64),
                fmt_i(p.insertions),
                fmt_i(p.deletions),
            );
        }
    }

    println!();
}

fn print_heatmap(h: &Heatmap) {
    // Block characters for intensity levels 0-4.
    const BLOCKS: [char; 5] = [' ', '░', '▒', '▓', '█'];

    // Month-label row: print abbreviated month name at the first bucket of
    // each new calendar month.  Each bucket is 1 char wide.
    let mut label_row = vec![' '; h.weeks.len()];
    let mut last_month = 0u32;
    let mut last_month_pos = 0usize;
    for (i, b) in h.weeks.iter().enumerate() {
        let m = b.week_start.month();
        if m != last_month {
            // Write a 3-char month abbreviation if there's room before the
            // next label position.
            let abbr = month_abbr(m);
            if i == 0 || i >= last_month_pos + 4 {
                for (j, ch) in abbr.chars().enumerate() {
                    if i + j < label_row.len() {
                        label_row[i + j] = ch;
                    }
                }
                last_month_pos = i;
            }
            last_month = m;
        }
    }

    let label_str: String = label_row.into_iter().collect();
    let heat_str: String = h
        .weeks
        .iter()
        .map(|b| BLOCKS[b.level(h.max_count) as usize])
        .collect();

    println!();
    println!("  {label_str}");
    println!("  [{heat_str}]");
    println!("  peak: {} CLs/week", h.max_count);
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn fmt_i(n: i64) -> String {
    // Insert thousands separators.
    let digits = n.unsigned_abs().to_string();
    let grouped: String = digits
        .chars()
        .rev()
        .enumerate()
        .flat_map(|(i, c)| {
            if i > 0 && i % 3 == 0 {
                Some(',')
            } else {
                None
            }
            .into_iter()
            .chain(std::iter::once(c))
        })
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if n < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

fn month_abbr(m: u32) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}
