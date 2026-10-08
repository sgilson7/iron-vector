# Iron Vector

A single-player 3D mech game that runs in the browser. Build a frame from parts, take it into the city, and fly it.

**Play:** https://sgilson7.github.io/iron-vector/

- **Garage.** Nine slots and 49 parts. You start with nine; the rest are won on missions. Hover any part to see it on your frame and compare every stat; locked parts say which mission wins them.
- **Star map.** The camera sweeps into the cockpit, the lights come up, and a hologram of five planets and their missions grows over the console, laid out as a Hasse diagram by requirement.
- **Five planets, twenty missions.** Halden (industrial city), Sere (desert mesas in a sandstorm), Spindle (a ring world: gravity throws you outward and the floor curves up overhead), Rime (an ice moon: low gravity, no grip) and Cinder (the floor is lava). Each mission gives a part, and reveals a + challenge after the first clear that gives another. Planets open at 2, 5, 8 and 11 clears.
- **Enemy frames** fly behaviour trees after vagrancy's: rules of conditions and moves made of key beats, committed to and interruptible, seen through a reaction delay. The HUD shows what each is doing.
- **Test field.** Practice drones on Halden.

## Controls

| Input | Does |
| --- | --- |
| W A S D | Move |
| Mouse | Look and aim |
| Space (hold) | Jump, then ascend (uses EN) |
| Shift | Quick boost (uses EN) |
| C | Glide boost on or off |
| Left / right button | Left-hand / right-hand weapon |
| E or F | Shoulder weapon |
| Esc | Pause and release the mouse |

## How it is built

Iron Vector is a [Builder](https://github.com/sgilson7/builder-setting) project.

- **`crates/core`** holds every rule: movement, EN, collision, weapons, lock-on, the camera and the matrices the page draws with. It uses no floats, so all of the 3D maths is Q16.16 fixed point.
- **`crates/shim`** moves values across the WebAssembly boundary. It converts the core's integers to the `f32` WebGL wants and decides nothing.
- **`web/`** is a static page with a WebGL 2 canvas. It uploads what the core returns and draws it, with one instanced draw per mesh.
- **`data/`** holds every part, colour, scenario, pilot, string and shader.

The enemy frame's pilot (`crates/core/src/pilot.rs`, rules in `data/pilots.json`) returns only the controls a player could press, and its frame moves by the same rule as yours.

The simulation runs at a fixed 60 ticks a second inside the core and is drawn at the display's rate, interpolated between ticks. Mouse look is applied on every frame, before any tick runs.

```bash
./scripts/setup.sh   # once: Playwright and three browser engines
./scripts/check.sh   # format, lint, tests, wasm build, import audit, browser check
python3 scripts/serve.py   # http://127.0.0.1:8000
```

`BRIEF.md` holds the request, `PLAN.md` the decisions, `SECOND-ORDER.md` what was found while building, and `HANDOFF.md` where the work stands.

## Licence

MIT. See `LICENSE`.
