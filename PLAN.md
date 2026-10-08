# Plan

Stage 2, written by the agent from `BRIEF.md`. `B2 Require step-by-step justification`.

The plan is approved in advance (BRIEF.md, Autonomy). Stage A ships and is deployed before stage B starts.

## Architecture: 3D inside the Builder defaults

Every Builder default stays in force: no floats, hash maps, clocks or randomness in the core (D2), and no `Math` or numeric literal other than 0 and 1 in the page (D3). A 3D game seems to need both, so this is how each is met.

| # | Decision | Reason | Test that shows it holds |
| --- | --- | --- | --- |
| 1 | The core does all 3D math in Q16.16 fixed point (`i32`, products in `i64`). Angles are integers, 65 536 to a turn. Sine is a Taylor series in integers; atan2 is a bounded polynomial. | D2 without loosening it; the same input gives the same frame everywhere | `fx` tests against hand-computed values (sin 30° = 32 768, sin 90° = 65 536), sin² + cos² ≈ 1 at every 256th angle |
| 2 | The core builds the view-projection matrix and one 3×4 model matrix and a colour per drawn box. The page uploads them and draws. | the page keeps no rule and no number (D3); the camera is a rule | a point in front of the camera projects inside the clip volume; a point behind it does not |
| 3 | The shim converts the core's `i32` values to `f32` by dividing by `fx::ONE`, through `iter().map` with no branch. | conversion is translation, which R5 allows | `shim_decides_nothing` |
| 4 | Vertex and fragment shaders live in `data/shaders/`, and the page fetches them. Lighting direction, fog and sky colour are uniforms from the core's `numbers()`. | GLSL has numbers in it; data is where non-rule text lives (D5) | `page_keeps_no_rule…` passes over `web/*.js` |
| 5 | Geometry is three meshes the core describes: a cube, a wedge and an octahedron. Each frame draws one instanced call per mesh. | an armoured-core look is boxes; three draw calls keep the frame cheap | the browser check runs the frame loop 10 s in three engines with no error |
| 6 | The simulation runs at a fixed 60 ticks a second inside the core. The page passes the frame timestamp, and the core accumulates time, caps the catch-up and interpolates positions between the last two ticks. | determinism, plus smooth drawing on 120 Hz displays | an `advance` of 1 000 ms runs at most the cap; alpha stays in [0, ONE] |
| 7 | Mouse look is applied in `advance`, every frame, before any tick runs. | look latency is the lag a player feels first | yaw changes on an `advance` that runs zero ticks |
| 8 | Input is a bit set. `data/controls.json` maps key codes to action names, and the core's `numbers()` maps action names to bits. | the page needs no number for an action | interactions press W and see the HUD speed rise |

## Stage A: the 3D MVP

| # | Decision | Reason | Test |
| --- | --- | --- | --- |
| 9 | Movement: walk, glide boost (toggle), quick boost (burst with cooldown), ascend (hold), gravity, landing. EN drains with thrust and recovers after a delay, and running EN out locks thrust until it refills to a threshold. | the feel the brief names, using the inspiring game's energy rhythm | `movement` tests: walk speed reached, glide faster than walk, quick boost costs EN, ascend stops at EN zero, fall lands on ground |
| 10 | Collision: the mech is an axis-aligned box resolved per axis against building boxes and the ground. | cheap, exact in integers, enough for boxes | the mech cannot pass through a building; it can stand on a roof |
| 11 | Weapons: a rifle (LMB) and a missile launcher (RMB). The lock picks the target nearest the screen centre inside a cone and a range. Missiles turn toward the lock. | the brief names destroying things; stage A needs something to shoot | a rifle shot at a dummy lowers its AP; a missile turns toward its target |
| 12 | The proving ground is a city grid made by a seeded integer generator in the core, with practice drones. | stage B reuses it for the mission map | the same seed gives the same map; no building overlaps the spawn |
| 13 | The page is a WebGL2 canvas, a DOM HUD and pointer lock. Without WebGL2 it shows a message from the copy file. | headless engines may lack WebGL; the check must not see an error | browser check in three engines |
| 14 | Deploy: GitHub Actions runs `cargo test`, builds, and publishes `dist/` to Pages. Asset URLs carry a content hash (from gear-master-2d) so a deploy is not hidden by Pages' 10-minute cache. | Sam approved Pages deploys; a stale cache looks like a broken deploy | `BUILDER_ORIGIN=https://sgilson7.github.io/iron-vector tests/browser/check.py` |

## Stage B: garage and mission

| # | Decision | Reason | Test |
| --- | --- | --- | --- |
| 15 | Parts are data (`data/parts.json`). Each part has stats and a list of boxes for its look. The core derives the frame's stats (AP, weight, EN capacity and recovery, speed, load limits) from the loadout. | Strings and tuning live in data; one rule computes the stats | stats of a hand-picked loadout equal hand-computed sums; overweight cuts speed |
| 16 | The garage is a mode of the same core: choose a slot, step through parts, see the stat deltas and a slowly turning preview. The loadout is saved in `localStorage` as a string the core validates. | browser storage is a project decision (BUILDER-SPEC); the core rejects an unknown part id | a loadout with an unknown id falls back to the default |
| 17 | The mission has phases: reach the waypoint across the city, destroy the helicopters, defeat the enemy mech. The core owns the phase, the objective text key and the result. | mission logic is a rule | a scripted run that kills every target ends in success; AP zero ends in failure |
| 18 | Helicopters circle a point, face the player and fire bursts. The enemy mech runs a small behaviour tree (from the CSC 584 lecture material and vagrancy's `pilot`): keep its range, strafe, quick boost when a missile is near, fire when it has line of sight. | "super simple" enemy, and a readable AI whose decisions can be tested | each tree branch is chosen in the state built for it |

## Open questions

None block stage A. The answers chosen so far are recorded in `SECOND-ORDER.md`.

## Status

- [x] Approved in advance by Sam (BRIEF.md, 2026-10-08).
- [x] Stage A deployed 2026-10-08 (e9c3189).
- [x] Stage B built; deployed with the commit that follows this line.
