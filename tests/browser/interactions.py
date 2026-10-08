"""The project's own browser interactions, run by check.py in every engine.

check.py has already loaded the page, waited for it to report ready, and let
it idle. These drive the game the way a person would and assert what the page
shows. Expected values are worked out by hand from the data files (C4), never
copied from the game's own output:

- The default frame weighs 79 100 kg: everything but the legs is
  3 000 + 21 000 + 11 000 + 1 600 + 5 500 + 5 000 + 4 200 + 7 800 = 59 100,
  plus the LG-20 STRIDE legs at 20 000. Hovering LG-08 WISP (13 000) shows
  59 100 + 13 000 = 72 100. LG-08 WISP is won by the plus challenge of
  Halden's race, so a new player cannot fit it.
- The default legs walk at 95 km/h, so after a second and a half of holding
  W in the test field the speed readout is between 90 and 96.
- The star map has 5 planets and 20 missions: 25 places to click. The second
  planet, Sere, opens when 2 missions are cleared.
"""

import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
COPY = json.loads((ROOT / "data" / "copy.json").read_text())
SAVE_KEY = "iron-vector.build"


def number(page, selector):
    return int(page.inner_text(selector))


def stat_row(page, key):
    """The cells of the garage stats row labelled with copy[key]."""
    for row in page.query_selector_all("#stats tr"):
        cells = [c.inner_text() for c in row.query_selector_all("td")]
        if cells and cells[0] == COPY[key]:
            return cells
    return None


def open_map(page):
    page.click("#to-map")
    # the page's CSP refuses evaluated strings, so wait on a selector instead
    page.wait_for_selector(".spot.planet:not([hidden])", timeout=8000)
    page.wait_for_timeout(200)


def run(page, check, engine):
    check(page.inner_text("#garage h1") == COPY["title"], "the title from data/copy.json is drawn")
    message = page.inner_text("#message")
    check(message in ("", COPY["no_webgl"]), f"no error message is shown ({message!r})")
    check(page.is_visible("#garage"), "the game opens in the garage")

    # garage: compare a locked part, fail to fit it, paint
    weight = stat_row(page, "stat_weight")
    check(weight and weight[1] == "79100", f"the default frame weighs 79100 ({weight})")
    page.click("#slots li:nth-child(7)")
    check(page.inner_text("#slots li.selected .k") == COPY["slot_legs"], "clicking a slot selects it")
    page.hover("#parts li:nth-child(2)")
    weight = stat_row(page, "stat_weight")
    check(weight and weight[2] == "→ 72100", f"hovering lighter legs shows the weight they would give ({weight})")
    check(page.is_visible("#parts li:nth-child(2).locked"), "a part not yet won is marked locked")
    check("SHAKEDOWN RUN" in page.inner_text("#detail-lock"), "the card says which mission wins it")
    page.click("#parts li:nth-child(2)")
    check(page.inner_text("#slots li.selected .v") == "LG-20 STRIDE", "a locked part cannot be fitted")
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
    ammo_before = page.inner_text("#w0 .ammo")
    page.mouse.move(640, 360)
    page.mouse.down()
    page.wait_for_timeout(600)
    page.mouse.up()
    check(int(page.inner_text("#w0 .ammo")) < int(ammo_before), "the left button fires the left-hand rifle")
    page.keyboard.press("Escape")
    page.wait_for_timeout(300)
    check(page.is_visible("#overlay") and page.is_visible("#abort"), "Esc pauses and offers to abort")
    page.click("#abort")
    page.wait_for_timeout(300)
    check(page.is_visible("#garage"), "aborting returns to the garage")

    # star map
    open_map(page)
    spots = len(page.query_selector_all(".spot:not([hidden])"))
    check(spots == 25, f"the map shows 5 planets and 20 missions ({spots})")
    page.click(".spot.planet >> nth=1")
    check(page.inner_text("#planet-name") == "SERE", "clicking a planet shows it")
    check(page.inner_text("#planet-lock") == COPY["opens_at"].replace("{n}", "2"), "Sere opens after two clears")
    page.click(".spot.planet >> nth=0")
    page.click("#mission-list li:nth-child(3)")
    check(page.is_disabled("#launch"), "a mission whose requirement is not met cannot launch")
    page.click("#mission-list li:nth-child(1)")
    check(page.is_enabled("#launch"), "an open mission can launch")
    page.click("#launch")
    page.wait_for_timeout(600)
    check(page.inner_text("#objective") == COPY["obj_destroy"], "the first mission is to destroy the gunships")
    check(page.inner_text("#c-hostiles b") == "2", "two gunships")
    page.keyboard.press("Escape")
    page.wait_for_timeout(200)
    page.click("#abort")
    page.wait_for_timeout(200)

    # the build is remembered; two clears open the second planet
    page.evaluate(
        "([k, extra]) => { const s = JSON.parse(localStorage.getItem(k)); s.progress = extra; localStorage.setItem(k, JSON.stringify(s)); }",
        [SAVE_KEY, {"cleared": ["halden-1", "halden-2"], "plus": ["halden-2"]}],
    )
    page.reload()
    page.wait_for_selector("body[data-ready='true']", timeout=20000)
    page.wait_for_timeout(300)
    check(page.is_visible("#paints .paint:nth-child(3).selected"), "the chosen paint is remembered after a reload")
    page.click("#slots li:nth-child(7)")
    # LG-08 WISP is the reward for the race's plus challenge (data/planets.json)
    check(not page.is_visible("#parts li:nth-child(2).locked"), "meeting the race's plus unlocked its legs")
    open_map(page)
    page.click(".spot.planet >> nth=1")
    check(page.inner_text("#planet-lock") == "", "after two clears Sere is open")
    page.click("#mission-list li:nth-child(1)")
    check(page.is_enabled("#launch"), "Sere's first mission can launch")
