# Sleeper power rankings site — build spec

I want a small, well-structured Python tool that pulls my Sleeper fantasy football league from the Sleeper API and builds a simple static web page my league mates can open on their phones. Each week it shows power rankings, weekly awards (best/worst team score, best bench player, best/worst starter, etc.), a season-long scoring board, and a handful of charts. No servers, no logins, no frontend framework: plain HTML/CSS/JS with Chart.js, hosted with GitLab Pages on my self-hosted GitLab instance, rebuilt automatically every Tuesday morning by a scheduled pipeline.

## League

- Sleeper league ID: `<LEAGUE_ID>` (2026 season). 12-team, PPR, superflex, 3 WR. Read roster slots from the league object's `roster_positions` — don't hardcode them.
- For development and testing, use last season's league (follow `previous_league_id` on the league object). It has a full regular season of real data. Don't hardcode either ID; read from the `SLEEPER_LEAGUE_ID` env var or `config.toml`.

## Sleeper API

Read-only, no auth. Base URL `https://api.sleeper.app/v1`. Docs: https://docs.sleeper.com — read them, then fetch one real week of matchups from last season and print the shape before writing any parsing code. The docs' matchup example omits `starters_points` and `players_points`, but real responses include them; verify this.

Endpoints:
- `GET /state/nfl` → `season`, `week`, `display_week`, `season_type`
- `GET /league/{id}` → `name`, `season`, `settings` (incl. `playoff_week_start`, `league_average_match`), `roster_positions`, `scoring_settings`, `previous_league_id`
- `GET /league/{id}/rosters` → `roster_id`, `owner_id`, `co_owners`, `players`, `starters`, `reserve`, `taxi`, `settings.{wins,losses,ties,fpts,fpts_decimal,fpts_against,fpts_against_decimal}`
- `GET /league/{id}/users` → `user_id`, `display_name`, `avatar`, `metadata.team_name`
- `GET /league/{id}/matchups/{week}` → per roster: `roster_id`, `matchup_id`, `points`, `custom_points`, `starters`, `starters_points`, `players`, `players_points`
- `GET /players/nfl` → ~5 MB map of `player_id → {full_name, position, team, ...}`. Cache to disk and refresh at most once per day (Sleeper asks for this). Team defenses use the team abbreviation as the ID (e.g. `"PHI"`).

Rate limit is 1000 calls/min; a full rebuild should need about 20 calls.

## Architecture: stateless rebuild

Every run fetches weeks 1..N and rebuilds the whole site from scratch. No database and no rankings-history file to maintain: movement arrows come from recomputing the rankings for every week in one pass. Cache raw API responses under `data/<season>/raw/` so completed weeks are never refetched and offline builds work; the in-progress week is always refetched.

## What to compute

Weekly, for week N:
1. **Team scores** — `custom_points` if not null, else `points`. Highest and lowest scoring team of the week.
2. **Best bench player** — bench = `players` minus `starters` minus `reserve` minus `taxi`; score from `players_points`. Also total bench points per team ("points left on bench").
3. **Best and worst starter** — across every team's `starters` / `starters_points`. Skip empty slots (`"0"`). If the worst starter scored 0, annotate it (bye/inactive) rather than hiding it.
4. **Matchup results** — pair by `matchup_id`: winner, margin, closest game, biggest blowout.
5. **All-play record** for the week — how many of the other 11 teams each team outscored.

Season, weeks 1..N (regular season only — stop at `playoff_week_start`):
6. **Season scoring board**
   - Log: Week | Top scorer | Score
   - Crown count: how many weeks each team has been the top scorer (ties count for everyone tied)
   - Cumulative PF leaderboard: Team | PF | Avg | Best week | Worst week
   - Season records so far: highest team score, lowest team score, highest starter, highest bench player (team, player, week)
7. **Power rankings** — below.

## Power rankings

Explainable and configurable. Weights live in `config.toml`; defaults:

- `win_pct` — official record (ties = 0.5) — weight 0.30
- `allplay_pct` — cumulative all-play record — weight 0.25
- `pf_norm` — season points-for, min-max scaled 0–1 across the league — weight 0.30
- `form_norm` — average of the last 3 weeks, min-max scaled 0–1 (fewer weeks early in the season) — weight 0.15

`score = Σ weight × component`; rank descending. Compute rankings for each week 1..N in order so every week has movement vs. the week before (▲2 / ▼1 / —). Add a `luck` column = `win_pct − allplay_pct`. Week 1 has no movement arrows.

Use the official record from `rosters.settings` for display; derive all-play and head-to-head from matchups.

## Site

Static output in `site/`. Mobile-first (people will open it from the league chat on a phone), dark mode via `prefers-color-scheme`, one CSS file, one JS file, Chart.js 4 pinned from a CDN with an SRI hash. No build tooling — no npm, no React. Python renders the HTML and tables with Jinja2; JS only draws charts from `site/data.js` (`window.FFPR = {...}`), so the site also works when opened straight from disk. Use relative links and asset paths everywhere — the site will be served under a subpath like `/sean/ffpr/`, not at a domain root.

Pages:
- `index.html` — the latest completed week: awards as big cards, power rankings table (with Δ and luck), rank trajectory chart, this week's scores chart, results list. A week dropdown links to older weeks.
- `weeks/week-N.html` — same layout for every completed week.
- `season.html` — season scoring board (log, crown count, PF leaderboard, records) plus the season charts.

Team identity: name = `metadata.team_name` if set, else `display_name`; avatar from `https://sleepercdn.com/avatars/thumbs/<avatar>` when present. Assign each roster a fixed color and use it consistently in every chart and table.

## Charts

Build exactly these; don't add charts that just restate a 12-row table.

Week pages:
1. **Rank trajectory** — line chart, x = week, y = power rank (axis reversed so 1 is on top), one line per team, current week's ranks labeled. This is the signature chart.
2. **This week's scores** — horizontal bars, all 12 teams sorted high→low, colored by team; tooltip shows opponent and margin.

Season page:
3. **Weekly points by team** — line chart of every team's score by week, with the league average as a dashed grey line.
4. **Weekly scoring spread** — floating bars per week from the lowest to the highest team score, with a marker at the median; tooltip names the top and bottom scorer. This is the visual version of the season scoring board.
5. **Luck** — scatter, x = all-play win %, y = actual win %, with a diagonal reference line. Above the line = lucky. Tooltip shows team and record.
6. **Points for vs. points against** — grouped horizontal bars, sorted by PF.
7. **Points left on bench** — horizontal bars, season-to-date bench points per team.

Chart behavior: responsive with fixed-height containers, touch-friendly tooltips, tap a legend entry to isolate that team and tap again to show all. Keep legends compact on narrow screens.

## Layout

```
ffpr/
  cli.py        # typer commands
  sleeper.py    # API client + raw response cache
  models.py     # Team, Matchup, PlayerScore, WeekSummary, SeasonSummary
  compute.py    # awards, all-play, season board, power rankings (pure functions)
  build.py      # renders site/ from SeasonSummary via Jinja2, writes data.js
templates/      # base.html, week.html, season.html
static/         # style.css, app.js — copied into site/
tests/fixtures/ # real API responses from one week of last season (committed)
config.toml     # league id (optional), weights, form window, site URL
data/<season>/raw/   # cached API responses (gitignored)
site/                # build output (gitignored)
.gitlab-ci.yml
```

## CLI

- `ffpr build [--through N] [--provisional]` — fetch (using the cache), compute weeks 1..N, write `site/`. Default N = most recently completed week: start at `state.week` and walk back until every roster has `points > 0`. `--provisional` also includes the in-progress week, clearly labeled, and keeps it out of the rankings.
- `ffpr serve` — local preview at http://localhost:8000.
- `ffpr blurb [--week N]` — print a short chat-ready summary (awards, top 3, link to the site) for pasting into the league chat.
- `--league` / `--season` overrides on every command.

## Hosting

GitLab Pages on my self-hosted GitLab (Linux package install, Docker runner). Write `.gitlab-ci.yml` with one job: install with `uv`, run `ffpr build`, and publish `site/` as the Pages content (`pages: true` + `publish: site`; if my instance turns out to be too old for that, name the job `pages` and copy `site/` to `public/` instead). Rules: run on scheduled pipelines, manual web runs, and pushes to the default branch. Nothing is committed back. I'll create the schedule myself in Build > Pipeline schedules (Tuesday 7:00, America/New_York) and set `SLEEPER_LEAGUE_ID` as a CI/CD variable — give me the exact values. Enabling Pages on the instance and exposing the Pages hostname to the internet is my job, not yours. The site is public, which is fine — the same data is visible in the Sleeper app. Keep `site/` fully self-contained so the job can rsync it to nginx on my homelab instead if I ever want to skip Pages.

## Edge cases

- Empty starter slots (`"0"`); players missing from `players_points` → 0; `custom_points` null → use `points`
- Ties for any award → list everyone tied
- Orphaned roster (`owner_id` null) → "Team <roster_id>"; co-owners → primary owner's name
- `league_average_match` on → the official record already includes median games; don't double count
- Playoff weeks: week pages still build, but they don't feed power rankings or the season board
- No completed week yet (preseason): build a landing page with the league name and "Week 1 rankings land Tuesday morning"
- Two decimal places everywhere

## Engineering

- Python 3.12, `uv`, `httpx`, `typer`, `jinja2`, dataclasses (or pydantic), `ruff`, type hints throughout
- `compute.py` is pure functions; pytest it against the committed fixtures. Add a smoke test that `build.py` produces every page and `data.js` from the fixtures.
- README: setup, how to find a league ID (`sleeper.com/leagues/<id>/...` in the browser, or `GET /user/<username>` for your `user_id` then `GET /user/<user_id>/leagues/nfl/<season>`), the weekly flow (automatic, plus how to trigger manually), tuning weights, the GitLab setup (schedule, CI variable, Pages)
- No secrets needed. No web framework, no database, no npm. No extra dependencies without asking me first.

## Process

1. Read the docs, hit `/state/nfl`, the league, rosters, users, and one week of matchups from last season. Show me the shapes.
2. Propose the file layout, the ranking formula, and a rough sketch of each page's sections. Wait for my OK before writing code.
3. Implement, then build all of last season and run `ffpr serve` so I can open it on my phone.
4. Add `.gitlab-ci.yml`, tell me exactly what to configure in GitLab (schedule, CI variable, Pages), and switch config to the 2026 league.

Not now, but leave hooks for later: per-team pages, optimal-lineup / "points lost vs. optimal" (superflex-aware), posting to Discord or GroupMe, trade and waiver tracking.
