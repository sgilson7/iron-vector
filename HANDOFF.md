# Handoff

Stage 9, written by the agent. `C5 Consolidate in your own words`.

A fresh session starts from this file, not from memory.

## Where the work is

Iron Vector is live at https://sgilson7.github.io/iron-vector/ (repository `sgilson7/iron-vector`, public). A push to `main` runs `./scripts/check.sh` in Chromium on GitHub Actions and deploys `dist/` to Pages only if it passes.

- **Stage A** (the 3D MVP) shipped first, at commit `e9c3189`, build `00030648`, and passed `BUILDER_ORIGIN=https://sgilson7.github.io/iron-vector tests/browser/check.py` in Chromium, Firefox and WebKit.
- **Stage B** adds the garage (nine slots, 26 parts, stat comparison, warnings, six paint schemes, saved build), the test field, and Operation Saltline (avenue run to the relay, four gunships, one enemy frame with a behaviour-tree pilot, stagger, debrief with a rank).

## Milestone 2 (2026-10-08)

The campaign, the cockpit star map and behaviour-tree pilots. `PLAN-M2.md` is the plan; `SECOND-ORDER.md` section "Milestone 2" is the notebook. Planet one was deployed first, at Sam's mid-session request ("deploy to live whenever you have a planet completed"); planets two to five followed once each was checked by eye. New core modules: `campaign` (planets, progress, unlocks, Hasse levels), `starmap` (layout and cockpit), `pilot` (rewritten as trees), `mission` (rewritten as a data-driven engine). New data: `planets.json`, `units.json`; `pilots.json` now holds moves and trees. Generator scripts for `parts.json`, `planets.json` and `pilots.json` were used during the build and are not in the repository; edit the JSON directly.

## What changed, by layer

- `crates/core`: every rule. `fx` and `geom` are the Q16.16 maths; `mech` is movement and EN; `world` is the tick; `combat` is shots and damage; `pilot` is the enemy's behaviour tree; `mission` is the phases and the rank; `garage` is the garage screen's content; `render` is the instance list, the cameras and the HUD; `game` is the modes, the clock and the input latch.
- `crates/shim`: one call per function, integer-to-float conversion, no branches.
- `web/`: WebGL 2 renderer (`gl.js`), input (`input.js`), HUD (`hud.js`), garage screen (`garage.js`), modes (`app.js`). No number but 0 and 1.
- `data/`: parts, palette, missions, pilots, controls, copy and shaders.

## What passed

`./scripts/check.sh`: fmt, clippy with warnings as errors, 98 core tests and 11 boundary tests, release wasm build, wasm import audit, and the browser check in Chromium, Firefox and WebKit. The browser interactions walk the garage (hand-computed weights), the test field (hand-computed walking speed), pause and abort, the sortie's first objective, and a reload that keeps the chosen paint.

## What remains

- Sam's playtest (`TRIAGE.md`). Notebook rows A12 and B5 are Sam's: feel, tuning and difficulty were set by the agent and measured on a player who never moves (B4), not judged by a person.
- No audio ships. Adding sound is Sam's decision (BRIEF.md).
- The camera pulls in front of buildings but can still clip a building's edge in tight streets.
- Mechs do not collide with each other.

## What the next session must not assume

- That a feel or balance number is right because a test pins it. Tests pin current behaviour; whether it is fun is in `TRIAGE.md`.
- That CI checked Firefox and WebKit. It checks Chromium only (A15); run the three-engine check locally before pushing.
- That a scripted text replacement applied. rustfmt rewraps lines; assert every replacement (B9).
