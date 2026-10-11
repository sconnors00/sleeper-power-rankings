//! Renders `site/` from a `SeasonSummary`: HTML pages through minijinja, plus
//! `data.js` (the chart payload) and `rosters.json` (the scripts' data).

mod context;
mod env;
mod payloads;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::models::{Manager, SeasonSummary};

pub use payloads::{build_data_js, build_rosters_json};

pub const CHART_JS_URL: &str = "https://cdn.jsdelivr.net/npm/chart.js@4.5.1/dist/chart.umd.min.js";
pub const CHART_JS_SRI: &str =
    "sha384-jb8JQMbMoBUzgWatfe6COACi2ljcDdZQ2OxczGA3bGNeWe+6DChMTBJemed7ZnvJ";

/// Settings shared by every season written in one build.
pub struct SiteOptions<'a> {
    pub site_url: &'a str,
    pub all_seasons: &'a [String],
    /// This season's path back to the site root: "" for the root call.
    pub site_root_prefix: &'a str,
    pub rivalries: &'a [Manager],
}

/// Render one season into `output_dir`.
pub fn render_site(season: &SeasonSummary, output_dir: &Path, opts: &SiteOptions) -> Result<()> {
    let weeks_dir = output_dir.join("weeks");
    fs::create_dir_all(&weeks_dir)
        .with_context(|| format!("cannot create {}", weeks_dir.display()))?;

    let static_out = output_dir.join("static");
    if static_out.exists() {
        fs::remove_dir_all(&static_out)?;
    }
    fs::create_dir_all(&static_out)?;
    for (name, bytes) in env::STATIC_FILES {
        fs::write(static_out.join(name), bytes)?;
    }

    let data_js = build_data_js(season);
    write(&output_dir.join("data.js"), &data_js)?;

    let environment = env::environment();
    let common = context::common(season, &data_js, opts);
    let render = |name: &str, extra: context::Ctx| -> Result<String> {
        let template = environment.get_template(name)?;
        Ok(template.render(context::merge(&common, extra))?)
    };

    let preseason_rows = context::preseason_rows(season);
    if season.draft.is_some() {
        let html = render("draft.html", context::draft(season))?;
        write(&output_dir.join("draft.html"), &html)?;
    }
    if !season.preseason.is_empty() {
        let html = render("week0.html", context::preseason_page(&preseason_rows))?;
        write(&weeks_dir.join("week-0.html"), &html)?;
    }
    if crate::pipeline::is_complete(season) {
        let html = render("season_recap.html", context::recap(season))?;
        write(
            &weeks_dir.join(format!("week-{}.html", season.playoff_week_start)),
            &html,
        )?;
    }
    if !opts.rivalries.is_empty() {
        let html = render("rivalries.html", context::rivalries(opts.rivalries))?;
        write(&output_dir.join("rivalries.html"), &html)?;
    }
    if context::has_trades_page(season) {
        let html = render("trades.html", context::trades(season))?;
        write(&output_dir.join("trades.html"), &html)?;
    }
    if let Some(report) = &season.roster_report {
        let html = render("rosters.html", context::rosters(season, report))?;
        write(&output_dir.join("rosters.html"), &html)?;
        write(
            &output_dir.join("rosters.json"),
            &build_rosters_json(season),
        )?;
    }
    let html = render("how_it_works.html", context::how_it_works(season))?;
    write(&output_dir.join("how-it-works.html"), &html)?;

    if season.weeks.is_empty() {
        let html = render("landing.html", context::landing(&preseason_rows))?;
        write(&output_dir.join("index.html"), &html)?;
        if let Some(prov) = &season.provisional_week_summary {
            let html = render("week.html", context::week(season, prov, "../", false))?;
            write(&weeks_dir.join(format!("week-{}.html", prov.week)), &html)?;
        }
        return Ok(());
    }

    for wk in &season.weeks {
        let html = render("week.html", context::week(season, wk, "../", false))?;
        write(&weeks_dir.join(format!("week-{}.html", wk.week)), &html)?;
    }
    if let Some(prov) = &season.provisional_week_summary {
        let html = render("week.html", context::week(season, prov, "../", false))?;
        write(&weeks_dir.join(format!("week-{}.html", prov.week)), &html)?;
    }

    let latest = season.weeks.last().expect("weeks is non-empty");
    let html = render("week.html", context::week(season, latest, "", true))?;
    write(&output_dir.join("index.html"), &html)?;

    let html = render("season.html", context::season(season))?;
    write(&output_dir.join("season.html"), &html)?;
    Ok(())
}

fn write(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("cannot write {}", path.display()))
}
