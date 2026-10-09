// Shared helpers. Team colors follow the theme: each team has a light and a
// dark palette color (data.js `color` / `colorDark`), like the team marks in
// style.css.
var FFPR_UTIL = (function () {
  "use strict";
  function team(rid) {
    var data = window.FFPR;
    return data && data.teams ? data.teams[String(rid)] : null;
  }
  function isDark() {
    return getComputedStyle(document.documentElement).colorScheme === "dark";
  }
  return {
    isDark: isDark,
    teamName: function (rid) {
      var t = team(rid);
      return t ? t.name : "Team " + rid;
    },
    teamColor: function (rid, fallback) {
      var t = team(rid);
      if (!t) return fallback || "#808080";
      return isDark() && t.colorDark ? t.colorDark : t.color;
    },
  };
})();

// Header. The theme button cycles auto (follow the OS) -> light -> dark; the
// choice is saved and stamped on <html> as data-theme, which style.css keys
// its tokens off, and an inline script in base.html applies it before first
// paint. The week and season pickers navigate on a mouse or touch pick, but
// from the keyboard only on Enter, so arrowing through the options doesn't
// leave the page. On a narrow screen the nav scrolls to the current page.
(function () {
  "use strict";
  var KEY = "ffpr-theme";
  var MODES = ["auto", "light", "dark"];
  var LABELS = { auto: "Theme: match device", light: "Theme: light", dark: "Theme: dark" };
  var root = document.documentElement;

  function current() {
    var stamped = root.getAttribute("data-theme");
    return stamped === "light" || stamped === "dark" ? stamped : "auto";
  }
  function show(button, mode) {
    button.setAttribute("data-mode", mode);
    button.setAttribute("aria-label", LABELS[mode]);
    button.title = LABELS[mode];
  }

  document.addEventListener("DOMContentLoaded", function () {
    var nav = document.querySelector(".site-nav");
    var here = nav && nav.querySelector('[aria-current="page"]');
    if (here && nav.scrollWidth > nav.clientWidth) {
      nav.scrollLeft = here.offsetLeft - nav.offsetLeft - (nav.clientWidth - here.offsetWidth) / 3;
    }

    Array.prototype.forEach.call(document.querySelectorAll("select[data-nav]"), function (select) {
      var keyed = false;
      select.hidden = false;
      select.addEventListener("keydown", function (e) {
        if (e.key === "Enter") {
          if (select.value) window.location.href = select.value;
        } else {
          keyed = /^(Arrow|Page|Home|End)/.test(e.key) || e.key.length === 1;
        }
      });
      select.addEventListener("change", function () {
        if (!keyed && select.value) window.location.href = select.value;
        keyed = false;
      });
    });

    var button = document.querySelector(".theme-toggle");
    if (!button) return;
    show(button, current());
    button.hidden = false;
    button.addEventListener("click", function () {
      var mode = MODES[(MODES.indexOf(current()) + 1) % MODES.length];
      if (mode === "auto") root.removeAttribute("data-theme");
      else root.setAttribute("data-theme", mode);
      try {
        if (mode === "auto") window.localStorage.removeItem(KEY);
        else window.localStorage.setItem(KEY, mode);
      } catch (e) {
        // blocked storage: the choice just lasts for this page
      }
      show(button, mode);
      document.dispatchEvent(new CustomEvent("ffpr:theme"));
    });
  });
})();

// Charts (week and season pages). Colors come from the CSS tokens in
// style.css, so they follow the theme; every chart is redrawn when it changes.
// Each renderer's key is a canvas id from the ui.chart macro in _macros.html.
(function () {
  "use strict";
  var DATA = window.FFPR;
  if (!DATA || typeof Chart === "undefined") {
    // style.css hides the empty chart cards
    document.documentElement.classList.add("no-charts");
    return;
  }
  var teamColor = FFPR_UTIL.teamColor;
  var teamName = FFPR_UTIL.teamName;
  var PICK_KEY = "ffpr-trade-team"; // the team you last looked at in the trade calculator

  var C = {};
  function readTokens() {
    var css = getComputedStyle(document.documentElement);
    function token(name) { return css.getPropertyValue(name).trim(); }
    C = {
      ink: token("--text-primary"),
      secondary: token("--text-secondary"),
      muted: token("--text-muted"),
      gridline: token("--gridline"),
      surface: token("--surface"),
      border: token("--border"),
      accent: token("--accent"),
      bad: token("--bad"),
      font: token("--font"),
    };
    Chart.defaults.color = C.secondary;
    Chart.defaults.borderColor = C.gridline;
    Chart.defaults.font.family = C.font;
    Chart.defaults.font.size = 11;
    if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      Chart.defaults.animation = false;
    }
    var tip = Chart.defaults.plugins.tooltip;
    tip.backgroundColor = C.surface;
    tip.titleColor = C.ink;
    tip.bodyColor = C.secondary;
    tip.borderColor = C.border;
    tip.borderWidth = 1;
    tip.padding = 10;
    tip.cornerRadius = 8;
    tip.boxPadding = 4;
    tip.usePointStyle = true;
    var legend = Chart.defaults.plugins.legend.labels;
    legend.usePointStyle = true;
    legend.pointStyle = "circle";
    legend.boxWidth = 8;
    legend.boxHeight = 8;
    legend.padding = 12;
  }

  // "#2a78d6" at the given opacity.
  function alpha(hex, a) {
    var n = parseInt(hex.slice(1), 16);
    return "rgba(" + (n >> 16) + "," + ((n >> 8) & 255) + "," + (n & 255) + "," + a + ")";
  }
  function signed(x, digits) {
    return (x > 0 ? "+" : "") + x.toFixed(digits);
  }

  // Line charts with a line per team: hovering near a team's line picks it
  // out and fades the rest; leaving the chart goes back to the chart's
  // chosen team ($picked), if it has one. Team lines read these through
  // scriptable colors (see teamLine), which Chart.js re-resolves on update.
  function focused(chart) {
    return chart.$highlighted != null ? chart.$highlighted : chart.$picked;
  }
  function highlightOnHover(event, elements, chart) {
    var idx = elements.length ? elements[0].datasetIndex : null;
    if (idx !== null && !chart.data.datasets[idx].$team) idx = null;
    if (chart.$highlighted === idx) return;
    chart.$highlighted = idx;
    chart.update("none");
  }
  var clearHighlightOnLeave = {
    id: "clearHighlightOnLeave",
    afterEvent: function (chart, args) {
      if (args.event.type === "mouseout") highlightOnHover(null, [], chart);
    },
  };

  // One team's line. With `context`, lines other than the focused one recede
  // to thin gray instead of a faded team color.
  function teamLine(rid, data, context) {
    var color = teamColor(rid);
    var faded = context ? alpha(C.muted, 0.3) : alpha(color, 0.15);
    function on(ctx) {
      var f = focused(ctx.chart);
      return f == null || f === ctx.datasetIndex;
    }
    function shade(ctx) { return on(ctx) ? color : faded; }
    return {
      label: teamName(rid),
      data: data,
      $team: true,
      borderColor: shade,
      backgroundColor: shade,
      pointBackgroundColor: shade,
      pointBorderColor: shade,
      borderWidth: function (ctx) { return focused(ctx.chart) === ctx.datasetIndex ? 3.5 : 2; },
      pointRadius: function (ctx) { return context && !on(ctx) ? 0 : 2.5; },
      pointHoverRadius: 6,
      spanGaps: true,
    };
  }

  // Each team's name at the end of its line, so the lines need no legend.
  var endLabels = {
    id: "endLabels",
    afterDatasetsDraw: function (chart) {
      var c = chart.ctx;
      var narrow = chart.width < 480;
      c.save();
      c.font = "500 11px " + C.font;
      c.textBaseline = "middle";
      chart.data.datasets.forEach(function (ds, i) {
        var meta = chart.getDatasetMeta(i);
        if (!ds.$team || meta.hidden || !meta.data.length) return;
        var last = meta.data[meta.data.length - 1];
        var f = focused(chart);
        c.globalAlpha = f == null || f === i ? 1 : 0.35;
        c.fillStyle = C.secondary;
        var text = narrow && ds.label.length > 11 ? ds.label.slice(0, 10) + "…" : ds.label;
        c.fillText(text, last.x + 8, last.y);
      });
      c.restore();
    },
  };

  // The value at the end of each horizontal bar.
  var barValues = {
    id: "barValues",
    afterDatasetsDraw: function (chart) {
      var c = chart.ctx;
      c.save();
      c.font = "600 11px " + C.font;
      c.fillStyle = C.secondary;
      c.textBaseline = "middle";
      chart.data.datasets.forEach(function (ds, i) {
        var meta = chart.getDatasetMeta(i);
        if (meta.hidden) return;
        meta.data.forEach(function (bar, j) {
          var v = ds.data[j];
          if (v == null) return;
          var left = v < 0;
          c.textAlign = left ? "right" : "left";
          c.fillText(ds.$format ? ds.$format(v) : v.toFixed(1), bar.x + (left ? -6 : 6), bar.y);
        });
      });
      c.restore();
    },
  };

  var xAxis = { grid: { display: false }, border: { display: false } };
  function yAxis(extra) {
    var axis = { grid: { color: C.gridline }, border: { display: false } };
    Object.keys(extra || {}).forEach(function (k) { axis[k] = extra[k]; });
    return axis;
  }
  function weekLabels(weeks) {
    return weeks.map(function (w) { return "Wk " + w; });
  }

  var renderers = {
    "chart-rank-trajectory": function (canvas) {
      var throughWeek = parseInt(canvas.getAttribute("data-through-week"), 10);
      var rt = DATA.rankTrajectory;
      var weeks = rt.weeks.filter(function (w) { return w <= throughWeek; });
      var datasets = Object.keys(rt.series).map(function (rid) {
        return teamLine(rid, rt.series[rid].slice(0, weeks.length));
      });
      var n = datasets.length;
      // room on the right for the longest end label
      var c = canvas.getContext("2d");
      c.font = "500 11px " + C.font;
      var widest = datasets.reduce(function (w, ds) { return Math.max(w, c.measureText(ds.label).width); }, 0);
      var room = Math.min(widest + 14, canvas.parentNode.clientWidth < 480 ? 76 : 180);
      return {
        type: "line",
        data: { labels: weekLabels(weeks), datasets: datasets },
        plugins: [clearHighlightOnLeave, endLabels],
        options: {
          layout: { padding: { right: room } },
          interaction: { mode: "nearest", intersect: false },
          onHover: highlightOnHover,
          scales: {
            y: yAxis({
              reverse: true,
              min: 0.5,
              max: n + 0.5,
              afterBuildTicks: function (axis) {
                axis.ticks = datasets.map(function (_, i) { return { value: i + 1 }; });
              },
            }),
            x: xAxis,
          },
          plugins: {
            legend: { display: false },
            tooltip: {
              callbacks: {
                label: function (ctx) { return ctx.dataset.label + ": #" + ctx.parsed.y; },
              },
            },
          },
        },
      };
    },

    "chart-week-scores": function (canvas) {
      var rows = (DATA.weekMatchups[canvas.getAttribute("data-week")] || []).slice();
      rows.sort(function (a, b) { return b.points - a.points; });
      return {
        type: "bar",
        data: {
          labels: rows.map(function (r) { return teamName(r.rosterId); }),
          datasets: [{
            data: rows.map(function (r) { return r.points; }),
            backgroundColor: rows.map(function (r) {
              return r.margin > 0 ? C.accent : alpha(C.muted, 0.45);
            }),
            borderRadius: 4,
            maxBarThickness: 18,
          }],
        },
        plugins: [barValues],
        options: {
          indexAxis: "y",
          layout: { padding: { right: 44 } },
          plugins: {
            legend: { display: false },
            tooltip: {
              callbacks: {
                label: function (ctx) {
                  var r = rows[ctx.dataIndex];
                  var opp = r.opponentRosterId != null ? teamName(r.opponentRosterId) : "bye";
                  var margin = r.margin != null ? " (" + signed(r.margin, 2) + ")" : "";
                  return r.points.toFixed(2) + " vs " + opp + margin;
                },
              },
            },
          },
          scales: { x: { display: false, beginAtZero: true }, y: xAxis },
        },
      };
    },

    "chart-weekly-points": function (canvas) {
      var wp = DATA.weeklyPointsByTeam;
      var ids = Object.keys(wp.series);
      var datasets = ids.map(function (rid) { return teamLine(rid, wp.series[rid], true); });
      datasets.push({
        label: "League average",
        data: wp.leagueAvg,
        borderColor: C.secondary,
        backgroundColor: C.secondary,
        borderDash: [6, 4],
        borderWidth: 1.5,
        pointRadius: 0,
      });
      return {
        type: "line",
        data: { labels: weekLabels(wp.weeks), datasets: datasets },
        plugins: [clearHighlightOnLeave],
        options: {
          interaction: { mode: "nearest", intersect: false },
          onHover: highlightOnHover,
          plugins: { legend: { display: false } },
          scales: { x: xAxis, y: yAxis() },
        },
        // after the chart exists: the "Highlight" team picker above it
        $after: function (chart) { pickTeam(chart, ids); },
      };
    },

    "chart-scoring-spread": function () {
      var rows = DATA.scoringSpread;
      return {
        type: "bar",
        data: {
          labels: rows.map(function (r) { return "Wk " + r.week; }),
          datasets: [
            {
              label: "Range",
              data: rows.map(function (r) { return [r.low, r.high]; }),
              backgroundColor: alpha(C.accent, 0.35),
              borderRadius: 4,
              maxBarThickness: 34,
            },
            {
              label: "Median",
              type: "line",
              data: rows.map(function (r) { return r.median; }),
              showLine: false,
              pointStyle: "line",
              pointRadius: 12,
              pointHoverRadius: 12,
              borderWidth: 2,
              borderColor: C.ink,
              pointBorderColor: C.ink,
            },
          ],
        },
        options: {
          plugins: {
            legend: { display: false },
            tooltip: {
              callbacks: {
                label: function (ctx) {
                  var r = rows[ctx.dataIndex];
                  if (ctx.dataset.label === "Median") return "Median: " + r.median.toFixed(2);
                  return [
                    "Top: " + teamName(r.highRosterId) + " " + r.high.toFixed(2),
                    "Low: " + teamName(r.lowRosterId) + " " + r.low.toFixed(2),
                  ];
                },
              },
            },
          },
          scales: { x: xAxis, y: yAxis({ beginAtZero: false, grace: "5%" }) },
        },
      };
    },

    // Luck as wins: how many more (or fewer) games a team won than its
    // all-play record says its scores deserved.
    "chart-luck": function () {
      var rows = DATA.luck.map(function (r) {
        return { r: r, wins: (r.winPct - r.allplayPct) * r.games };
      }).sort(function (a, b) { return b.wins - a.wins; });
      return {
        type: "bar",
        data: {
          labels: rows.map(function (x) { return teamName(x.r.rosterId); }),
          datasets: [{
            data: rows.map(function (x) { return x.wins; }),
            backgroundColor: rows.map(function (x) { return x.wins >= 0 ? C.accent : alpha(C.bad, 0.85); }),
            borderRadius: 4,
            maxBarThickness: 16,
            $format: function (v) { return signed(v, 1); },
          }],
        },
        plugins: [barValues],
        options: {
          indexAxis: "y",
          layout: { padding: { left: 30, right: 30 } },
          plugins: {
            legend: { display: false },
            tooltip: {
              callbacks: {
                label: function (ctx) {
                  var x = rows[ctx.dataIndex];
                  return signed(x.wins, 1) + " wins: " + x.r.record + " (all-play " + x.r.allplayRecord + ")";
                },
              },
            },
          },
          scales: {
            x: yAxis({
              title: { display: true, text: "Wins above what scores earned" },
              ticks: { callback: function (v) { return (v > 0 ? "+" : "") + v; } },
            }),
            y: xAxis,
          },
        },
      };
    },

    "chart-pf-pa": function () {
      var rows = DATA.pfVsPa.slice().sort(function (a, b) { return b.pf - a.pf; });
      return {
        type: "bar",
        data: {
          labels: rows.map(function (r) { return teamName(r.rosterId); }),
          datasets: [
            { label: "PF", data: rows.map(function (r) { return r.pf; }), backgroundColor: C.accent, borderRadius: 4, maxBarThickness: 14 },
            { label: "PA", data: rows.map(function (r) { return r.pa; }), backgroundColor: alpha(C.muted, 0.6), borderRadius: 4, maxBarThickness: 14 },
          ],
        },
        options: {
          indexAxis: "y",
          plugins: { legend: { position: "bottom", labels: { pointStyle: "rectRounded" } } },
          scales: { x: yAxis(), y: xAxis },
        },
      };
    },
  };

  // The weekly-points chart highlights one team, picked above the chart:
  // the team last used in the trade calculator, else this week's #1.
  function pickTeam(chart, ids) {
    var select = document.getElementById("highlight-team");
    if (!select) return;
    var saved = null;
    try {
      saved = window.localStorage.getItem(PICK_KEY);
    } catch (e) {
      saved = null;
    }
    var rt = DATA.rankTrajectory.series;
    var leader = ids.filter(function (rid) { return rt[rid] && rt[rid][rt[rid].length - 1] === 1; })[0];
    // a redraw (theme change) keeps whatever was picked on this page
    var rid = select.getAttribute("data-picked") ||
      (saved && ids.indexOf(saved) >= 0 ? saved : leader || ids[0]);
    function apply(value) {
      chart.$picked = ids.indexOf(value);
      chart.update("none");
    }
    select.value = rid;
    apply(rid);
    select.parentNode.hidden = false;
    select.onchange = function () {
      select.setAttribute("data-picked", select.value);
      apply(select.value);
      try {
        window.localStorage.setItem(PICK_KEY, select.value);
      } catch (e) {
        // blocked storage: just don't remember
      }
    };
  }

  var charts = [];
  function renderAll() {
    charts.forEach(function (chart) { chart.destroy(); });
    charts = [];
    readTokens();
    Object.keys(renderers).forEach(function (id) {
      var canvas = document.getElementById(id);
      if (!canvas) return;
      var config = renderers[id](canvas);
      var after = config.$after;
      delete config.$after;
      config.options.responsive = true;
      config.options.maintainAspectRatio = false;
      var chart = new Chart(canvas, config);
      if (after) after(chart);
      charts.push(chart);
    });
  }

  document.addEventListener("DOMContentLoaded", function () {
    renderAll();
    document.addEventListener("ffpr:theme", renderAll);
    if (window.matchMedia) {
      var media = window.matchMedia("(prefers-color-scheme: dark)");
      if (media.addEventListener) media.addEventListener("change", renderAll);
    }
  });
})();

// Trade calculator (trades.html). Kept apart from the charts so it works even
// when the Chart.js CDN doesn't load. Player values, projections, each
// player's availability week by week and the verdict bands come from
// data.js; the lineup and verdict logic mirror compute.best_lineup and
// compute.trade_verdict.
(function () {
  "use strict";
  var DATA = window.FFPR;
  var root = document.getElementById("trade-calculator");
  if (!DATA || !DATA.trade || !root) return;

  var T = DATA.trade;
  var PLAYERS = T.players;
  var HORIZON = T.horizon || [];
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
  var status = document.getElementById("trade-status"); // read out by screen readers
  var summary = root.querySelector(".trade-summary"); // sticky verdict on phones
  var announced = false;

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
  var teamName = FFPR_UTIL.teamName;
  var teamColor = FFPR_UTIL.teamColor;
  // A team's badge and name, built like ui.team_inline in _macros.html.
  function teamInline(rid) {
    var t = DATA.teams[String(rid)] || {};
    var mark = h("span", "team-mark", [(Array.from(teamName(rid))[0] || "?").toUpperCase()]);
    mark.setAttribute("aria-hidden", "true");
    mark.style.setProperty("--team-l", t.color || "#808080");
    mark.style.setProperty("--team-d", t.colorDark || t.color || "#808080");
    if (t.avatar) {
      var img = h("img", "team-avatar");
      img.alt = "";
      img.src = t.avatar;
      mark.appendChild(img);
    }
    return h("span", "team-inline", [mark, h("span", "team-name", [teamName(rid)])]);
  }
  function checkboxFor(i, pid) {
    return sides[i].el.querySelector('.trade-roster input[value="' + pid + '"]');
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
    if (p.waiver) return "a waiver " + p.pos + " (" + fmt(p.proj) + ")";
    return p.name + " (" + p.pos + ", " + fmt(p.proj) + ")";
  }

  // The highest-scoring legal lineup from these players, as in
  // compute.best_lineup: fixed slots take the best player left at their
  // position, then flex slots are searched exhaustively. Players score their
  // projection unless `pts` says otherwise (one week's expected points). On
  // equal points the players in `keep` stay in, so a tie never reads as a
  // lineup change.
  function bestLineup(pids, keep, pts) {
    pts = pts || function (pid) { return PLAYERS[pid].proj; };
    var remaining = pids.filter(isActive).sort(function (a, b) {
      var diff = pts(b) - pts(a);
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
        var points = Math.round(sum(picked, pts) * 100) / 100;
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

  function sum(pids, pts) {
    return pids.reduce(function (acc, pid) { return acc + pts(pid); }, 0);
  }

  // Each remaining week's weight for this team: regular-season weeks count
  // fully, playoff weeks by the team's playoff odds (or the league-wide share
  // of teams that make it, without odds).
  function weekWeights(rid) {
    var pct = T.playoffPct[rid];
    return HORIZON.map(function (week, k) {
      var playoff = T.playoffFrom != null && week >= T.playoffFrom;
      return playoff && pct != null ? pct : T.weights[k];
    });
  }

  // Expected starting-lineup points per week over the rest of the season:
  // the best lineup is rebuilt every week from who's available (byes,
  // injuries), and the weeks are averaged by their weights. With no weeks
  // left to play it's just the healthy lineup.
  function seasonPoints(pids, weights) {
    if (!HORIZON.length) return sum(bestLineup(pids, {}), function (pid) { return PLAYERS[pid].proj; });
    var total = 0;
    var weight = 0;
    weights.forEach(function (w, k) {
      if (!w) return;
      var pts = function (pid) {
        var avail = PLAYERS[pid].avail;
        return PLAYERS[pid].proj * (avail && avail[k] != null ? avail[k] : 1);
      };
      total += w * sum(bestLineup(pids, {}, pts), pts);
      weight += w;
    });
    return weight ? total / weight : 0;
  }

  // A stand-in for the best player on waivers: a full-time player at the
  // position's replacement level.
  function waiverPlayer(pos, n) {
    var pid = "waiver:" + pos + ":" + n;
    PLAYERS[pid] = PLAYERS[pid] || {
      name: "Waiver pickup", pos: pos, nfl: null, proj: T.replacement[pos], value: 0,
      injury: null, active: true, avail: null, waiver: true,
    };
    return pid;
  }

  // The roster a team would really field after the trade. Over the roster
  // limit, it drops whoever its lineup misses least, one at a time; with spots
  // the trade opened, it fills each from waivers at whichever position helps
  // its lineup most.
  function settle(before, after, weights) {
    var activeBefore = before.filter(isActive).length;
    var activeAfter = after.filter(isActive).length;
    var mustDrop = Math.max(0, activeAfter - Math.max(T.rosterLimit, activeBefore));
    var open = Math.max(0, Math.min(activeBefore, T.rosterLimit) - activeAfter);
    var pids = after.slice();
    var dropped = [];
    var added = [];
    for (var d = 0; d < mustDrop; d++) {
      var cut = null;
      var keepPoints = -Infinity;
      pids.filter(isActive).forEach(function (pid) {
        var points = seasonPoints(pids.filter(function (x) { return x !== pid; }), weights);
        var better = points > keepPoints + 0.005 ||
          (Math.abs(points - keepPoints) <= 0.005 && PLAYERS[pid].value < PLAYERS[cut].value);
        if (better) {
          cut = pid;
          keepPoints = points;
        }
      });
      pids.splice(pids.indexOf(cut), 1);
      dropped.push(cut);
    }
    var startable = Object.keys(T.replacement).filter(function (pos) {
      return SLOTS.some(function (s) { return s === pos || (T.flex[s] || []).indexOf(pos) >= 0; });
    });
    var current = open ? seasonPoints(pids, weights) : 0;
    for (var a = 0; a < open; a++) {
      var pick = null;
      var pickPoints = current + 0.005;
      startable.forEach(function (pos) {
        var pid = waiverPlayer(pos, a);
        var points = seasonPoints(pids.concat([pid]), weights);
        if (points > pickPoints) {
          pick = pid;
          pickPoints = points;
        }
      });
      if (!pick) break;
      pids.push(pick);
      added.push(pick);
      current = pickPoints;
    }
    return { pids: pids, dropped: dropped, added: added };
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
      var rid = sides[i].select.value;
      var weights = weekWeights(rid);
      var before = rosterOf(i);
      var traded = before.filter(function (pid) { return sends[i].indexOf(pid) < 0; }).concat(sends[1 - i]);
      var settled = settle(before, traded, weights);
      var after = settled.pids;
      var lineupBefore = bestLineup(before, {});
      var keep = {};
      lineupBefore.forEach(function (pid) { keep[pid] = true; });
      var lineupAfter = bestLineup(after, keep);
      return {
        rid: rid,
        gets: gets[i],
        gives: gets[1 - i],
        before: seasonPoints(before, weights),
        after: seasonPoints(after, weights),
        weight: weights.reduce(function (acc, w) { return acc + w; }, 0),
        playoffPct: T.playoffPct[rid],
        swaps: pairSwaps(
          lineupAfter.filter(function (pid) { return lineupBefore.indexOf(pid) < 0; }),
          lineupBefore.filter(function (pid) { return lineupAfter.indexOf(pid) < 0; })
        ),
        dropped: settled.dropped,
        added: settled.added,
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

  // What the deal does to each team's lineup, in words: a change under
  // T.fitGap points a week doesn't count.
  function fitLine(teams) {
    var moves = teams.map(function (t) {
      var d = t.after - t.before;
      return d >= T.fitGap ? "helps" : d <= -T.fitGap ? "hurts" : null;
    });
    if (!moves[0] && !moves[1]) return "Barely changes either lineup";
    if (moves[0] === moves[1]) return (moves[0] === "helps" ? "Helps" : "Hurts") + " both lineups";
    var parts = [0, 1].filter(function (i) { return moves[i]; }).map(function (i) {
      var name = teamName(teams[i].rid);
      return moves[i] + " " + name + (/s$/i.test(name) ? "'" : "'s") + " lineup";
    });
    var text = parts.join(", ");
    return text.charAt(0).toUpperCase() + text.slice(1);
  }

  function horizonNote(t, delta) {
    var regular = T.weeksLeft > 0
      ? "the " + T.weeksLeft + " regular-season week" + (T.weeksLeft === 1 ? "" : "s") + " left"
      : null;
    var playoffs = T.playoffFrom != null
      ? "the playoffs" + (t.playoffPct != null
        ? ", which count by this team's " + Math.round(t.playoffPct * 100) + "% playoff odds"
        : "")
      : null;
    var span = [regular, playoffs].filter(Boolean).join(" and ");
    return "About " + signed(delta * t.weight) + " points over " + span + ".";
  }

  function teamCard(t) {
    var delta = t.after - t.before;
    var card = h("div", "award-card", [
      h("h3", null, [teamInline(t.rid)]),
      h("p", "trade-line", [
        "Gets ", h("strong", null, [fmt(t.gets)]), " · gives " + fmt(t.gives) + " · net ",
        h("span", trend(t.gets - t.gives), [signed(t.gets - t.gives)]),
      ]),
      h("p", "trade-line", [
        (HORIZON.length ? "Lineup, rest of season " : "Best lineup ") +
          fmt(t.before) + " → " + fmt(t.after) + " pts/wk ",
        h("strong", trend(delta), [signed(delta)]),
      ]),
    ]);
    card.style.setProperty("--award-accent", teamColor(t.rid));
    if (HORIZON.length && t.weight > 0 && Math.abs(delta) >= 0.005) {
      card.appendChild(h("p", "note trade-line", [horizonNote(t, delta)]));
    }
    t.dropped.forEach(function (pid) {
      card.appendChild(h("p", "trade-line down", [
        "Drops " + describe(pid) + ", the player its lineup misses least, to stay under the " +
          T.rosterLimit + "-player roster limit.",
      ]));
    });
    t.added.forEach(function (pid) {
      card.appendChild(h("p", "trade-line", [
        "Fills the open roster spot with " + describe(pid) + ", a replacement-level pickup.",
      ]));
    });
    if (t.swaps.length) {
      card.appendChild(h("p", "note trade-line", ["Healthy lineup:"]));
      card.appendChild(h("ul", "trade-changes", t.swaps.map(function (pair) {
        if (pair[0] && pair[1]) return h("li", null, [describe(pair[0]) + " replaces " + describe(pair[1])]);
        if (pair[0]) return h("li", null, [describe(pair[0]) + " fills an empty slot"]);
        return h("li", "down", [describe(pair[1]) + " leaves a slot nobody can fill"]);
      })));
    } else {
      card.appendChild(h("p", "note trade-line", ["Healthy starting lineup unchanged."]));
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
      btn.addEventListener("click", function () {
        toggle(s, pid, true);
        var chips = sides[s].el.querySelectorAll(".trade-chip");
        if (chips.length) chips[chips.length - 1].focus();
      });
      p.appendChild(btn);
    });
    return p;
  }

  function valueBar(a) {
    var bar = h("div", "trade-bar");
    [0, 1].forEach(function (i) {
      var seg = h("span");
      seg.style.flexGrow = String(a.gets[i]);
      seg.style.background = teamColor(sides[i].select.value);
      bar.appendChild(seg);
    });
    return bar;
  }

  function renderResult() {
    result.textContent = "";
    summary.textContent = "";
    if (!sides[0].picked.length && !sides[1].picked.length) {
      result.appendChild(h("p", "note", ["Tap players on either side to build a trade."]));
      status.textContent = announced ? "Trade cleared." : "";
      announced = false;
      return;
    }
    var a = analyze();
    var bar = valueBar(a);
    var detail = a.verdict.key === "even"
      ? "Neither side gets a player projected above replacement level."
      : teamName(a.teams[0].rid) + " gets " + fmt(a.gets[0]) + " in value, " +
        teamName(a.teams[1].rid) + " gets " + fmt(a.gets[1]) +
        " (points per week above replacement, rest of season).";
    result.appendChild(h("div", "award-card trade-verdict", [
      h("h3", null, ["Verdict"]),
      h("p", "award-value", [headline(a.verdict)]),
      h("p", "award-teams", [detail]),
      bar,
      h("div", "trade-bar-legend", [0, 1].map(function (i) {
        return h("span", null, [teamName(a.teams[i].rid) + " " + fmt(a.gets[i])]);
      })),
      h("p", "trade-line", [h("strong", null, ["Lineup fit: "]), fitLine(a.teams)]),
    ]));
    status.textContent = headline(a.verdict) + ". Lineup fit: " + fitLine(a.teams) + ".";
    announced = true;
    var jump = h("button", null, [headline(a.verdict), valueBar(a)]);
    jump.type = "button";
    jump.setAttribute("aria-label", "Show the verdict");
    jump.addEventListener("click", function () { result.scrollIntoView({ behavior: "smooth", block: "start" }); });
    summary.appendChild(jump);
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
      sides[0].select.focus();
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
      var group = h("div", null, [h("div", "trade-group", [pos])]);
      group.setAttribute("role", "group");
      group.setAttribute("aria-label", pos);
      box.appendChild(group);
      groups[pos].forEach(function (pid) {
        var p = PLAYERS[pid];
        var input = h("input");
        input.type = "checkbox";
        input.value = pid;
        input.addEventListener("change", function () { toggle(i, pid, input.checked); });
        group.appendChild(h("label", "trade-player", [
          input,
          h("span", "trade-name", [p.name]),
          p.injury ? h("span", "inj", [p.injury]) : null,
          h("span", "trade-meta", [(p.nfl || "FA") + " · ", h("span", "sr-only", ["projected "]), fmt(p.proj)]),
          h("span", "trade-val", [h("span", "sr-only", ["value "]), fmt(p.value)]),
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
      chip.addEventListener("click", function () {
        // keep keyboard focus nearby: the next chip, else the player's checkbox
        var at = sides[i].picked.indexOf(pid);
        toggle(i, pid, false);
        var chips = sides[i].el.querySelectorAll(".trade-chip");
        var next = chips[Math.min(at, chips.length - 1)] || checkboxFor(i, pid);
        if (next) next.focus();
      });
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
      sides[i].el.style.setProperty("--team", teamColor(sides[i].select.value));
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
    root.querySelector(".trade-sides").hidden = false; // shown only when this runs
    // team colors follow the theme
    document.addEventListener("ffpr:theme", update);
    var media = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)");
    if (media && media.addEventListener) media.addEventListener("change", update);
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
