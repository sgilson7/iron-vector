// The page draws what the core returns and decides nothing. It loads the data
// files, starts the core through the shim, and each display frame passes the
// clock, the held actions and the mouse movement in, then draws what comes
// out. The core says which mode the game is in; the page shows that screen.
import init, { Game, numbers, mesh } from "./pkg/iv_shim.js";
import { makeRenderer } from "./gl.js";
import { makeInput } from "./input.js";
import { makeHud } from "./hud.js";
import { makeGarage } from "./garage.js";

const BUILD = "__BUILD__";
const SAVE_KEY = "iron-vector.build";
const el = (id) => document.getElementById(id);
const file = (path) => fetch(`${path}?v=${BUILD}`).then((r) => r.text());

// Browser storage can be missing or refused; the game works without it.
const storage = {
  load() {
    try {
      return localStorage.getItem(SAVE_KEY) || "";
    } catch {
      return "";
    }
  },
  save(text) {
    try {
      localStorage.setItem(SAVE_KEY, text);
    } catch {
      // nothing to do: the build is simply not remembered
    }
  },
};

function fillCopy(copy) {
  for (const node of document.querySelectorAll("[data-copy]")) {
    node.textContent = copy[node.dataset.copy];
  }
  document.title = copy.title;
  el("build").textContent = copy.build.replace("{hash}", BUILD);
  for (const table of document.querySelectorAll("table.controls")) {
    for (const [keys, does] of copy.controls) {
      const row = table.insertRow();
      row.insertCell().textContent = keys;
      row.insertCell().textContent = does;
    }
  }
  el("briefing-text").replaceChildren(
    ...copy.briefing.map((t) => {
      const p = document.createElement("p");
      p.textContent = t;
      return p;
    }),
  );
  el("objectives").replaceChildren(
    ...copy.objectives.map((t) => {
      const li = document.createElement("li");
      li.textContent = t;
      return li;
    }),
  );
}

async function start() {
  const texts = await Promise.all(
    [
      "data/copy.json",
      "data/controls.json",
      "data/parts.json",
      "data/palette.json",
      "data/missions.json",
      "data/pilots.json",
      "data/shaders/scene.vert",
      "data/shaders/scene.frag",
    ].map(file),
  );
  const [copyText, controlsText, parts, palette, missions, pilots, vertex, fragment] = texts;
  const copy = JSON.parse(copyText);
  fillCopy(copy);
  await init();
  const N = JSON.parse(numbers());

  let game;
  try {
    game = new Game(parts, palette, missions, pilots, storage.load());
  } catch (e) {
    el("message").textContent = String(e);
    document.body.dataset.ready = "true";
    return;
  }

  const canvas = el("view");
  const renderer = makeRenderer(canvas, N, { vertex, fragment }, (id) => mesh(id), game.uniforms(), game.clear_color());
  if (!renderer) el("message").textContent = copy.no_webgl;
  let sceneVersion = null;

  const hud = makeHud(copy);
  const garage = makeGarage(game, copy, (text) => storage.save(text));
  const overlay = el("overlay");
  const input = makeInput(canvas, JSON.parse(controlsText), N.actions, () => {
    overlay.hidden = true;
  });

  const screens = { garage: el("garage"), briefing: el("briefing"), debrief: el("debrief") };
  let mode = null;
  let paused = null;

  function showMode(h) {
    mode = h.mode;
    document.body.dataset.mode = mode;
    for (const [name, node] of Object.entries(screens)) node.hidden = name !== mode;
    const flying = mode === "sortie" || mode === "test";
    hud.show(flying && !h.paused);
    overlay.hidden = !(flying && h.paused);
    el("abort").hidden = !flying;
    if (!flying) input.release();
    if (mode === "garage") garage.draw(true);
    if (mode === "debrief") {
      const d = h.debrief;
      el("debrief-title").textContent = d.success ? copy.debrief_success : copy.debrief_failure;
      el("d-time").textContent = `${d.seconds} s`;
      el("d-ap").textContent = `${d.ap_kept_pct}%`;
      el("d-kills").textContent = d.kills;
      el("d-rank").textContent = d.rank;
    }
  }

  el("to-briefing").addEventListener("click", () => game.briefing());
  el("to-test").addEventListener("click", () => {
    game.test_field();
    input.engage();
  });
  el("briefing-back").addEventListener("click", () => game.to_garage());
  el("launch").addEventListener("click", () => {
    game.launch();
    input.engage();
  });
  el("retry").addEventListener("click", () => {
    game.launch();
    input.engage();
  });
  el("debrief-garage").addEventListener("click", () => game.to_garage());
  el("abort").addEventListener("click", () => game.to_garage());
  el("start").addEventListener("click", () => input.engage());
  canvas.addEventListener("click", () => {
    if ((mode === "sortie" || mode === "test") && !input.engaged()) input.engage();
  });

  function resize() {
    const [width, height] = game.resize(canvas.clientWidth, canvas.clientHeight, window.devicePixelRatio || 1);
    if (renderer) renderer.resize(width, height);
  }
  window.addEventListener("resize", resize);
  resize();

  function frame(now) {
    const { bits, dx, dy } = input.read();
    game.advance(now, bits, dx, dy);
    if (renderer && game.scene_version() !== sceneVersion) {
      sceneVersion = game.scene_version();
      renderer.setStatic(game.static_instances());
    }
    if (renderer) renderer.frame(game.view(), game.instances(), game.draws());
    const h = JSON.parse(game.hud());
    if (h.mode !== mode || h.paused !== paused) {
      paused = h.paused;
      showMode(h);
    }
    if (mode === "sortie" || mode === "test") hud.update(h);
    requestAnimationFrame(frame);
  }

  el("status").hidden = true;
  el("start").hidden = false;
  requestAnimationFrame(frame);
  document.body.dataset.ready = "true";
}

start();
