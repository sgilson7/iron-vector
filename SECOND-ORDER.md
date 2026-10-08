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
