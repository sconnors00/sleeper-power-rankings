import os, httpx
B = "https://api.sleeper.app/v1"
g = lambda p: httpx.get(B + p, timeout=30).json()
lid = os.environ["SLEEPER_LEAGUE_ID"]
while lid:
    L = g(f"/league/{lid}")
    s = L["settings"]
    print(f"DIAG === {L['season']} divisions={s.get('divisions')} playoff_teams={s.get('playoff_teams')} seed_type={s.get('playoff_seed_type')} playoff_type={s.get('playoff_type')} round_type={s.get('playoff_round_type')} status={L.get('status')}")
    names = {}
    for d in (L.get("metadata") or {}):
        if d.startswith("division_") and d.endswith("_name") is False and d.count("_") == 1:
            pass
    rosters = g(f"/league/{lid}/rosters")
    rows = []
    for r in rosters:
        st = r.get("settings") or {}
        pf = (st.get("fpts") or 0) + (st.get("fpts_decimal") or 0) / 100
        rows.append((st.get("wins", 0), st.get("ties", 0), pf, r["roster_id"], st.get("division")))
    rows.sort(key=lambda x: (-(x[0] + 0.5 * x[1]), -x[2]))
    for i, (w, t, pf, rid, div) in enumerate(rows, 1):
        print(f"DIAG   {i:>2}. rid={rid:>2} div={div} W={w} T={t} PF={pf:.2f}")
    br = g(f"/league/{lid}/winners_bracket") or []
    r1 = sorted({x for m in br if m.get("r") == 1 for x in (m.get("t1"), m.get("t2")) if isinstance(x, int)})
    r2 = [m for m in br if m.get("r") == 2]
    byes = sorted({m[k] for m in r2 for k in ("t1", "t2") if isinstance(m.get(k), int) and not m.get(k + "_from")})
    print(f"DIAG   round1={r1} byes={byes}")
    print(f"DIAG   bracket={[(m.get('r'), m.get('m'), m.get('t1'), m.get('t2'), m.get('p')) for m in br]}")
    lid = L.get("previous_league_id")
