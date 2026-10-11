//! `ffpr` command line: build, serve, blurb.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use ffpr::build::{render_site, SiteOptions};
use ffpr::config::{resolve_league_id, Config, CONFIG_PATH};
use ffpr::pipeline;
use ffpr::serve;
use ffpr::sleeper::SleeperClient;

const DATA_DIR: &str = "data";
const SITE_DIR: &str = "site";

#[derive(Parser)]
#[command(about = "Sleeper fantasy football power rankings static site generator.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch (using the cache), compute, and write site/.
    Build {
        /// Build only through this week.
        #[arg(long)]
        through: Option<i64>,
        /// Also include the in-progress week, excluded from rankings.
        #[arg(long)]
        provisional: bool,
        /// Sleeper league id override.
        #[arg(long)]
        league: Option<String>,
        /// Season override (must match league).
        #[arg(long)]
        season: Option<String>,
    },
    /// Serve site/ at http://localhost:<port> for local preview.
    Serve {
        /// Local port to serve site/ on.
        #[arg(long, default_value_t = 8000)]
        port: u16,
    },
    /// Print a short chat-ready summary for pasting into the league chat.
    Blurb {
        /// Week to summarize (default: latest).
        #[arg(long)]
        week: Option<i64>,
        /// Sleeper league id override.
        #[arg(long)]
        league: Option<String>,
        /// Season override (must match league).
        #[arg(long)]
        season: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build {
            through,
            provisional,
            league,
            season,
        } => build(through, provisional, league.as_deref(), season.as_deref()),
        Command::Serve { port } => {
            let site = Path::new(SITE_DIR);
            if !site.exists() {
                bail!("site/ doesn't exist yet -- run `ffpr build` first.");
            }
            println!("Serving site/ at http://localhost:{port}/ (Ctrl+C to stop)");
            serve::serve(site, port)
        }
        Command::Blurb {
            week,
            league,
            season,
        } => {
            let config = Config::load(Path::new(CONFIG_PATH))?;
            let league_id = resolve_league_id(league.as_deref(), &config)?;
            let settings = config.settings();
            let client = SleeperClient::new(DATA_DIR);
            let text = pipeline::blurb(&client, &league_id, week, season.as_deref(), &settings)?;
            println!("{text}");
            Ok(())
        }
    }
}

fn build(
    through: Option<i64>,
    provisional: bool,
    league: Option<&str>,
    season: Option<&str>,
) -> Result<()> {
    let config = Config::load(Path::new(CONFIG_PATH))?;
    let league_id = resolve_league_id(league, &config)?;
    let settings = config.settings();
    let client = SleeperClient::new(DATA_DIR);

    let all = pipeline::build_all(&client, &league_id, season, through, provisional, &settings)?;
    let all_seasons: Vec<String> = all.iter().map(|s| s.season.clone()).collect();
    let rivalries = ffpr::compute::rivalries::build_rivalries(&all);

    let site = PathBuf::from(SITE_DIR);
    let opts_root = SiteOptions {
        site_url: &settings.site_url,
        all_seasons: &all_seasons,
        site_root_prefix: "",
        rivalries: &rivalries,
    };
    render_site(&all[0], &site, &opts_root)?;
    for s in &all {
        let dir = site.join(&s.season);
        let opts = SiteOptions {
            site_url: &settings.site_url,
            all_seasons: &all_seasons,
            site_root_prefix: "../",
            rivalries: &rivalries,
        };
        render_site(s, &dir, &opts).with_context(|| format!("rendering {}", dir.display()))?;
    }

    let current = &all[0];
    let past = all.len() - 1;
    let mut msg = format!("Built site/ through week {}", current.through_week);
    if let Some(week) = current.provisional_week {
        msg.push_str(&format!(" (+ provisional week {week})"));
    }
    if past > 0 {
        msg.push_str(&format!(", plus {past} past season(s)"));
    }
    println!("{msg}");
    Ok(())
}
