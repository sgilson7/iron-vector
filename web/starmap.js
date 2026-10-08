// The star map screen. The hologram is drawn by the core inside the cockpit;
// this places a clickable spot where the core says each planet and mission
// is on screen, and fills the side panel from the core's star_map_info().

const el = (id) => document.getElementById(id);

function item(tag, cls, text) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined) node.textContent = text;
  return node;
}

export function plusText(copy, rule, value) {
  return copy[`plus_${rule}`].replace("{value}", value);
}

export function makeStarMap(game, copy) {
  const layer = el("hotspots");
  const pool = [];
  let info = null;

  function select(planet, mission) {
    if (mission === null) game.select_planet(planet);
    else game.select_mission(planet, mission);
    refresh();
  }

  function spot(i) {
    while (pool.length <= i) {
      const k = pool.length;
      const b = item("button", "spot");
      b.type = "button";
      b.append(item("span"));
      b.addEventListener("click", () => select(b.dataset.planet, b.dataset.mission === "" ? null : b.dataset.mission));
      b.addEventListener("mouseenter", () => game.select_hover(k));
      b.addEventListener("mouseleave", () => game.select_hover(-1));
      layer.append(b);
      pool.push(b);
    }
    return pool[i];
  }

  // Moves the spots to where the core drew the nodes this frame.
  function place() {
    const spots = JSON.parse(game.hotspots());
    spots.forEach((h, i) => {
      const b = spot(i);
      b.hidden = false;
      b.style.transform = `translate(${h.x}px, ${h.y}px)`;
      b.className = h.mission === null ? "spot planet" : "spot mission";
      const selected = info && info.planet === h.planet && (h.mission === null ? info.mission === null : info.mission === h.mission);
      b.classList.toggle("selected", Boolean(selected));
      b.dataset.planet = h.planet;
      b.dataset.mission = h.mission === null ? "" : h.mission;
      b.firstChild.textContent = h.label;
    });
    pool.slice(spots.length).forEach((b) => {
      b.hidden = true;
    });
  }

  // Rewrites the side panel from the core's view of the map.
  function refresh() {
    info = JSON.parse(game.star_map_info());
    el("map-progress").textContent = copy.progress
      .replace("{cleared}", info.cleared)
      .replace("{total}", info.total)
      .replace("{plus}", info.plus);
    const p = info.planets[info.planet];
    el("planet-name").textContent = p.name;
    el("planet-blurb").textContent = p.blurb;
    el("planet-gimmick").textContent = p.gimmick;
    el("planet-lock").textContent = p.open ? "" : copy.opens_at.replace("{n}", p.opens_at);
    el("mission-list").replaceChildren(
      ...p.missions.map((m, i) => {
        const li = item("li", m.standing);
        li.classList.toggle("selected", info.mission === i);
        const head = item("div", "m-head");
        head.append(item("span", "m-name", m.name), item("span", "m-state", `${copy[m.kind]} · ${copy[`standing_${m.standing}`]}`));
        li.append(head, item("p", "m-scene", m.scene));
        li.append(item("div", "m-reward", `${copy.reward}: ${m.reward}`));
        const plus = m.plus ? `${plusText(copy, m.plus.rule, m.plus.value)} → ${m.plus.reward}` : copy.plus_hidden;
        li.append(item("div", "m-plus", `${copy.plus_label} ${plus}`));
        if (m.standing === "locked" && m.requires.length) {
          li.append(item("div", "lock", copy.requires.replace("{list}", m.requires.join(", "))));
        }
        li.addEventListener("click", () => select(info.planet, i));
        return li;
      }),
    );
    const chosen = info.mission === null ? null : p.missions[info.mission];
    el("launch").disabled = !chosen || chosen.standing === "locked";
  }

  return {
    refresh,
    // Called every frame on the map: the spots follow the turning hologram.
    update(ready) {
      layer.hidden = !ready;
      el("map-panel").hidden = !ready;
      if (ready) place();
    },
  };
}
