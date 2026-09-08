(function () {
  "use strict";
  var DATA = window.FFPR;
  if (!DATA) return;

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
