"""The project's own browser interactions, run by check.py in every engine.

check.py has already loaded the page, waited for it to report ready, and let
it idle. These drive the game the way a person would and assert what the page
shows. Expected values are worked out by hand from the data files (C4), never
copied from the game's own output:

- The default frame weighs 79 100 kg: everything but the legs is
  3 000 + 21 000 + 11 000 + 1 600 + 5 500 + 5 000 + 4 200 + 7 800 = 59 100,
  plus the LG-20 STRIDE legs at 20 000. Hovering LG-08 WISP (13 000) in the
  legs slot shows the weight going to 59 100 + 13 000 = 72 100.
- The default legs walk at 95 km/h, so after a second and a half of holding
  W in the test field the speed readout is between 90 and 96.
- Holding Space for a second from standing rises well above 10 m: the jump
  alone is 17 m/s, and the booster then climbs toward 36 m/s.
"""

import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
COPY = json.loads((ROOT / "data" / "copy.json").read_text())


def number(page, selector):
    return int(page.inner_text(selector))


def stat_row(page, key):
    """The cells of the garage stats row labelled with copy[key]."""
    for row in page.query_selector_all("#stats tr"):
        cells = [c.inner_text() for c in row.query_selector_all("td")]
        if cells and cells[0] == COPY[key]:
            return cells
    return None


def run(page, check, engine):
    check(page.inner_text("#garage h1") == COPY["title"], "the title from data/copy.json is drawn")
    message = page.inner_text("#message")
    check(message in ("", COPY["no_webgl"]), f"no error message is shown ({message!r})")
    if message:
        print(f"  NOTE {engine} has no WebGL 2 here; the simulation is still checked")
    check(page.is_visible("#garage"), "the game opens in the garage")

    # garage: compare, then paint
    weight = stat_row(page, "stat_weight")
    check(weight and weight[1] == "79100", f"the default frame weighs 79100 ({weight})")
    page.click("#slots li:nth-child(7)")
    check(page.inner_text("#slots li.selected .k") == COPY["slot_legs"], "clicking a slot selects it")
    page.hover("#parts li:nth-child(2)")
    weight = stat_row(page, "stat_weight")
    check(weight and weight[2] == "→ 72100", f"hovering lighter legs shows the weight they would give ({weight})")
    check("down" not in (page.get_attribute("#stats tr:nth-child(2) td:nth-child(3)", "class") or ""),
          "less weight is not marked worse")
    page.click("#paints .paint:nth-child(3)")
    check(page.is_visible("#paints .paint:nth-child(3).selected"), "a paint scheme can be chosen")

    # test field
    page.click("#to-test")
    page.wait_for_timeout(300)
    check(page.is_hidden("#garage") and page.is_hidden("#overlay"), "the test field starts at once")
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

    # pause, abort, sortie
    page.keyboard.press("Escape")
    page.wait_for_timeout(300)
    check(page.is_visible("#overlay") and page.is_visible("#abort"), "Esc pauses and offers to abort")
    page.click("#abort")
    page.wait_for_timeout(300)
    check(page.is_visible("#garage"), "aborting returns to the garage")
    page.click("#to-briefing")
    check(page.inner_text("#briefing h1") == COPY["briefing_title"], "the briefing shows the operation")
    page.click("#launch")
    page.wait_for_timeout(600)
    check(page.inner_text("#objective") == COPY["obj_advance"], "the sortie starts on its first objective")
    check(page.is_visible("#waypoint"), "the waypoint marker is shown")
    page.keyboard.press("Escape")
    page.wait_for_timeout(200)
    page.click("#abort")
    page.wait_for_timeout(200)

    # the build is remembered across a reload
    page.reload()
    page.wait_for_selector("body[data-ready='true']", timeout=20000)
    page.wait_for_timeout(300)
    check(page.is_visible("#paints .paint:nth-child(3).selected"), "the chosen paint is remembered after a reload")
