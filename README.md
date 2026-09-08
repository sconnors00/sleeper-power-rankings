# sleeper-power-rankings (ffpr)

A small Python tool that pulls a Sleeper fantasy football league from the
[Sleeper API](https://docs.sleeper.com) and builds a static, mobile-first web
page: power rankings, weekly awards, a season scoring board, and a handful of
Chart.js charts. No servers, no logins, no frontend framework -- plain
HTML/CSS/JS, rebuilt from scratch on every run and hosted with GitLab Pages.

## Setup

Requires Python 3.12 and [uv](https://docs.astral.sh/uv/).

```bash
git clone https://git.scone.us/homelab/sleeper-power-rankings.git
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

## Weekly flow

A GitLab CI/CD pipeline schedule rebuilds and republishes the site every
Tuesday morning automatically. To trigger a build manually, go to **Build >
Pipelines** in this project and click **Run pipeline** on the default branch.
Nothing is ever committed back to the repo -- every run rebuilds `site/` from
scratch from the Sleeper API.

## GitLab setup (one-time, manual)

This repo ships `.gitlab-ci.yml` with a `build-and-publish` job that installs
`uv`, runs `ffpr build`, and publishes `site/` as the Pages content. You need
to configure three things in the GitLab UI:

1. **CI/CD variable** -- **Settings > CI/CD > Variables** -- add:
   - Key: `SLEEPER_LEAGUE_ID`
   - Value: `1378825622272356352` (the 2026 league)
   - Type: Variable, not masked/protected unless your default branch is
     protected (in which case mark it protected too so it's available to
     pipelines on that branch)

2. **Pipeline schedule** -- **Build > Pipeline schedules > New schedule**:
   - Description: `Weekly rebuild`
   - Interval pattern (cron): `0 7 * * 2` (Tuesday 7:00)
   - Timezone: `America/New_York`
   - Target branch: your default branch

3. **GitLab Pages** -- enable Pages for this project (**Deploy > Pages** or
   your instance's equivalent) and make sure the Pages hostname is exposed to
   the internet if you want your league mates to reach it from their phones.
   Once enabled, note the URL GitLab gives you and set `site_url` in
   `config.toml` to it (used only for the link `ffpr blurb` prints).

If this GitLab instance is too old to support the `pages: true` / `publish:`
job keys (needs GitLab >= 17.1), swap to the commented-out fallback `pages:`
job at the bottom of `.gitlab-ci.yml`, which copies `site/` to `public/`
instead.

Because `site/` is fully self-contained (relative asset paths, no absolute
domain references), the same `public/` (or `site/`) output can also be
rsynced to nginx on the homelab instance instead of using Pages, if you ever
want to skip Pages entirely.

## Development

```bash
uv run pytest        # unit tests + a build smoke test, against committed fixtures
uv run ruff check .  # lint
uv run ruff format . # format
```

`tests/fixtures/` holds real (trimmed) Sleeper API responses from one week of
a past season so `compute.py` and `build.py` can be tested without hitting
the network.
