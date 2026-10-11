#!/usr/bin/env python3
"""Measure league rates and talent spread from Retrosheet 2015-2019 event files.

Writes data/internal/params.json, which holds league-wide aggregates only.
Player ids are used in memory to measure spreads and are never written,
printed or logged. Zips stay in memory. Standard library only.

Run: python3 -I -B tools/build_params.py
"""

import csv
import io
import json
import math
import os
import re
import urllib.request
import zipfile

YEARS = (2015, 2016, 2017, 2018, 2019)
SOURCE_URL = "https://www.retrosheet.org/events/{year}eve.zip"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_PATH = os.path.join(ROOT, "data", "internal", "params.json")

OUTCOMES = ("k", "bb", "hbp", "single", "double", "triple", "hr", "ipo")
MIN_QUALIFIER_PA = 300
RUNNER_ONLY = ("NP", "SB", "CS", "POCS", "PO", "WP", "PB", "BK", "DI", "OA", "FLE")
FORCE_KINDS = frozenset(("S", "BB", "HBP", "ROE", "FC"))
AB_KINDS = frozenset(("S", "D", "T", "HR", "K", "OUT", "FC", "ROE"))
OUT_KINDS = frozenset(("K", "OUT", "SH", "SF", "OTHER"))
BALL_KINDS = ("OUT", "SH", "SF", "FC")
OUTCOME_INDEX = {"K": 0, "BB": 1, "HBP": 2, "S": 3, "D": 4, "T": 5, "HR": 6}
MOVE_RE = re.compile(r"([123B])([-X])([123H])")
PAREN_RE = re.compile(r"\((\d|B)\)")
STEAL_RE = re.compile(r"(POCS|SB|CS|PO)(\d)")

# name -> number of outs buckets (index = outs at the start of the play)
BR_SPEC = (
    ("single_r1_to_3", 3),
    ("single_r2_scores", 3),
    ("double_r1_scores", 3),
    ("dp_given_ground_r1", 2),
    ("ground_r3_scores", 2),
    ("ground_r2_to_3", 2),
    ("fly_r3_scores", 2),
    ("fly_r2_to_3", 2),
)


# Generic linear-weight run values per outcome (runs relative to an out), in
# OUTCOMES order. Textbook constants, used only to convert pitcher plate
# appearances into designated-hitter ones for the run-environment target.
LINEAR_WEIGHTS = (-0.03, 0.70, 0.73, 0.89, 1.26, 1.59, 2.05, 0.0)


class Totals:
    def __init__(self):
        self.games = 0
        self.games_no_type = 0
        self.malformed = 0
        self.pa = 0
        self.outcome = [0] * 8
        self.kinds = {}
        self.unknown = {}
        self.gdp_flag = 0
        self.gdp_computed = 0
        self.cons_checked = 0
        self.cons_bad = 0
        self.steal_att = {2: 0, 3: 0}
        self.steal_ok = {2: 0, 3: 0}
        self.opp = {2: 0, 3: 0}
        self.pitch_sum = [0] * 8
        self.pitch_n = [0] * 8
        self.br = {name: [[0, 0] for _ in range(n)] for name, n in BR_SPEC}
        self.bat = {}
        self.pit = {}
        self.balls = {"G": 0, "F": 0, "L": 0}
        self.opp_counted = 0
        self.runs = 0
        self.extra_games = 0
        self.pitcher_outcome = [0] * 8
        # (year, team) -> [pa, ab, hits, strikeouts, home runs] for non-pitcher batters
        self.team_rows = {}
        self.team_games = {}
        self.cur_team = None


def classify(base):
    """Map the batter-event code (modifiers removed) to a kind."""
    if not base:
        return "OTHER"
    if base.startswith("IW") or base.startswith("W"):
        return "BB"
    if base.startswith("HP"):
        return "HBP"
    if base.startswith("C"):  # catcher's interference: batter to first
        return "BB"
    if base.startswith("K"):
        return "K"
    if base.startswith("H"):
        return "HR"
    if base.startswith("DP") or base.startswith("TP"):
        return "OTHER"
    if base.startswith("T"):
        return "T"
    if base.startswith("D"):
        return "D"
    if base.startswith("SF"):
        return "SF"
    if base.startswith("SH"):
        return "SH"
    if base.startswith("S"):
        return "S"
    if base.startswith("FC"):
        return "FC"
    if base.startswith("E") and base[1:2].isdigit():
        return "ROE"
    if base[0].isdigit() or base[0] in "GFPL":
        return "OUT"
    return "OTHER"


def ball_type(first, kind):
    """Batted-ball type of an in-play out: G ground, F fly (incl. popups), L line.

    Uses the trajectory modifiers after the first slash. With none, a single
    fielder number is a fly out and two or more fielder numbers a ground out.
    """
    if kind == "SH":
        return "G"
    if kind == "SF":
        return "F"
    mods = first.split("/")[1:]
    for m in mods:
        if m.startswith("SF"):
            return "F"
        if m.startswith("SH"):
            return "G"
    for m in mods:
        if m == "FO":  # force out modifier, a ground ball
            return "G"
        if m.startswith(("G", "BG")):
            return "G"
        if m.startswith(("L", "BL")):
            return "L"
        if m.startswith(("P", "BP", "IF", "F")):
            return "F"
    if kind == "FC":
        return "G"
    base = re.sub(r"\([^)]*\)", "", first.split("/")[0])
    return "G" if len(base) >= 2 else "F"


def count_pitches(seq):
    """Count real pitches; N (no pitch), digits, '.', '>', '+', '*' and '?' are not pitches."""
    return sum(1 for ch in seq if ch in "BCFHIKLMOPQRSTUVXY")


def add_player(table, pid, oc):
    row = table.get(pid)
    if row is None:
        row = [0] * 9
        table[pid] = row
    row[0] += 1
    row[oc + 1] += 1


def record(tot, name, idx, hit):
    cell = tot.br[name][idx]
    cell[1] += 1
    if hit:
        cell[0] += 1


def count_opportunity(tot, pre):
    """One plate appearance began with these bases; count steal opportunities."""
    tot.opp_counted += 1
    if pre[1] and not pre[2]:
        tot.opp[2] += 1
    if pre[2] and not pre[3]:
        tot.opp[3] += 1


def process_play(tot, st, ev, batter, pit_id, pitches, count_it):
    head, _, adv = ev.partition(".")
    parts = head.split("+")
    first = parts[0]
    is_pa = not first.startswith(RUNNER_ONLY)
    occ = st["occ"]
    entering = occ[:]
    outs_before = st["outs"]
    if st.get("pa_pre") is None:
        st["pa_pre"] = occ[:]  # base state when this plate appearance began
        st["pa_count"] = count_it

    # Steals, caught stealing and pickoffs happen before the batted ball.
    steal_outs = 0
    for part in parts:
        m = STEAL_RE.match(part)
        if not m:
            continue
        kind, d = m.group(1), int(m.group(2))
        if kind == "PO":
            if occ[d]:
                occ[d] = False
                steal_outs += 1
            continue
        if d not in (2, 3):
            continue
        src = d - 1
        if kind == "SB":
            if occ[src] and not occ[d]:
                occ[src], occ[d] = False, True
                if count_it:
                    tot.steal_ok[d] += 1
                    tot.steal_att[d] += 1
        elif occ[src]:
            occ[src] = False
            steal_outs += 1
            if count_it:
                tot.steal_att[d] += 1

    moves = MOVE_RE.findall(adv)
    listed = {}
    batter_move = None
    for src, sep, dst in moves:
        if src == "B":
            batter_move = (sep, dst)
        else:
            listed.setdefault(int(src), (sep, dst))
    paren = PAREN_RE.findall(first) if is_pa else []
    paren_runners = {int(x) for x in paren if x != "B"}
    batter_out = "B" in paren

    kind = None
    if is_pa:
        base = first.split("/", 1)[0]
        kind = classify(base)
        if kind == "OTHER":
            code = base[:8]
            tot.unknown[code] = tot.unknown.get(code, 0) + 1

    new = [False] * 4
    scored = 0
    runner_outs = 0
    unlisted = []

    def put(base):
        # A second runner on the same base is a data conflict. Leaving it unset
        # makes the conservation check count it as a mismatch.
        if not new[base]:
            new[base] = True

    for s in (1, 2, 3):
        if not occ[s]:
            continue
        if s in listed:
            sep, dst = listed[s]
            if sep == "X":
                runner_outs += 1
            elif dst == "H":
                scored += 1
            else:
                put(int(dst))
        elif s in paren_runners:
            runner_outs += 1
        else:
            unlisted.append(s)

    # Runners with no explicit move. A home run clears the bases. Otherwise a
    # runner is forced up only when the batter reached first and every base
    # behind him was occupied before the batted ball; all others stay put.
    # Unlisted forced runners are processed in ascending base order.
    if kind == "HR":
        scored += len(unlisted)
    else:
        for s in unlisted:
            forced = kind in FORCE_KINDS and all(occ[1:s])
            if not forced:
                put(s)
            elif s == 3:
                scored += 1
            else:
                put(s + 1)

    if is_pa:
        if batter_move is not None:
            sep, dst = batter_move
            if sep == "X":
                batter_out = True
            elif dst == "H":
                scored += 1
            else:
                put(int(dst))
        elif kind in FORCE_KINDS:
            put(1)
        elif kind == "D":
            put(2)
        elif kind == "T":
            put(3)
        elif kind == "HR":
            scored += 1
        elif kind in OUT_KINDS:
            # A force-out modifier means a runner was retired and the batter
            # reached first, so he is not out.
            if "FO" in first.split("/")[1:]:
                put(1)
            else:
                batter_out = True

    tot.runs += scored

    # Conservation: every runner (and the batter) ends on a base, scores or is out.
    before = sum(entering[1:]) + (1 if is_pa else 0)
    after = sum(new[1:]) + scored + runner_outs + steal_outs + (1 if batter_out else 0)
    tot.cons_checked += 1
    if before != after:
        tot.cons_bad += 1

    if is_pa:
        pre = st["pa_pre"]
        if st["pa_count"]:
            count_opportunity(tot, pre)
        st["pa_pre"] = None

    if is_pa and not count_it:
        tot.pitcher_outcome[OUTCOME_INDEX.get(kind, 7)] += 1

    if is_pa and count_it and tot.cur_team is not None:
        row = tot.team_rows.setdefault(tot.cur_team, [0] * 5)
        row[0] += 1
        if kind in AB_KINDS:
            row[1] += 1
        if kind in ("S", "D", "T", "HR"):
            row[2] += 1
        if kind == "K":
            row[3] += 1
        if kind == "HR":
            row[4] += 1

    if is_pa and count_it:
        oc = OUTCOME_INDEX.get(kind, 7)
        tot.pa += 1
        tot.outcome[oc] += 1
        tot.kinds[kind] = tot.kinds.get(kind, 0) + 1
        add_player(tot.bat, batter, oc)
        if pit_id is not None:
            add_player(tot.pit, pit_id, oc)
        n_p = count_pitches(pitches)
        if n_p:
            tot.pitch_sum[oc] += n_p
            tot.pitch_n[oc] += 1
        on_play_outs = runner_outs + (1 if batter_out else 0)
        outs_bb = outs_before + steal_outs
        ball = ball_type(first, kind) if kind in BALL_KINDS else None
        if ball is not None:
            tot.balls[ball] += 1
        ground = kind in ("OUT", "FC") and ball == "G"
        fly = kind in ("OUT", "SF") and ball == "F"
        if ground and on_play_outs >= 2:
            tot.gdp_computed += 1
        if "/GDP" in ev:
            tot.gdp_flag += 1

        def dst_of(src):
            hit = listed.get(src)
            return hit[1] if hit and hit[0] == "-" else None

        occ0 = occ  # after any steal, before the batted ball
        if kind == "S" and outs_bb <= 2:
            if occ0[1] and not occ0[2]:
                record(tot, "single_r1_to_3", outs_bb, dst_of(1) in ("3", "H"))
            if occ0[2]:
                record(tot, "single_r2_scores", outs_bb, dst_of(2) == "H")
        elif kind == "D" and occ0[1] and outs_bb <= 2:
            record(tot, "double_r1_scores", outs_bb, dst_of(1) == "H")
        elif ground and outs_bb <= 1:
            if occ0[1]:
                record(tot, "dp_given_ground_r1", outs_bb, on_play_outs >= 2)
            if outs_bb + on_play_outs <= 2:
                if occ0[3] and not (occ0[1] and occ0[2]):
                    record(tot, "ground_r3_scores", outs_bb, dst_of(3) == "H")
                if occ0[2] and not occ0[3] and not occ0[1]:
                    record(tot, "ground_r2_to_3", outs_bb, dst_of(2) == "3")
        elif fly and outs_bb <= 1:
            if occ0[3]:
                record(tot, "fly_r3_scores", outs_bb, dst_of(3) == "H")
            if occ0[2] and not occ0[3]:
                record(tot, "fly_r2_to_3", outs_bb, dst_of(2) == "3")

    st["occ"] = new
    st["outs"] = outs_before + steal_outs + runner_outs + (1 if batter_out else 0)


def run_text(text, tot):
    game = None
    st = None
    half = None
    pit = {}
    pos = {}

    def fresh_state():
        return {"occ": [False] * 4, "outs": 0, "pa_pre": None, "pa_count": True}

    def flush_pending():
        # A half-inning can end on a caught stealing with the batter still up.
        if st is not None and st.get("pa_pre") is not None and st.get("pa_count"):
            count_opportunity(tot, st["pa_pre"])

    for raw in text.splitlines():
        line = raw.rstrip("\r")
        if not line:
            continue
        if line.startswith("id,"):
            flush_pending()
            finish_game(game, tot)
            ident = line.split(",")[1] if "," in line else ""
            game = {"regular": None, "home": ident[:3], "year": ident[3:7], "vis": None}
            st = fresh_state()
            half = None
            pit = {}
            pos = {}
            continue
        if game is None:
            continue
        if line.startswith("info,visteam,"):
            game["vis"] = line.split(",")[2].strip()
        if line.startswith("info,gametype,"):
            game["regular"] = line.split(",")[2].strip() == "regular"
        elif game["regular"] is False:
            continue
        elif line.startswith(("start,", "sub,")):
            f = next(csv.reader([line]))
            if len(f) >= 6:
                pos[f[1]] = f[5]
                if f[5] == "1" and f[3] in ("0", "1"):
                    pit[int(f[3])] = f[1]
        elif line.startswith("play,"):
            f = line.split(",", 6)
            if len(f) < 7 or f[2] not in ("0", "1"):
                tot.malformed += 1
                continue
            key = (f[1], f[2])
            if f[1].isdigit():
                game["max_inn"] = max(game.get("max_inn", 0), int(f[1]))
            if key != half:
                flush_pending()
                half = key
                st = fresh_state()
            fielding_side = 1 - int(f[2])
            tot.cur_team = (game["year"], game["vis"] if f[2] == "0" else game["home"])
            count_it = pos.get(f[3]) != "1"  # skip pitchers batting
            process_play(tot, st, f[6], f[3], pit.get(fielding_side), f[5], count_it)
    flush_pending()
    finish_game(game, tot)


def finish_game(game, tot):
    """The *eve.zip archives hold regular-season games only; a file that names
    a game type is checked, one that does not is trusted."""
    if game is None:
        return
    if game["regular"] is None:
        tot.games_no_type += 1
        tot.games += 1
    elif game["regular"]:
        tot.games += 1
    else:
        return
    for team in (game["home"], game["vis"]):
        key = (game["year"], team)
        tot.team_games[key] = tot.team_games.get(key, 0) + 1
    if game.get("max_inn", 0) > 9:
        tot.extra_games += 1


def fetch(year):
    req = urllib.request.Request(
        SOURCE_URL.format(year=year), headers={"User-Agent": "baseball-sim-calibration"}
    )
    with urllib.request.urlopen(req, timeout=300) as resp:
        return resp.read()


def spread(table, league, exclude, min_n):
    """Relative true-talent SD per outcome, with binomial sampling noise removed."""
    rows = [r for pid, r in table.items() if pid not in exclude and r[0] >= min_n]
    n = len(rows)
    if n < 30:
        raise SystemExit("too few qualifying players: %d" % n)
    sds = []
    for o in range(8):
        ps = [r[o + 1] / r[0] for r in rows]
        mean = sum(ps) / n
        var = sum((p - mean) ** 2 for p in ps) / (n - 1)
        noise = sum(p * (1 - p) / r[0] for p, r in zip(ps, rows)) / n
        true_var = max(0.0, var - noise)
        sds.append(math.sqrt(true_var) / league[o] if league[o] > 0 else 0.0)
    return sds, n


def build(tot):
    if tot.pa == 0:
        raise SystemExit("no plate appearances read")
    league_vals = [tot.outcome[i] / tot.pa for i in range(7)]
    league_vals.append(1.0 - sum(league_vals))
    league = dict(zip(OUTCOMES, league_vals))

    b = tot.balls
    shares = b["G"] + b["F"] + b["L"]
    if shares == 0:
        raise SystemExit("no in-play outs read")
    roe = tot.kinds.get("ROE", 0)
    other = tot.kinds.get("OTHER", 0)
    fielding = {
        "ground": b["G"] / shares,
        "fly": b["F"] / shares,
        "line": b["L"] / shares,
        "error_rate": roe / (roe + shares + other),
    }

    baserunning = {}
    for name, _ in BR_SPEC:
        vals = []
        for idx, cell in enumerate(tot.br[name]):
            if cell[1] == 0:
                raise SystemExit("no observations for %s at outs=%d" % (name, idx))
            vals.append(cell[0] / cell[1])
        baserunning[name] = vals
    for d, key in ((2, "sb2"), (3, "sb3")):
        if tot.opp[d] == 0 or tot.steal_att[d] == 0:
            raise SystemExit("no steal observations for base %d" % d)
        baserunning[key + "_attempt"] = tot.steal_att[d] / tot.opp[d]
        baserunning[key + "_success"] = tot.steal_ok[d] / tot.steal_att[d]
    baserunning = {
        "single_r1_to_3": baserunning["single_r1_to_3"],
        "single_r2_scores": baserunning["single_r2_scores"],
        "double_r1_scores": baserunning["double_r1_scores"],
        "dp_given_ground_r1": baserunning["dp_given_ground_r1"],
        "ground_r3_scores": baserunning["ground_r3_scores"],
        "ground_r2_to_3": baserunning["ground_r2_to_3"],
        "fly_r3_scores": baserunning["fly_r3_scores"],
        "fly_r2_to_3": baserunning["fly_r2_to_3"],
        "sb2_attempt": baserunning["sb2_attempt"],
        "sb2_success": baserunning["sb2_success"],
        "sb3_attempt": baserunning["sb3_attempt"],
        "sb3_success": baserunning["sb3_success"],
    }

    total_n = sum(tot.pitch_n)
    overall = sum(tot.pitch_sum) / total_n
    pitches = [
        tot.pitch_sum[i] / tot.pitch_n[i] if tot.pitch_n[i] else overall for i in range(8)
    ]

    bat_sd, bat_n = spread(tot.bat, league_vals, frozenset(), MIN_QUALIFIER_PA)
    pit_sd, pit_n = spread(tot.pit, league_vals, frozenset(), 200)

    # Run environment. Pitchers batted in part of the sample, but the simulator
    # uses a designated hitter for everyone, so also report the runs per team-game
    # as if each pitcher plate appearance had been a league-average batter's.
    team_games = 2 * tot.games
    pitcher_pa = sum(tot.pitcher_outcome)
    value = lambda rates: sum(r * w for r, w in zip(rates, LINEAR_WEIGHTS))
    pitcher_rates = [x / pitcher_pa for x in tot.pitcher_outcome] if pitcher_pa else league_vals
    dh_gain = (pitcher_pa / team_games) * (value(league_vals) - value(pitcher_rates))
    rows = [(tot.team_rows[k], tot.team_games[k]) for k in sorted(tot.team_rows) if tot.team_games.get(k)]
    hits = sum(r[2] for r, _ in rows)
    abs_ = sum(r[1] for r, _ in rows)

    def dist(values):
        n = len(values)
        mean = sum(values) / n
        sd = math.sqrt(sum((v - mean) ** 2 for v in values) / (n - 1))
        return {"mean": mean, "sd": sd}

    team_season = {
        "n": len(rows),
        "avg": dist([r[2] / r[1] for r, _ in rows]),
        "k_pct": dist([r[3] / r[0] for r, _ in rows]),
        "hr_per_game": dist([r[4] / g for r, g in rows]),
        "hr_pct": dist([r[4] / r[0] for r, _ in rows]),
    }
    run_env = {
        "avg": hits / abs_,
        "team_season": team_season,
        "runs_per_team_game": tot.runs / team_games,
        "runs_per_team_game_dh": tot.runs / team_games + dh_gain,
        "extra_inning_share": tot.extra_games / tot.games,
        "pitcher_pa_per_team_game": pitcher_pa / team_games,
        "pitcher_rates": dict(zip(OUTCOMES, pitcher_rates)),
    }

    return {
        "source": "Retrosheet 2015-2019 regular-season event files, aggregated by tools/build_params.py",
        "pa": tot.pa,
        "league": league,
        "talent": {
            "batter": dict(zip(OUTCOMES, bat_sd)),
            "pitcher": dict(zip(OUTCOMES, pit_sd)),
        },
        "pitches_per_pa": dict(zip(OUTCOMES, pitches)),
        "fielding": fielding,
        "baserunning": baserunning,
        "run_environment": run_env,
    }, {"bat_n": bat_n, "pit_n": pit_n}


def report(tot, params, counts):
    team_games = 2 * tot.games
    print("games: %d (team-games %d); of which with no gametype line: %d"
          % (tot.games, team_games, tot.games_no_type))
    print("plate appearances: %d; malformed play lines: %d" % (tot.pa, tot.malformed))
    for name, val in params["league"].items():
        print("  league %-7s %.4f" % (name, val))
    print("GDP-flagged plays per team-game: %.3f" % (tot.gdp_flag / team_games))
    print("ground-out double plays by outs per team-game: %.3f" % (tot.gdp_computed / team_games))
    env = params["run_environment"]
    print("runs per team-game: %.3f (DH-equivalent %.3f); extra-inning games: %.1f%%; pitcher PA per team-game: %.2f"
          % (env["runs_per_team_game"], env["runs_per_team_game_dh"],
             100 * env["extra_inning_share"], env["pitcher_pa_per_team_game"]))
    print("state-consistency mismatches: %d of %d plays (%.4f%%)"
          % (tot.cons_bad, tot.cons_checked, 100.0 * tot.cons_bad / max(1, tot.cons_checked)))
    top = sorted(tot.unknown.items(), key=lambda kv: -kv[1])[:10]
    print("unclassified batter codes (top 10): %s" % top)
    print("PAs with steal-opportunity bookkeeping: %d" % tot.opp_counted)
    print("steal attempts 2nd/3rd: %d/%d (ok %d/%d); opportunities 2nd/3rd: %d/%d"
          % (tot.steal_att[2], tot.steal_att[3], tot.steal_ok[2], tot.steal_ok[3],
             tot.opp[2], tot.opp[3]))
    print("qualifying players (batters >= %d PA, pitchers >= 200 BF): batters %d, pitchers %d"
          % (MIN_QUALIFIER_PA, counts["bat_n"], counts["pit_n"]))
    print("pitches per PA (mean): %s" % {k: round(v, 2) for k, v in params["pitches_per_pa"].items()})
    print("fielding: %s" % {k: round(v, 4) for k, v in params["fielding"].items()})
    print("baserunning: %s" % {k: [round(x, 4) for x in v] if isinstance(v, list) else round(v, 4)
                               for k, v in params["baserunning"].items()})
    print("talent SD (batter): %s" % {k: round(v, 3) for k, v in params["talent"]["batter"].items()})
    print("talent SD (pitcher): %s" % {k: round(v, 3) for k, v in params["talent"]["pitcher"].items()})


def main():
    tot = Totals()
    for year in YEARS:
        print("reading %d" % year)
        data = fetch(year)
        with zipfile.ZipFile(io.BytesIO(data)) as zf:
            for name in sorted(zf.namelist()):
                if name.upper().endswith((".EVA", ".EVN")):
                    run_text(zf.read(name).decode("latin-1"), tot)
        del data
    params, counts = build(tot)
    os.makedirs(os.path.dirname(OUT_PATH), exist_ok=True)
    with open(OUT_PATH, "w", encoding="utf-8") as fh:
        json.dump(params, fh, indent=2)
        fh.write("\n")
    report(tot, params, counts)
    print("wrote %s" % OUT_PATH)


if __name__ == "__main__":
    main()
