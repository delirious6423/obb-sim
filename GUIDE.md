# Player guide

## Your job

You are the general manager of one club. On the New game screen you can
also take the owner seat (budget, ticket price, ballpark, stance) and the
commissioner seat (league rules). The computer runs every seat you do not
hold: the field manager, the other 29 general managers, and the owner and
commissioner unless you took them. The other general managers are as good
as the difficulty you chose.

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

A club whose cash is below zero is in debt: it may sign only minimum-salary
players to fill its roster, may not take on salary in a trade, is held to the
payroll floor and pays its owner nothing until its profit has paid the debt.
Nobody bails a club out.

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

## How to move a save

Use the Saves screen to move a league to another computer.

1. On the machine you are leaving, open Saves and click **Export** on the
   slot. The save is written as `<slot>.bsave` in the export folder (Downloads
   by default; you can type another folder). The file holds the whole league
   and its checksum. Copy it to the other machine by any means.
2. On the new machine, open Saves, type the path of the `.bsave` file under
   **Import file**, and click **Import**. You can also type a slot name under
   **as**. If you leave it empty, the save gets a free name from its file.
3. Import never overwrites a slot. If the name is taken, the save gets a new
   name such as `career-2`.

The game refuses a file that is damaged, cut short, or not a save. It also
refuses a save made with other calibration data, and says which data it
needs. A save made by an older build of the game loads, and the game tells you
that the rules may differ.

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
