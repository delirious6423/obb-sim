# Player guide

## Your job

You are the general manager of one club. The computer runs the owner, the
commissioner, the field manager and the other 29 general managers.
The other general managers are as good as the difficulty you chose.

## The year

1. **Regular season.** Advance from the Home screen. Your lineup and depth
   chart can change any day.
2. **Playoffs.** "Finish season" plays the playoffs, then the finances and
   ageing. Then the draft opens.
3. **Draft.** The other clubs pick until you are on the clock. Draft a player
   or pass. A player who asks for more than the pick can pay will not sign.
4. **Trades and free agency.** Answer trade offers, propose trades, sign free
   agents. Then "Start next season" lets the other clubs finish, sets the
   rosters and starts the new year.

## What you can see

Grades (20 to 80) are what your scouts think. They have errors. The true
talent of a player is hidden. You can see public stats, prices and your own
books.

## Rosters

A roster must be legal before the season starts: the Roster and Depth chart
screens list the problems in red. "Auto-arrange" fixes the order the way the
club staff would.

## Saves

Use the Saves screen. Save files are in `~/.baseball-sim/saves`, or in the
folder named by the environment variable `BASEBALL_SIM_SAVES`. Each slot has
a `.sav` file and a `.manifest.json` file. The game keeps a `.bak` copy of the
last good save.

## Rename clubs and players

All names are invented. You can rename clubs and players in a saved league
with the command-line tool. It changes names only. It never changes the saved
league in place: it writes a new slot.

```bash
cargo run --release -p cli -- roster export my-save names.json
# edit the "name" fields in names.json with a text editor
cargo run --release -p cli -- roster import my-save names.json my-save-renamed
```

Rules for the edit: a name is not empty, has at most 40 characters and no
control characters; two players do not share a name; do not change the ids,
ages, clubs or positions. If a rule is broken, the tool writes nothing and says
which row is wrong. Use `--saves <folder>` to name the save folder.
Then load `my-save-renamed` in the game.
