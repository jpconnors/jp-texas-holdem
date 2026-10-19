# JP's Texas Hold'em

No-limit Texas Hold'em in the browser: you against 1–5 virtual opponents at Easy, Medium, Hard, or a mixed table. The game rules and the bots are written in **Rust** and compiled to **WebAssembly**. The table is plain HTML/CSS/JavaScript. You don't need Rust to play, because the compiled engine is committed to the repo (`js/holdem-wasm.js`).

## Run it

**Mac:** the simplest option is to double-click `index.html`, since the game runs fully in the browser. To use the launcher instead, double-click `Start (Mac).command`. macOS will say it "can't verify" the file because it came from the internet. Dismiss that warning, go to **System Settings → Privacy & Security**, and click **Open Anyway** next to the message. Or clear the quarantine flag once in Terminal:

```
xattr -dr com.apple.quarantine path/to/jp-texas-holdem
```

You can also skip the launcher and run the server from Terminal:

```
cd path/to/jp-texas-holdem
python3 server.py
```

**Windows:** double-click `Start (Windows).bat`. Or in a terminal: `py server.py` (or `node server.js`).

Either launcher starts a local server at `http://127.0.0.1:8765/` (next free port if that one is taken) and opens your browser. Close the terminal window to stop it. If neither Python 3 nor Node is installed, the launcher opens `index.html` directly. The game works that way too, because the WebAssembly is embedded in a script file rather than downloaded.

## How to play

1. Pick your name, number of opponents, difficulty, stack, blinds, and bot speed, then **Deal me in**.
2. Click **I'm Ready** (or press Space) to deal each hand.
3. On your turn, use **Fold / Check / Call / Bet**. The box next to *Money* is the total you're betting or raising **to**. The presets fill it in: **All in**, **.5x / 1x / 2x** pot-sized raises, **MIN** legal raise.

Keyboard: `F` fold · `C` check or call · `K` check · `B` or `Enter` bet/raise · `Space` deal · `H` hand history.

## The opponents

| Level | How it plays |
|---|---|
| **Easy** | Loose and passive. Plays most hands, calls too much, rarely raises, judges its hand with a lot of noise. |
| **Medium** | Runs a Monte Carlo simulation of its equity (1,500 run-outs) against the players still in, compares it to the pot odds, value-bets good hands, and bluffs occasionally. |
| **Hard** | Simulates 12,000 run-outs against a modeled range for each opponent, built from their VPIP/PFR history and how they've acted this hand. It uses positional opening ranges, 3-bets (wider against loose raisers), push/fold play when short-stacked, continuation bets, semi-bluffs, check-raise traps, and implied odds. It tracks your tendencies and adjusts: more bluffs if you fold to bets often, thinner value bets and overbets if you call too much. |

Bots see only what a real player would: their own cards, the board, bets, stacks, and public stats. No peeking. The page itself never receives opponents' cards until they're shown down.

Each bot also gets a small random personality (looser or tighter, more or less aggressive), so two bots at the same level don't play identically.

In 40 heads-up freezeouts per matchup (the `difficulty_ladder` test): Hard beat Medium 30–10, Hard beat Easy 27–13, and Medium beat Easy 28–12.

## Rules implemented

- Dealer button rotation, small and big blinds, heads-up blind rules (button posts the small blind and acts first preflop)
- Big blind option when everyone limps
- Minimum raise equal to the last full raise. A short all-in raise does not reopen betting for players who already acted.
- All-ins with automatic run-out, side pots, return of uncalled bets, split pots (odd chip to the first winner left of the button)
- Optional rising blinds (tournament style), player elimination, game-over standings, and a win/loss record per difficulty saved in the browser

## Project layout

```
index.html                 table markup and dialogs
css/style.css              look and layout
js/ui.js                   rendering, game loop, controls
js/wasm-game.js            loads the WebAssembly engine; exposes it as Poker.Game
js/holdem-wasm.js          the compiled engine (GENERATED, embedded as base64)
js/cards.js                card display helpers
engine/                    Rust crate `holdem-core`
  src/game.rs              betting rounds, min-raise rules, side pots, showdown
  src/eval.rs              7-card hand evaluator and hand names
  src/ai.rs                bot logic and Monte Carlo equity
  src/api.rs               JSON commands the page sends (new, start, act, bot, ...)
  src/lib.rs               the three wasm exports
  tests/                   rules, evaluator, bot tournaments, JSON API
tools/build-wasm.sh        compile to wasm + embed
.github/workflows/         CI: test, build, smoke-test, commit js/holdem-wasm.js
server.py / server.js      tiny local servers (stdlib / no npm)
```

## Rebuilding the engine

You only need this if you change the Rust code. Pushing to GitHub does it for you: the **Build engine** workflow runs the tests, compiles to WebAssembly, checks the build in Node, and commits the new `js/holdem-wasm.js` to your branch. After it finishes, pull.

To build locally, install Rust from rustup.rs and Node.js, then:

```
rustup target add wasm32-unknown-unknown
cargo test --release --manifest-path engine/Cargo.toml     # rules + bot tests
bash tools/build-wasm.sh                                    # writes js/holdem-wasm.js
node tests/wasm-smoke.js                                    # plays games through the wasm build
```

## Extending it

- **Tune a difficulty:** thresholds live in `preflop()` and `postflop()` in `engine/src/ai.rs`; simulation counts are in `Difficulty::sims()`.
- **Add a level:** add a variant to `Difficulty` in `ai.rs`, handle it in `preflop()`/`postflop()`, and add an `<option>` in `index.html`.
- **Debug bot decisions:** in the browser console run `JP_DEBUG = true`. Each bot action then logs with its reason and equity.
- **Use the engine elsewhere:** `holdem-core` has no browser dependencies. The same crate could back a native app or a multiplayer server.
