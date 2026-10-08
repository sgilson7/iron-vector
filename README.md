# Iron Vector

A single-player 3D mech game that runs in the browser. Build a frame from parts, take it into the city, and fly it.

**Play:** https://sgilson7.github.io/iron-vector/

Stage A, live now, is a proving ground. A city of a few hundred buildings and a dozen practice drones, and one mech that walks, glide-boosts, quick-boosts, ascends on EN, locks on and fires a rifle, missiles and a shoulder cannon. Stage B adds the garage and a mission.

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
| Esc | Release the mouse |

## How it is built

Iron Vector is a [Builder](https://github.com/sgilson7/builder-setting) project.

- **`crates/core`** holds every rule: movement, EN, collision, weapons, lock-on, the camera and the matrices the page draws with. It uses no floats, so all of the 3D maths is Q16.16 fixed point.
- **`crates/shim`** moves values across the WebAssembly boundary. It converts the core's integers to the `f32` WebGL wants and decides nothing.
- **`web/`** is a static page with a WebGL 2 canvas. It uploads what the core returns and draws it, with one instanced draw per mesh.
- **`data/`** holds every part, colour, scenario, string and shader.

The simulation runs at a fixed 60 ticks a second inside the core and is drawn at the display's rate, interpolated between ticks. Mouse look is applied on every frame, before any tick runs.

```bash
./scripts/setup.sh   # once: Playwright and three browser engines
./scripts/check.sh   # format, lint, tests, wasm build, import audit, browser check
python3 scripts/serve.py   # http://127.0.0.1:8000
```

`BRIEF.md` holds the request, `PLAN.md` the decisions, `SECOND-ORDER.md` what was found while building, and `HANDOFF.md` where the work stands.

## Licence

MIT. See `LICENSE`.
