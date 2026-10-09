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

### Pre-season rankings ("Week 0")

Pre-season power rankings -- last season's final power-ranking order (via
`previous_league_id`), carried over by roster -- always live at
`weeks/week-0.html` ("Preseason" in the week dropdown), from before Week 1
through the rest of the season, not just during the offseason. Before any
week completes it's also the site's landing page. Sleeper keeps roster ids
stable across league renewals, so in a keeper league the roster is the
continuous entity even when an owner changes (those get a "new owner"
badge; the reigning champ gets a trophy).

### Draft grades

`draft.html` grades the auction: each pick's surplus is its market value
(same price curve) minus the price paid, summed per team and mapped to a
letter grade by z-score. Keepers are listed but excluded -- their prices come
from keeper rules, not open bidding. League-wide biggest steals and overpays
round out the page. Requires a completed auction draft; snake drafts are
skipped.

### Roster moves

Each week page ends with a "Roster moves" table:
every team's adds and drops that week, sourced from Sleeper's transactions
log for that week (waivers with the FAAB bid, free agent pickups, trades).
Failed transactions (an outbid claim, a vetoed trade) never happened, so
they're excluded.

### Rosters and FAAB

`rosters.html` ("Rosters" in the nav) lists every team's lineup (with slot
names), bench, IR and taxi squad, FAAB remaining and spent, and the waiver
bids each team won. The same data is served as `rosters.json` next to it
(`site/rosters.json`, and `site/<year>/rosters.json` per season) for scripts:
stable keys and ordering, no timestamps.

```bash
curl -s https://sconnors00.github.io/sleeper-power-rankings/rosters.json \
  | jq '.teams[] | {team_name, faab_remaining: .faab.remaining}'
```

FAAB remaining is `waiver_budget` minus each roster's `waiver_budget_used`, so
it includes FAAB traded between teams. It's 0 for leagues without a FAAB
budget.

### Trade calculator and analyzer

`trades.html` ("Trades" in the nav) has:

- **Trade calculator** -- pick two teams, tap the players each sends, and
  get a verdict (fair / slight edge / favors / lopsided), a lineup-fit line,
  each team's starting lineup before and after played out over every week
  left (byes, injuries, the drop or waiver pickup the deal forces, playoff
  weeks weighted by that team's playoff odds), the healthy-lineup swaps,
  players that would even a one-sided deal, and a shareable link
  (`trades.html#trade=<roster>:<players>/<roster>:<players>`). It runs in the
  browser from `data.js`; the lineup and verdict logic mirror
  `compute.best_lineup` and `compute.trade_verdict`.
- **This season's trades** -- each trade graded on points scored since
  (players' starts for their new team) and on the value of what each side
  received, going forward.
- **Projected lineups** -- every team's best projected lineup, ranked
  overall and position by position, to spot who needs what.
- **Player values** -- every rostered player, by position.

A player's value is his projected points per week above a replacement-level
starter at his position. The projection blends his scoring this season with
what his Sleeper `search_rank` implies at his position (a fit over this
league's own results), counting the ranking as `PRIOR_GAMES` (5) games of
evidence. Replacement level comes from filling every team's starting slots
(`roster_positions`, flex and superflex included) from the league-wide pool.
That surplus is scaled by the share of the weeks left, through the fantasy
championship, that he's expected to play: byes come from Sleeper's NFL
schedule (`api.sleeper.com/schedule/nfl/regular/<season>`, best-effort and
cached), and `injury_status` costs him next week (Out, or a discount for
Questionable/Doubtful) or the next four (IR, PUP). Playoff weeks count by
the share of the league that makes the playoffs.
Everything is Sleeper data only: no outside rankings or projections.
Draft picks aren't valued.
The calculator appears for the current season once a week is complete; past
seasons keep a history-only Trades page.

### Season recap ("Week N+1")

Once the regular season is fully built, a recap page appears one week past
the last regular-season week (`weeks/week-<playoff_week_start>.html`,
"Season recap" in the dropdown -- week 15 for a 14-week regular season):
highest/lowest scoring team, highest/lowest scoring active (starting) player,
highest bench player, and the season's closest game and biggest blowout.
Regular season only.

### Past seasons

`ffpr build` always walks the league's `previous_league_id` chain and builds
every past season too, each into its own `site/<year>/` (fully
self-contained: own `data.js`, own `static/`), alongside the current season
at `site/` root. Every page gets a "Season" dropdown in the header
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

### Changing the look

- **Colors and spacing** are tokens at the top of `static/style.css` (light
  theme, then dark). The charts in `static/app.js` read the same tokens, so a
  palette change is made in one place. Visitors can switch between light,
  dark and their device's setting with the button in the header.
- **Navigation** is the `nav` list at the top of `templates/base.html`: one
  line per page (key, file, label, whether it's shown). A page highlights its
  own entry with `{% set active_page = "<key>" %}`.
- **Shared pieces** (team names with avatars, rank badges, award tiles,
  movement arrows, percentage bars, jump links, the week pager) are macros in
  `templates/_macros.html`. Templates import them with
  `{% import "_macros.html" as ui with context %}`.
- Every top-level `<section>` in a page renders as a card; wrap two in
  `<div class="card-row">` to put them side by side on wide screens. Add
  `class="num"` to a numeric column's `<th>` and `<td>`s to right-align it.
