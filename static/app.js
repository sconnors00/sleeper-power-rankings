(function () {
  "use strict";
  var DATA = window.FFPR;
  if (!DATA || typeof Chart === "undefined") return;

  function isDark() {
    var stamped = document.documentElement.getAttribute("data-theme");
    if (stamped === "dark") return true;
    if (stamped === "light") return false;
    return window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
  }

  var ink = isDark() ? "#ffffff" : "#0b0b0b";
  var mutedInk = "#898781";
  var gridline = isDark() ? "#2c2c2a" : "#e1e0d9";
  var surface = isDark() ? "#1a1a19" : "#fcfcfb";

  Chart.defaults.color = ink;
  Chart.defaults.borderColor = gridline;
  Chart.defaults.font.family = "system-ui, -apple-system, 'Segoe UI', sans-serif";

  function teamColor(rosterId) {
    var t = DATA.teams[String(rosterId)];
    return t ? t.color : mutedInk;
  }
  function teamName(rosterId) {
    var t = DATA.teams[String(rosterId)];
    return t ? t.name : "Team " + rosterId;
  }

  // Tap-to-isolate legend: tap a team, others fade; tap again to restore.
  function isolateLegend(chart, legendItem, isolatedIndexRef) {
    var idx = legendItem.datasetIndex;
    if (isolatedIndexRef.value === idx) {
      chart.data.datasets.forEach(function (ds, i) {
        chart.setDatasetVisibility(i, true);
      });
      isolatedIndexRef.value = null;
    } else {
      chart.data.datasets.forEach(function (ds, i) {
        chart.setDatasetVisibility(i, i === idx);
      });
      isolatedIndexRef.value = idx;
    }
    chart.update();
  }

  function legendIsolatePlugin() {
    var isolated = { value: null };
    return {
      onClick: function (e, legendItem, legend) {
        isolateLegend(legend.chart, legendItem, isolated);
      },
    };
  }

  var baseFont = { size: 11 };

  function renderRankTrajectory() {
    var canvas = document.getElementById("chart-rank-trajectory");
    if (!canvas) return;
    var throughWeek = parseInt(canvas.getAttribute("data-through-week"), 10);
    var rt = DATA.rankTrajectory;
    var weeks = rt.weeks.filter(function (w) {
      return w <= throughWeek;
    });
    var n = weeks.length;

    var datasets = Object.keys(rt.series).map(function (rid) {
      var full = rt.series[rid];
      var data = full.slice(0, n);
      return {
        label: teamName(rid),
        data: data,
        borderColor: teamColor(rid),
        backgroundColor: teamColor(rid),
        pointRadius: 3,
        pointHoverRadius: 6,
        borderWidth: 2,
        tension: 0,
        spanGaps: true,
      };
    });

    var numTeams = Object.keys(rt.series).length;

    new Chart(canvas, {
      type: "line",
      data: { labels: weeks.map(function (w) { return "Wk " + w; }), datasets: datasets },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        interaction: { mode: "nearest", intersect: false },
        scales: {
          y: {
            reverse: true,
            min: 1,
            max: numTeams,
            ticks: { stepSize: 1, font: baseFont },
            grid: { color: gridline },
          },
          x: { ticks: { font: baseFont }, grid: { display: false } },
        },
        plugins: {
          legend: {
            position: "bottom",
            labels: { boxWidth: 10, font: { size: 10 } },
            onClick: legendIsolatePlugin().onClick,
          },
          tooltip: {
            callbacks: {
              label: function (ctx) {
                return ctx.dataset.label + ": rank " + ctx.parsed.y;
              },
            },
          },
        },
      },
    });
  }

  function renderWeekScores() {
    var canvas = document.getElementById("chart-week-scores");
    if (!canvas) return;
    var week = canvas.getAttribute("data-week");
    var rows = (DATA.weekMatchups[week] || []).slice();
    rows.sort(function (a, b) {
      return b.points - a.points;
    });

    new Chart(canvas, {
      type: "bar",
      data: {
        labels: rows.map(function (r) { return teamName(r.rosterId); }),
        datasets: [
          {
            data: rows.map(function (r) { return r.points; }),
            backgroundColor: rows.map(function (r) { return teamColor(r.rosterId); }),
            borderRadius: 4,
          },
        ],
      },
      options: {
        indexAxis: "y",
        responsive: true,
        maintainAspectRatio: false,
        plugins: {
          legend: { display: false },
          tooltip: {
            callbacks: {
              label: function (ctx) {
                var r = rows[ctx.dataIndex];
                var opp = r.opponentRosterId != null ? teamName(r.opponentRosterId) : "bye";
                var margin = r.margin != null ? (r.margin >= 0 ? "+" : "") + r.margin.toFixed(2) : "";
                return r.points.toFixed(2) + " vs " + opp + " (" + margin + ")";
              },
            },
          },
        },
        scales: {
          x: { grid: { color: gridline } },
          y: { grid: { display: false } },
        },
      },
    });
  }

  function renderWeeklyPointsByTeam() {
    var canvas = document.getElementById("chart-weekly-points");
    if (!canvas) return;
    var wp = DATA.weeklyPointsByTeam;
    var datasets = Object.keys(wp.series).map(function (rid) {
      return {
        label: teamName(rid),
        data: wp.series[rid],
        borderColor: teamColor(rid),
        backgroundColor: teamColor(rid),
        pointRadius: 2,
        pointHoverRadius: 5,
        borderWidth: 2,
        tension: 0.15,
        spanGaps: true,
      };
    });
    datasets.push({
      label: "League average",
      data: wp.leagueAvg,
      borderColor: mutedInk,
      borderDash: [6, 4],
      borderWidth: 2,
      pointRadius: 0,
      tension: 0.15,
    });

    new Chart(canvas, {
      type: "line",
      data: { labels: wp.weeks.map(function (w) { return "Wk " + w; }), datasets: datasets },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        interaction: { mode: "nearest", intersect: false },
        plugins: {
          legend: { position: "bottom", labels: { boxWidth: 10, font: { size: 10 } }, onClick: legendIsolatePlugin().onClick },
        },
        scales: {
          x: { ticks: { font: baseFont }, grid: { display: false } },
          y: { grid: { color: gridline } },
        },
      },
    });
  }

  function renderScoringSpread() {
    var canvas = document.getElementById("chart-scoring-spread");
    if (!canvas) return;
    var rows = DATA.scoringSpread;

    new Chart(canvas, {
      type: "bar",
      data: {
        labels: rows.map(function (r) { return "Wk " + r.week; }),
        datasets: [
          {
            label: "Range",
            data: rows.map(function (r) { return [r.low, r.high]; }),
            backgroundColor: isDark() ? "#3987e5aa" : "#2a78d6aa",
            borderRadius: 4,
          },
          {
            label: "Median",
            type: "line",
            data: rows.map(function (r) { return r.median; }),
            borderColor: mutedInk,
            borderDash: [4, 4],
            pointRadius: 3,
            pointBackgroundColor: mutedInk,
            borderWidth: 1,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: {
          legend: { display: false },
          tooltip: {
            callbacks: {
              label: function (ctx) {
                var r = rows[ctx.dataIndex];
                if (ctx.dataset.label === "Median") return "Median: " + r.median.toFixed(2);
                return (
                  "Top: " + teamName(r.highRosterId) + " " + r.high.toFixed(2) +
                  " | Low: " + teamName(r.lowRosterId) + " " + r.low.toFixed(2)
                );
              },
            },
          },
        },
        scales: {
          x: { ticks: { font: baseFont }, grid: { display: false } },
          y: { grid: { color: gridline } },
        },
      },
    });
  }

  function renderLuck() {
    var canvas = document.getElementById("chart-luck");
    if (!canvas) return;
    var rows = DATA.luck;

    var maxVal = 1;
    var scatterData = rows.map(function (r) {
      return { x: r.allplayPct, y: r.winPct, r };
    });

    new Chart(canvas, {
      type: "scatter",
      data: {
        datasets: [
          {
            label: "Teams",
            data: scatterData,
            backgroundColor: rows.map(function (r) { return teamColor(r.rosterId); }),
            pointRadius: 7,
            pointHoverRadius: 9,
          },
          {
            label: "Even luck",
            type: "line",
            data: [
              { x: 0, y: 0 },
              { x: maxVal, y: maxVal },
            ],
            borderColor: mutedInk,
            borderDash: [4, 4],
            pointRadius: 0,
            borderWidth: 1,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: {
          legend: { display: false },
          tooltip: {
            callbacks: {
              label: function (ctx) {
                if (ctx.dataset.label !== "Teams") return null;
                var r = rows[ctx.dataIndex];
                return teamName(r.rosterId) + ": " + r.record + " (all-play " + r.allplayRecord + ")";
              },
            },
          },
        },
        scales: {
          x: { title: { display: true, text: "All-play win %" }, min: 0, max: 1, grid: { color: gridline } },
          y: { title: { display: true, text: "Actual win %" }, min: 0, max: 1, grid: { color: gridline } },
        },
      },
    });
  }

  function renderPfVsPa() {
    var canvas = document.getElementById("chart-pf-pa");
    if (!canvas) return;
    var rows = DATA.pfVsPa.slice().sort(function (a, b) { return b.pf - a.pf; });

    new Chart(canvas, {
      type: "bar",
      data: {
        labels: rows.map(function (r) { return teamName(r.rosterId); }),
        datasets: [
          { label: "PF", data: rows.map(function (r) { return r.pf; }), backgroundColor: "#2a78d6", borderRadius: 4 },
          { label: "PA", data: rows.map(function (r) { return r.pa; }), backgroundColor: mutedInk, borderRadius: 4 },
        ],
      },
      options: {
        indexAxis: "y",
        responsive: true,
        maintainAspectRatio: false,
        plugins: { legend: { position: "bottom" } },
        scales: {
          x: { grid: { color: gridline } },
          y: { grid: { display: false } },
        },
      },
    });
  }

  function renderBenchPoints() {
    var canvas = document.getElementById("chart-bench");
    if (!canvas) return;
    var rows = DATA.benchPoints.slice().sort(function (a, b) { return b.points - a.points; });

    new Chart(canvas, {
      type: "bar",
      data: {
        labels: rows.map(function (r) { return teamName(r.rosterId); }),
        datasets: [
          {
            data: rows.map(function (r) { return r.points; }),
            backgroundColor: rows.map(function (r) { return teamColor(r.rosterId); }),
            borderRadius: 4,
          },
        ],
      },
      options: {
        indexAxis: "y",
        responsive: true,
        maintainAspectRatio: false,
        plugins: { legend: { display: false } },
        scales: {
          x: { grid: { color: gridline } },
          y: { grid: { display: false } },
        },
      },
    });
  }

  document.addEventListener("DOMContentLoaded", function () {
    renderRankTrajectory();
    renderWeekScores();
    renderWeeklyPointsByTeam();
    renderScoringSpread();
    renderLuck();
    renderPfVsPa();
    renderBenchPoints();
  });
})();

// Trade calculator (trades.html). Kept apart from the charts so it works even
// when the Chart.js CDN doesn't load. Player values, projections and the
// verdict bands come from data.js; the lineup and verdict logic mirror
// compute.best_lineup and compute.trade_verdict.
(function () {
  "use strict";
  var DATA = window.FFPR;
  var root = document.getElementById("trade-calculator");
  if (!DATA || !DATA.trade || !root) return;

  var T = DATA.trade;
  var PLAYERS = T.players;
  var POS_ORDER = ["QB", "RB", "WR", "TE", "K", "DEF"];
  var NON_STARTING = { BN: true, IR: true, TAXI: true };
  var SLOTS = T.rosterPositions.filter(function (s) { return !NON_STARTING[s]; });
  var FIXED = SLOTS.filter(function (s) { return !T.flex[s]; });
  var FLEXES = SLOTS.filter(function (s) { return T.flex[s]; }).map(function (s) { return T.flex[s]; });
  var STORE_KEY = "ffpr-trade-team";
  var SHARE_LABEL = "Copy link to this trade";

  var sides = [0, 1].map(function (i) {
    var el = root.querySelector('.trade-side[data-side="' + i + '"]');
    var select = el.querySelector(".trade-team");
    return { el: el, select: select, current: select.value, picked: [] };
  });
  var result = root.querySelector(".trade-result");

  function h(tag, className, children) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    (children || []).forEach(function (c) {
      if (c == null) return;
      node.appendChild(typeof c === "string" ? document.createTextNode(c) : c);
    });
    return node;
  }
  function fmt(x) {
    return (Math.abs(x) < 0.005 ? 0 : x).toFixed(2);
  }
  function signed(x) {
    return (x >= 0.005 ? "+" : "") + fmt(x);
  }
  function trend(x) {
    return Math.abs(x) < 0.005 ? "" : x > 0 ? "up" : "down";
  }
  function teamName(rid) {
    var t = DATA.teams[String(rid)];
    return t ? t.name : "Team " + rid;
  }
  function teamColor(rid) {
    var t = DATA.teams[String(rid)];
    return t ? t.color : "#898781";
  }
  function swatch(rid) {
    var s = h("span", "team-swatch");
    s.style.background = teamColor(rid);
    return s;
  }
  function total(pids, key) {
    return pids.reduce(function (acc, pid) { return acc + PLAYERS[pid][key]; }, 0);
  }
  function rosterOf(i) {
    return T.rosters[sides[i].select.value] || [];
  }
  function isActive(pid) {
    return PLAYERS[pid].active;
  }
  function describe(pid) {
    var p = PLAYERS[pid];
    return p.name + " (" + p.pos + ", " + fmt(p.proj) + ")";
  }

  // The highest-projected legal lineup from these players, as in
  // compute.best_lineup: fixed slots take the best player left at their
  // position, then flex slots are searched exhaustively. On equal points the
  // players in `keep` stay in, so a tie never reads as a lineup change.
  function bestLineup(pids, keep) {
    var remaining = pids.filter(isActive).sort(function (a, b) {
      var diff = PLAYERS[b].proj - PLAYERS[a].proj;
      return diff || (keep[b] ? 1 : 0) - (keep[a] ? 1 : 0);
    });
    var lineup = [];
    FIXED.forEach(function (slot) {
      for (var i = 0; i < remaining.length; i++) {
        if (PLAYERS[remaining[i]].pos === slot) {
          lineup.push(remaining.splice(i, 1)[0]);
          return;
        }
      }
    });
    // Only the top FLEXES.length players at a position can ever reach a flex slot.
    var taken = {};
    var candidates = remaining.filter(function (pid) {
      var pos = PLAYERS[pid].pos;
      var eligible = FLEXES.some(function (e) { return e.indexOf(pos) >= 0; });
      if (!eligible || (taken[pos] || 0) >= FLEXES.length) return false;
      taken[pos] = (taken[pos] || 0) + 1;
      return true;
    });
    var best = { points: -Infinity, kept: 0, picks: [] };
    var picked = [];
    function search(i) {
      if (i === FLEXES.length) {
        var points = Math.round(total(picked, "proj") * 100) / 100;
        var kept = picked.filter(function (pid) { return keep[pid]; }).length;
        if (points > best.points || (points === best.points && kept > best.kept)) {
          best = { points: points, kept: kept, picks: picked.slice() };
        }
        return;
      }
      var eligible = candidates.filter(function (pid) {
        return FLEXES[i].indexOf(PLAYERS[pid].pos) >= 0 && picked.indexOf(pid) < 0;
      });
      if (!eligible.length) search(i + 1);
      eligible.forEach(function (pid) {
        picked.push(pid);
        search(i + 1);
        picked.pop();
      });
    }
    search(0);
    return lineup.concat(best.picks);
  }

  // Who joins the lineup in place of whom, like compute.pair_swaps:
  // like-for-like first, then best in for worst out. [in, null] fills an
  // empty slot; [null, out] leaves one empty.
  function pairSwaps(ins, outs) {
    ins = ins.slice().sort(function (a, b) { return PLAYERS[b].proj - PLAYERS[a].proj; });
    outs = outs.slice().sort(function (a, b) { return PLAYERS[a].proj - PLAYERS[b].proj; });
    var pairs = [];
    ins.slice().forEach(function (pid) {
      for (var j = 0; j < outs.length; j++) {
        if (PLAYERS[outs[j]].pos === PLAYERS[pid].pos) {
          pairs.push([pid, outs.splice(j, 1)[0]]);
          ins.splice(ins.indexOf(pid), 1);
          return;
        }
      }
    });
    ins.forEach(function (pid) { pairs.push([pid, outs.length ? outs.shift() : null]); });
    outs.forEach(function (pid) { pairs.push([null, pid]); });
    return pairs;
  }

  // Mirrors compute.trade_verdict for two sides.
  function verdict(gets) {
    var top = gets[0] >= gets[1] ? 0 : 1;
    var hi = gets[top];
    if (hi < 0.005) return { key: "even", side: null };
    var gap = hi - gets[1 - top];
    var share = gap / hi;
    if (gap < T.fairGap || share < T.bands[0]) return { key: "fair", side: null };
    if (share < T.bands[1]) return { key: "slight edge", side: top };
    return { key: share < T.bands[2] ? "favors" : "lopsided", side: top };
  }

  function analyze() {
    var sends = [sides[0].picked, sides[1].picked];
    var gets = [total(sends[1], "value"), total(sends[0], "value")];
    var teams = [0, 1].map(function (i) {
      var before = rosterOf(i);
      var after = before.filter(function (pid) { return sends[i].indexOf(pid) < 0; }).concat(sends[1 - i]);
      var lineupBefore = bestLineup(before, {});
      var keep = {};
      lineupBefore.forEach(function (pid) { keep[pid] = true; });
      var lineupAfter = bestLineup(after, keep);
      var activeBefore = before.filter(isActive).length;
      var activeAfter = after.filter(isActive).length;
      return {
        rid: sides[i].select.value,
        gets: gets[i],
        gives: gets[1 - i],
        before: total(lineupBefore, "proj"),
        after: total(lineupAfter, "proj"),
        swaps: pairSwaps(
          lineupAfter.filter(function (pid) { return lineupBefore.indexOf(pid) < 0; }),
          lineupBefore.filter(function (pid) { return lineupAfter.indexOf(pid) < 0; })
        ),
        mustDrop: Math.max(0, activeAfter - Math.max(T.rosterLimit, activeBefore)),
      };
    });
    return { gets: gets, teams: teams, verdict: verdict(gets) };
  }

  function headline(v) {
    if (v.key === "even") return "Even";
    if (v.key === "fair") return "Fair trade";
    var name = teamName(sides[v.side].select.value);
    if (v.key === "slight edge") return "Slight edge: " + name;
    if (v.key === "favors") return "Favors " + name;
    return "Lopsided for " + name;
  }

  function teamCard(t) {
    var delta = t.after - t.before;
    var card = h("div", "award-card", [
      h("h3", null, [swatch(t.rid), " " + teamName(t.rid)]),
      h("p", "trade-line", [
        "Gets ", h("strong", null, [fmt(t.gets)]), " · gives " + fmt(t.gives) + " · net ",
        h("span", trend(t.gets - t.gives), [signed(t.gets - t.gives)]),
      ]),
      h("p", "trade-line", [
        "Best lineup " + fmt(t.before) + " → " + fmt(t.after) + " pts/wk ",
        h("strong", trend(delta), [signed(delta)]),
      ]),
    ]);
    if (T.weeksLeft > 0 && Math.abs(delta) >= 0.005) {
      card.appendChild(h("p", "note trade-line", [
        "About " + signed(delta * T.weeksLeft) + " points over the " + T.weeksLeft +
          " regular-season week" + (T.weeksLeft === 1 ? "" : "s") + " left.",
      ]));
    }
    if (t.swaps.length) {
      card.appendChild(h("ul", "trade-changes", t.swaps.map(function (pair) {
        if (pair[0] && pair[1]) return h("li", null, [describe(pair[0]) + " replaces " + describe(pair[1])]);
        if (pair[0]) return h("li", null, [describe(pair[0]) + " fills an empty slot"]);
        return h("li", "down", [describe(pair[1]) + " leaves a slot nobody can fill"]);
      })));
    } else {
      card.appendChild(h("p", "note trade-line", ["Starting lineup unchanged."]));
    }
    if (t.mustDrop) {
      card.appendChild(h("p", "trade-line down", [
        "Would have to drop " + t.mustDrop + " player" + (t.mustDrop === 1 ? "" : "s") +
          " to stay under the " + T.rosterLimit + "-player roster limit.",
      ]));
    }
    return card;
  }

  // Players from the team that's ahead that would bring the other side's
  // haul closest to even.
  function suggestions(a) {
    var s = a.verdict.side;
    if (s == null) return null;
    var gap = a.gets[s] - a.gets[1 - s];
    var picks = rosterOf(s)
      .filter(function (pid) { return PLAYERS[pid].value > 0 && sides[s].picked.indexOf(pid) < 0; })
      .sort(function (x, y) { return Math.abs(PLAYERS[x].value - gap) - Math.abs(PLAYERS[y].value - gap); })
      .slice(0, 3);
    if (!picks.length) return null;
    var p = h("p", "trade-suggest", ["To even it up, " + teamName(sides[s].select.value) + " could add: "]);
    picks.forEach(function (pid) {
      var btn = h("button", "btn", [PLAYERS[pid].name + " (" + fmt(PLAYERS[pid].value) + ")"]);
      btn.type = "button";
      btn.addEventListener("click", function () { toggle(s, pid, true); });
      p.appendChild(btn);
    });
    return p;
  }

  function renderResult() {
    result.textContent = "";
    if (!sides[0].picked.length && !sides[1].picked.length) {
      result.appendChild(h("p", "note", ["Tap players on either side to build a trade."]));
      return;
    }
    var a = analyze();
    var bar = h("div", "trade-bar");
    [0, 1].forEach(function (i) {
      var seg = h("span");
      seg.style.flexGrow = String(a.gets[i]);
      seg.style.background = teamColor(sides[i].select.value);
      bar.appendChild(seg);
    });
    var detail = a.verdict.key === "even"
      ? "Neither side gets a player projected above replacement level."
      : teamName(a.teams[0].rid) + " gets " + fmt(a.gets[0]) + " in value, " +
        teamName(a.teams[1].rid) + " gets " + fmt(a.gets[1]) + " (points per week above replacement).";
    result.appendChild(h("div", "award-card trade-verdict", [
      h("h3", null, ["Verdict"]),
      h("p", "award-value", [headline(a.verdict)]),
      h("p", "award-teams", [detail]),
      bar,
    ]));
    result.appendChild(h("div", "trade-teams", a.teams.map(teamCard)));
    var hint = suggestions(a);
    if (hint) result.appendChild(hint);

    var share = h("button", "btn", [SHARE_LABEL]);
    share.type = "button";
    share.addEventListener("click", function () { copyLink(share); });
    var clear = h("button", "btn", ["Clear"]);
    clear.type = "button";
    clear.addEventListener("click", function () {
      sides[0].picked = [];
      sides[1].picked = [];
      update();
    });
    result.appendChild(h("div", "trade-actions", [share, clear]));
  }

  function copyLink(button) {
    var url = window.location.href;
    function done() {
      button.textContent = "Link copied";
      setTimeout(function () { button.textContent = SHARE_LABEL; }, 2000);
    }
    function fallback() {
      window.prompt("Copy this link:", url);
    }
    if (navigator.clipboard && window.isSecureContext) {
      navigator.clipboard.writeText(url).then(done, fallback);
    } else {
      fallback();
    }
  }

  function renderRoster(i) {
    var box = sides[i].el.querySelector(".trade-roster");
    box.textContent = "";
    box.scrollTop = 0;
    var groups = {};
    rosterOf(i).forEach(function (pid) {
      var pos = PLAYERS[pid].pos;
      (groups[pos] = groups[pos] || []).push(pid);
    });
    var order = POS_ORDER.filter(function (p) { return groups[p]; }).concat(
      Object.keys(groups).filter(function (p) { return POS_ORDER.indexOf(p) < 0; }).sort()
    );
    order.forEach(function (pos) {
      box.appendChild(h("div", "trade-group", [pos]));
      groups[pos].forEach(function (pid) {
        var p = PLAYERS[pid];
        var input = h("input");
        input.type = "checkbox";
        input.value = pid;
        input.addEventListener("change", function () { toggle(i, pid, input.checked); });
        box.appendChild(h("label", "trade-player", [
          input,
          h("span", "trade-name", [p.name]),
          p.injury ? h("span", "inj", [p.injury]) : null,
          h("span", "trade-meta", [(p.nfl || "FA") + " · " + fmt(p.proj)]),
          h("span", "trade-val", [fmt(p.value)]),
        ]));
      });
    });
  }

  function renderPicked(i) {
    var box = sides[i].el.querySelector(".trade-picked");
    box.textContent = "";
    var picked = sides[i].picked;
    if (!picked.length) {
      box.appendChild(h("p", "note", ["Nobody yet. Tap players below."]));
      return;
    }
    picked.forEach(function (pid) {
      var chip = h("button", "trade-chip", [PLAYERS[pid].name + " " + fmt(PLAYERS[pid].value) + " ×"]);
      chip.type = "button";
      chip.setAttribute("aria-label", "Remove " + PLAYERS[pid].name);
      chip.addEventListener("click", function () { toggle(i, pid, false); });
      box.appendChild(chip);
    });
    box.appendChild(h("p", "trade-total", ["Sends " + fmt(total(picked, "value")) + " in value"]));
  }

  function syncChecks(i) {
    var inputs = sides[i].el.querySelectorAll(".trade-roster input");
    for (var j = 0; j < inputs.length; j++) {
      inputs[j].checked = sides[i].picked.indexOf(inputs[j].value) >= 0;
    }
  }

  function writeHash() {
    var url = window.location.pathname + window.location.search;
    if (sides[0].picked.length || sides[1].picked.length) {
      url += "#trade=" + sides.map(function (s) {
        return s.select.value + ":" + s.picked.join(",");
      }).join("/");
    }
    try {
      history.replaceState(null, "", url);
    } catch (e) {
      // some browsers refuse history updates on file:// pages; the calculator still works
    }
  }

  // "#trade=3:4046,6011/7:9509" -> teams and the players each sends.
  function readHash() {
    var m = /^#trade=(.+)$/.exec(window.location.hash);
    if (!m) return false;
    var parts;
    try {
      parts = decodeURIComponent(m[1]).split("/");
    } catch (e) {
      return false; // a mangled link: start fresh rather than fail
    }
    if (parts.length !== 2) return false;
    var parsed = parts.map(function (part) {
      var bits = part.split(":");
      var roster = T.rosters[bits[0]];
      if (!roster) return null;
      var pids = (bits[1] || "").split(",").filter(function (pid, j, all) {
        return roster.indexOf(pid) >= 0 && all.indexOf(pid) === j;
      });
      return { rid: bits[0], pids: pids };
    });
    if (!parsed[0] || !parsed[1] || parsed[0].rid === parsed[1].rid) return false;
    parsed.forEach(function (p, i) {
      sides[i].select.value = p.rid;
      sides[i].current = p.rid;
      sides[i].picked = p.pids;
    });
    return true;
  }

  function toggle(i, pid, on) {
    var picked = sides[i].picked;
    var at = picked.indexOf(pid);
    if (on && at < 0) picked.push(pid);
    if (!on && at >= 0) picked.splice(at, 1);
    update();
  }

  function update() {
    [0, 1].forEach(function (i) {
      renderPicked(i);
      syncChecks(i);
    });
    renderResult();
    writeHash();
  }

  function setTeam(i, rid) {
    var other = sides[1 - i];
    if (other.select.value === rid) {
      // Picking the other side's team swaps the two.
      other.select.value = sides[i].current;
      other.current = other.select.value;
      other.picked = [];
    }
    sides[i].select.value = rid;
    sides[i].current = rid;
    sides[i].picked = [];
    renderRoster(0);
    renderRoster(1);
    if (i === 0) {
      try {
        window.localStorage.setItem(STORE_KEY, rid);
      } catch (e) {
        // private browsing or blocked storage: just don't remember
      }
    }
    update();
  }

  sides.forEach(function (side, i) {
    side.select.addEventListener("change", function () { setTeam(i, side.select.value); });
  });

  document.addEventListener("DOMContentLoaded", function () {
    if (!readHash()) {
      var saved = null;
      try {
        saved = window.localStorage.getItem(STORE_KEY);
      } catch (e) {
        saved = null;
      }
      if (saved && T.rosters[saved] && saved !== sides[0].select.value) {
        setTeam(0, saved);
        return;
      }
    }
    renderRoster(0);
    renderRoster(1);
    update();
  });
})();
