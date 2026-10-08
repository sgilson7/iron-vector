"""The project's own browser interactions, run by check.py in every engine.

check.py has already loaded the page, waited for it to report ready, and let
it idle. These drive the game the way a person would and assert what the HUD
shows. Expected values are worked out by hand from the data files (C4), never
copied from the game's own output:

- The default legs (lg-stride) walk at 95 km/h, so after a second and a half
  of holding W the speed readout is between 90 and 96.
- Holding Space for a second from standing rises well above 10 m: the jump
  alone is 17 m/s, and the booster then climbs toward 36 m/s.
"""

import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
COPY = json.loads((ROOT / "data" / "copy.json").read_text())


def number(page, selector):
    return int(page.inner_text(selector))


def run(page, check, engine):
    check(page.inner_text("h1") == COPY["title"], "the title from data/copy.json is drawn")
    message = page.inner_text("#message")
    check(message in ("", COPY["no_webgl"]), f"no error message is shown ({message!r})")
    if message:
        print(f"  NOTE {engine} has no WebGL 2 here; the simulation is still checked")

    page.click("#start")
    page.wait_for_timeout(300)
    check(page.is_hidden("#overlay"), "starting hides the title panel")

    page.keyboard.down("KeyW")
    page.wait_for_timeout(1500)
    speed = number(page, "#speed")
    page.keyboard.up("KeyW")
    check(90 <= speed <= 96, f"holding W walks at the legs' 95 km/h (HUD shows {speed})")

    page.wait_for_timeout(800)
    page.keyboard.down("Space")
    page.wait_for_timeout(1000)
    alt = number(page, "#alt")
    page.keyboard.up("Space")
    check(alt > 10, f"holding Space ascends (HUD shows {alt} m)")

    ammo_before = page.inner_text("#w0 .ammo")
    page.mouse.move(640, 360)
    page.mouse.down()
    page.wait_for_timeout(600)
    page.mouse.up()
    ammo_after = page.inner_text("#w0 .ammo")
    check(int(ammo_after) < int(ammo_before), f"the left button fires the left-hand rifle ({ammo_before} → {ammo_after})")

    page.keyboard.press("Escape")
    page.wait_for_timeout(300)
