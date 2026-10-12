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

    let preseason_rows = context::preseason_rows(season);
    let week_url = |week: i64| format!("weeks/week-{week}.html");

    // Every page this season gets, as (path, template, page context). The nav
    // shows a page only if it's in this list, so adding a page is one entry.
    let mut pages: Vec<(String, &str, context::Ctx)> = Vec::new();
    if let Some(latest) = season.weeks.last() {
        pages.push((
            "index.html".into(),
            "week.html",
            context::week(season, latest, true),
        ));
        pages.push(("season.html".into(), "season.html", context::season(season)));
    } else {
        let landing = context::preseason(&preseason_rows);
        pages.push(("index.html".into(), "landing.html", landing));
    }
    if !season.preseason.is_empty() {
        let week0 = context::preseason(&preseason_rows);
        pages.push((week_url(0), "week0.html", week0));
    }
    for wk in season.weeks.iter().chain(&season.provisional_week_summary) {
        pages.push((
            week_url(wk.week),
            "week.html",
            context::week(season, wk, false),
        ));
    }
    if crate::pipeline::is_complete(season) {
        let recap = context::recap(season);
        pages.push((
            week_url(season.playoff_week_start),
            "season_recap.html",
            recap,
        ));
    }
    if let Some(report) = &season.roster_report {
        pages.push((
            "rosters.html".into(),
            "rosters.html",
            context::rosters(report),
        ));
    }
    if context::has_trades_page(season) {
        pages.push(("trades.html".into(), "trades.html", context::trades(season)));
    }
    if season.draft.is_some() {
        pages.push(("draft.html".into(), "draft.html", context::draft(season)));
    }
    if !opts.rivalries.is_empty() {
        let rivalries = context::rivalries(opts.rivalries);
        pages.push(("rivalries.html".into(), "rivalries.html", rivalries));
    }
    let how = context::how_it_works(season);
    pages.push(("how-it-works.html".into(), "how_it_works.html", how));

    let environment = env::environment();
    let built: Vec<String> = pages.iter().map(|(path, _, _)| path.clone()).collect();
    let common = context::common(season, &data_js, opts, &built);
    for (path, template, mut ctx) in pages {
        let depth = path.matches('/').count();
        ctx.insert("asset_prefix", minijinja::Value::from("../".repeat(depth)));
        let html = environment
            .get_template(template)?
            .render(context::merge(&common, ctx))?;
        write(&output_dir.join(&path), &html)?;
    }
    if season.roster_report.is_some() {
        write(
            &output_dir.join("rosters.json"),
            &build_rosters_json(season),
        )?;
    }
    Ok(())
}

fn write(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("cannot write {}", path.display()))
}
