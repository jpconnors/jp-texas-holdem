# JP's Texas Hold'em

No-limit Texas Hold'em in the browser: you against 1–5 virtual opponents at Easy, Medium, Hard, or a mixed table. Plain HTML/CSS/JavaScript with no dependencies, no build step, and nothing to install beyond what's usually already on the machine.

## Run it

**Mac:** the simplest option is to double-click `index.html`, since the game runs fully in the browser. To use the launcher instead, double-click `Start (Mac).command`. macOS will say it "can't verify" the file because it came from the internet. Dismiss that warning, go to **System Settings → Privacy & Security**, and click **Open Anyway** next to the message. Or clear the quarantine flag once in Terminal:

```
xattr -dr com.apple.quarantine path/to/jp-holdem
```

You can also skip the launcher and run the server from Terminal:

```
cd path/to/jp-holdem
python3 server.py
```

**Windows:** double-click `Start (Windows).bat`. Or in a terminal: `py server.py` (or `node server.js`).

Either launcher starts a local server at `http://127.0.0.1:8765/` (next free port if that one is taken) and opens your browser. Close the terminal window to stop it. If neither Python 3 nor Node is installed, the launcher opens `index.html` directly. The game works that way too, since it uses no modules or network calls.

## How to play

1. Pick your name, number of opponents, difficulty, stack, blinds, and bot speed, then **Deal me in**.
2. Click **I'm Ready** (or press Space) to deal each hand.
3. On your turn, use **Fold / Check / Call / Bet**. The box next to *Money* is the total you're betting or raising **to**. The presets fill it in: **All in**, **.5x / 1x / 2x** pot-sized raises, **MIN** legal raise.

Keyboard: `F` fold · `C` check or call · `K` check · `B` or `Enter` bet/raise · `Space` deal · `H` hand history.

## The opponents

| Level | How it plays |
|---|---|
| **Easy** | Loose and passive. Plays most hands, calls too much, rarely raises, judges its hand with a lot of noise. |
| **Medium** | Runs a Monte Carlo simulation of its equity (~450 run-outs) against the players still in, compares it to the pot odds, value-bets good hands, and bluffs occasionally. |
| **Hard** | Simulates ~1,100 run-outs against a modeled range for each opponent, built from their VPIP/PFR history and how they've acted this hand. It uses positional opening ranges, 3-bets (wider against loose raisers), push/fold play when short-stacked, continuation bets, semi-bluffs, check-raise traps, and implied odds. It tracks your tendencies and adjusts: more bluffs if you fold to bets often, thinner value bets and overbets if you call too much. |

Bots see only what a real player would: their own cards, the board, bets, stacks, and public stats. No peeking.

Each bot also gets a small random personality (looser or tighter, more or less aggressive), so two bots at the same level don't play identically.

In 30 heads-up freezeouts per matchup during testing: Hard beat Easy 24–6, Hard beat Medium 24–6, and Medium beat Easy 21–9.

## Rules implemented

- Dealer button rotation, small and big blinds, heads-up blind rules (button posts the small blind and acts first preflop)
- Big blind option when everyone limps
- Minimum raise equal to the last full raise. A short all-in raise does not reopen betting for players who already acted.
- All-ins with automatic run-out, side pots, return of uncalled bets, split pots (odd chip to the first winner left of the button)
- Optional rising blinds (tournament style), player elimination, game-over standings, and a win/loss record per difficulty saved in the browser

## Project layout

```
index.html              table markup and dialogs
css/style.css           look and layout
js/cards.js             cards, deck, shuffling (crypto-strength RNG)
js/evaluator.js         7-card hand evaluator and hand names
js/engine.js            betting rounds, pots, showdown (no DOM; reusable)
js/ai.js                bot logic and Monte Carlo equity
js/ui.js                rendering, game loop, controls
server.py / server.js   tiny local servers (stdlib / no npm)
tests/run-tests.js      engine + AI tests:  node tests/run-tests.js
```

## Extending it

- **Tune a difficulty:** thresholds live in `preflop()` and `postflop()` in `js/ai.js`.
- **Add a level:** add a name to `DIFFICULTIES`, branch on it in `ai.js`, and add an `<option>` in `index.html`.
- **Debug bot decisions:** in the browser console run `JP_DEBUG = true`. Each bot action then logs with its reason and equity.
- The engine is independent of the UI (`new Poker.Game({...})`, `startHand()`, `act({type, amount})`, `viewFor(seat)`), so it can drive a multiplayer server later.
