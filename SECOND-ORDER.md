# Second-order notebook

Stage 6, kept by the agent as the work happens, not afterwards. `B3 Build in self-questioning`.

One row per assumption, surprise or open item. **Kind** is *divergence* (what was assumed, what turned out to be true, what changed), *finding*, or *worklist* (the person's to do). **Status** is *done*, *open*, or *person*.

## Stage A: the 3D MVP

| # | Kind | What was assumed or found | What is true | What changed | Status |
| --- | --- | --- | --- | --- | --- |
| A1 | finding | A 3D game would need floats in the core and `Math` in the page, which would break D2 and D3. | Neither is needed. The core does Q16.16 fixed point with an integer Taylor sine, builds the view-projection and per-box matrices, and the shim divides by `fx::ONE` on the way out. The page holds no number but 0 and 1. | No Builder default was changed. The table at the end of `BUILDER-SPEC.md` stays empty. | done |
| A2 | finding | The page scanner reads every file in `web/`. | It reads only files directly in `web/`, and it strips strings and template literals before scanning. Shaders contain numbers, so they live in `data/shaders/` and the page fetches them. | Shaders are data; lighting, fog and grid values reach them as uniforms from the core. | done |
| A3 | finding | `time`, `io`, `env`, `process` and `print` are forbidden identifiers in the core (the clock and I/O scan). | They are. Nothing in the core may be named `time`, so the tick counter is `tick` and the clock lives in the page and arrives as an argument. | Naming only. | done |
| A4 | divergence | Mouse movement could cross the boundary as whole pixels. | Firefox and high-DPI displays report fractional `movementX`, and `as i32` truncates every one of them toward zero, so slow aiming would not move. | The shim sends hundredths of a pixel; the core divides by 100 after scaling. | done |
| A5 | finding | A key tapped and released between two ticks would be lost at 144 Hz. | `advance` latches newly pressed bits until the next tick consumes them. Test: `a_tap_shorter_than_a_tick_still_quick_boosts`. Planted defect (latch removed): that test fails. | | done |
| A6 | divergence | `walking_reaches_the_legs_walking_speed_and_no_more` checked the walking speed. | It compared against `kmh(95)`, the same conversion it was meant to check, so a wrong conversion (÷200 instead of ÷216) passed it. Only the HUD test caught the planted defect. | It now compares against 28 823, worked by hand (95 km/h = 0.43981 m/tick). The planted defect fails both tests. | done |
| A7 | divergence | The lock's line-of-sight rule was covered. | Removing `clear_line` from `update_lock` failed no test. | Added `a_drone_behind_a_building_is_not_locked`; the planted defect now fails it. | done |
| A8 | finding | Default arm weapons: rifle right, missiles left. | The inspiring game puts each hand on its own mouse button, and players expect the primary gun on the left button. | Left hand holds the rifle (left button) and the right hand the missiles (right button). | done |
| A9 | finding | Pointer lock always works after a click. | It can fail in an embedded frame or a headless engine, and a rejected promise would log a console error. | `input.js` tries raw (unadjusted) pointer lock, then plain pointer lock, then a fallback that takes the keyboard and reads `movementX` without capture. Every rejection is caught. | done |
| A10 | finding | The release profile was `opt-level = "s"` (template). | The frame loop builds about 1 600 instances a frame in wasm; speed matters more than 50 KB. | `opt-level = 3`, `lto`, one codegen unit. This is a cargo profile, not a Builder default. | done |
| A11 | finding | Pages deploys are the person's step (R9). | Sam approved repository creation and Pages deploys in advance in the request. | `.github/workflows/deploy.yml` runs `./scripts/check.sh` and deploys only after it passes. Recorded in `BRIEF.md`. | done |
| A12 | worklist | Feel and tuning (speeds, EN, gravity, mouse sensitivity) were set by the agent from the inspiring game's rhythm, not playtested by a person. | | Sam: try `https://sgilson7.github.io/iron-vector/` and write rows in `TRIAGE.md`. | person |
| A13 | finding | The first screenshots showed buildings and the mech too dark, the mech in the middle of the crosshair, and the standing shadow as a dark square. | Ambient light was too low, the camera pivot sat at chest height, and the shadow was 5 m wide and nearly black. | Ambient raised, the pivot moved 3 m above the chest, and the shadow made smaller and lighter. | done |
| A14 | finding | The camera could end up inside a building. | `camera_eye` pulls the camera in front of any building between the pivot and the eye. Test: `the_camera_is_pulled_in_front_of_a_wall_behind_the_mech`. | | done |
| A15 | divergence | CI could install all three browser engines with their system packages, as the Builder reference workflow does. | The first run sat in that step for over 15 minutes and was cancelled. | CI installs and checks Chromium only, and gates the deploy on that. The three-engine check is run locally before each push. | done |

## Stage B: garage and mission

| # | Kind | What was assumed or found | What is true | What changed | Status |
| --- | --- | --- | --- | --- | --- |
| B1 | finding | The enemy frame needs its own movement rules. | It does not. Following vagrancy's `pilot` crate, the enemy pilot returns only `Controls` and an aim, and its frame moves by `Body::step` like the player's. It can do nothing the player cannot. | `pilot.rs`; the tree's rules and timings are in `data/pilots.json`. | done |
| B2 | finding | The enemy's behaviour tree should be visible to the player. | Showing the chosen action ("intent: strafing", "evading") under the enemy's AP bar makes the AI legible, which is the point of the course material on game AI. | `BossHud.intent`; each action has a copy key. | done |
| B3 | divergence | The pilot's aim error made the enemy miss. | `World::fire` led every locked shot onto the target exactly, so the error moved the arms and not the shots. A stationary player went from 9 462 AP to 0 in about 10 s once in range. | A pilot's direct fire now goes where it aims; only missiles home. The player keeps the lock's lead (the inspiring game's hard lock). | done |
| B4 | finding | Balance was set by hand. | Measured on a player who never moves: gunships take about 830 AP in 20 s; the enemy frame (aim error 5°, 0.9 s bursts, missiles every 5.5 s) takes about 9 000 AP in 25 s of fighting, mostly from missiles, which a moving player can dodge. | Numbers in `data/pilots.json` and `data/missions.json`. | done |
| B5 | worklist | Whether the mission is fun, too hard or too easy needs a person playing it. | | Sam: play the sortie and note the result and rank in `TRIAGE.md`. | person |
| B6 | finding | Ammo was spent per shot. | A shotgun's eight pellets would spend eight rounds. | Ammo counts one a trigger pull, except missiles, which count one each. | done |
| B7 | finding | The garage could keep a second copy of the stats. | The page shows only `GarageView`, built in `garage.rs`; better or worse is decided per stat there (`SHOWN`). Test: `hovering_a_lighter_head_shows_less_weight_as_better`. | | done |
| B8 | divergence | A saved build would be the loadout alone. | The paint scheme is saved too, as `{"loadout", "paint"}`. A save that does not parse, or names a part that no longer exists, falls back slot by slot to the default. | `Game::saved`, `Loadout::restore`. | done |
| B9 | finding | rustfmt rewrapped lines between edits, so several scripted replacements silently missed and the build broke. | Replacements now assert that they matched, and `rustfmt.toml` sets `max_width = 110`. | | done |

## Milestone 2: campaign, cockpit star map, behaviour trees

| # | Kind | What was assumed or found | What is true | What changed | Status |
| --- | --- | --- | --- | --- | --- |
| C1 | finding | The planet's environment could be a field called `env`. | The boundary test bans `env` in the core (it is how `std::env` is named); the test failed on first compile. | Renamed to `climate`, in code and in `data/planets.json`. The ban stays. | done |
| C2 | divergence | "Each planet a fixed map" could mean hand-built maps. | Five seeded generator specs with a style each (towers, mesas, ruins, spires, pillars) give fixed, distinct maps; mission pads and protected structures are set down by hand on top. | `MapSpec.style`, `MissionSpec.pads`, `protect`. | done |
| C3 | finding | A ring world needs its own physics. | It does not: the simulation runs on the unrolled floor with x wrapping, and only drawing bends it (`render::Warp`). Gravity "outward" is simply the flat floor's down. Distances near the seam use `World::map_delta`. | Tests: `on_a_ring_the_floor_across_the_ring_is_overhead`, `on_a_ring_walking_off_one_edge_comes_back_on_the_other`. | done |
| C4 | divergence | The plus challenge could be met on the first clear. | The request says the plus is revealed after the first clear, so a first clear never also wins the plus; the second run can. | `Progress::record`; test `a_clear_gives_the_reward_and_reveals_the_plus_which_a_later_run_can_win`. | done |
| C5 | finding | Planet unlocks continue the request's 2, 5, 8 by +3. | Planet five opens at 11 clears. | `opens_at` in `data/planets.json`. | done |
| C6 | divergence | An interrupt rule may break into any running move (vagrancy). | Two dodge rules interrupting each other kept restarting quick boosts. | An interrupt breaks in only for a rule of higher priority than the running one. | done |
| C7 | finding | The browser check could wait with `page.wait_for_function("…")`. | The page's own CSP forbids evaluated strings, so Playwright's string wait is refused. | The check waits on a selector. The CSP stays. | done |
| C8 | divergence | My browser test expected Halden's race to unlock LG-08 WISP. | That part is the race's plus reward; the plain reward is AR-02 REED. The test was wrong, not the game. | The test now records the plus, and its docstring says why. | done |
| C9 | finding | Planets 2–5 needed checking by eye, not just by the core tests. | Checked in Chromium screenshots: Glass Nest started facing away from the nest (heading 180° faces +z), the ring's floor seam glared, and the final ace took a third of the player's AP in three seconds with the THRESHER. | Glass Nest faces the nest; the seam is dimmer; the ace carries the MARROW and aims a little wider. | done |
| C10 | worklist | Difficulty of 20 missions is set by the agent. | | Sam's playtest, `TRIAGE.md`. | person |
