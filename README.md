# sleeper-power-rankings (ffpr)

A small Python tool that pulls a Sleeper fantasy football league from the
[Sleeper API](https://docs.sleeper.com) and builds a static, mobile-first web
page: power rankings, weekly awards, a season scoring board, and a handful of
Chart.js charts. No servers, no logins, no frontend framework -- plain
HTML/CSS/JS, rebuilt from scratch on every run and hosted with GitHub Pages.

## Setup

Requires Python 3.12 and [uv](https://docs.astral.sh/uv/).

```bash
git clone https://github.com/sconnors00/sleeper-power-rankings.git
cd sleeper-power-rankings
uv sync
```

### Finding a Sleeper league ID

- Open the league in the Sleeper app or at `sleeper.com/leagues/<id>/...` --
  the number in the URL is the league ID.
- Or, via the API: `GET /user/<username>` to get your `user_id`, then
  `GET /user/<user_id>/leagues/nfl/<season>` to list your leagues for that
  season.

Set the league ID in `config.toml` (`league_id`), or override per-run with
`--league`, or set the `SLEEPER_LEAGUE_ID` environment variable (checked in
that order of precedence: `--league` > env var > `config.toml`).

## Usage

```bash
uv run ffpr build              # build site/ through the latest completed week
uv run ffpr build --through 8  # build only through week 8
uv run ffpr build --provisional  # also include the in-progress week (excluded from rankings)
uv run ffpr serve              # preview at http://localhost:8000
uv run ffpr blurb              # print a chat-ready recap of the latest week
```

Every command accepts `--league <id>` and `--season <year>` overrides.

Raw Sleeper API responses are cached under `data/<season>/raw/` (gitignored)
so completed weeks are never refetched and builds work offline once cached;
the in-progress week and league/roster/user data are always refetched. The
`data/players_nfl.json` cache (the ~5 MB player map) refreshes at most once a
day, per Sleeper's guidance.

## Tuning the power rankings

Weights and the "form" (recent performance) window live in `config.toml`:

```toml
[weights]
win_pct = 0.30
allplay_pct = 0.25
pf_norm = 0.30
form_norm = 0.15

[form]
window = 3
```

`score = Σ weight × component`, ranked descending. See the build spec
(`sleeper-power-rankings-prompt.md`) for exactly how each component is
computed.

### Pre-season rankings

Before any week completes, the landing page shows pre-season power rankings:
last season's final power-ranking order (via `previous_league_id`), carried
over by roster -- Sleeper keeps roster ids stable across league renewals, so
in a keeper league the roster is the continuous entity even when an owner
changes (those get a "new owner" badge; the reigning champ gets a trophy).
They disappear once Week 1 is in the books.

### Draft grades

`draft.html` grades the auction: each pick's surplus is its market value
(same price curve) minus the price paid, summed per team and mapped to a
letter grade by z-score. Keepers are listed but excluded -- their prices come
from keeper rules, not open bidding. League-wide biggest steals and overpays
round out the page. Requires a completed auction draft; snake drafts are
skipped.

### Roster moves

Each week page has a "Roster moves" table (right after power rankings):
every team's adds and drops that week, sourced from Sleeper's transactions
log for that week (waivers with the FAAB bid, free agent pickups, trades).
Failed transactions (an outbid claim, a vetoed trade) never happened, so
they're excluded.

### Past seasons

`ffpr build` always walks the league's `previous_league_id` chain and builds
every past season too, each into its own `site/<year>/` (fully
self-contained: own `data.js`, own `static/`), alongside the current season
at `site/` root. Every page gets a "Season" dropdown next to the week picker
for jumping between years. Past seasons build with their own draft grades
(if that season had a completed auction) but never pre-season rankings or a
provisional week -- those only make sense for the current, in-progress
season.

## Weekly flow

A GitHub Actions schedule rebuilds and republishes the site every Tuesday
morning automatically. To trigger a build manually, go to **Actions > Build
and deploy site > Run workflow** in this repo. Nothing is ever committed back
to the repo -- every run rebuilds `site/` from scratch from the Sleeper API.

## GitHub setup (one-time, manual)

This repo ships `.github/workflows/build.yml`, which installs `uv`, runs
`ffpr build`, and publishes `site/` to GitHub Pages. You need to configure
two things in the GitHub UI:

1. **Repo secret** -- **Settings > Secrets and variables > Actions > New
   repository secret**:
   - Name: `SLEEPER_LEAGUE_ID`
   - Value: `1378825622272356352` (the 2026 league)

2. **Pages source** -- **Settings > Pages** -- under "Build and deployment",
   set **Source** to **GitHub Actions** (not "Deploy from a branch"). No
   further config needed there; the workflow handles the deploy.

Once both are set, run the workflow once manually (**Actions > Build and
deploy site > Run workflow**) to publish the first version. The site will be
at `https://sconnors00.github.io/sleeper-power-rankings/` (already set as
`site_url` in `config.toml`).

Note: GitHub Actions' `schedule` cron is UTC-only (no timezone support), so
the workflow is pinned to `11:00 UTC` Tuesdays, which is 7:00am during EDT
and drifts to 6:00am during EST (the US DST boundary). Adjust the cron in
`.github/workflows/build.yml` if you want it exact year-round.

Because `site/` is fully self-contained (relative asset paths, no absolute
domain references), the same output can also be rsynced to nginx on the
homelab instance instead of using Pages, if you ever want to skip Pages
entirely.

## Development

```bash
uv run pytest        # unit tests + a build smoke test, against committed fixtures
uv run ruff check .  # lint
uv run ruff format . # format
```

`tests/fixtures/` holds real (trimmed) Sleeper API responses from one week of
a past season so `compute.py` and `build.py` can be tested without hitting
the network.
