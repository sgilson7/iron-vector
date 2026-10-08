// The page draws what the core returns and decides nothing. It loads the data
// files, starts the core through the shim, and each display frame passes the
// clock, the held actions and the mouse movement in, then draws what comes out.
import init, { Game, numbers, mesh } from "./pkg/iv_shim.js";
import { makeRenderer } from "./gl.js";
import { makeInput } from "./input.js";
import { makeHud } from "./hud.js";

const BUILD = "__BUILD__";
const el = (id) => document.getElementById(id);
const file = (path) => fetch(`${path}?v=${BUILD}`).then((r) => r.text());

function fillCopy(copy) {
  for (const node of document.querySelectorAll("[data-copy]")) {
    node.textContent = copy[node.dataset.copy];
  }
  document.title = copy.title;
  el("build").textContent = copy.build.replace("{hash}", BUILD);
  const table = el("controls");
  for (const [keys, does] of copy.controls) {
    const row = table.insertRow();
    row.insertCell().textContent = keys;
    row.insertCell().textContent = does;
  }
}

async function start() {
  const [copyText, controlsText, parts, palette, missions, vertex, fragment] = await Promise.all([
    file("data/copy.json"),
    file("data/controls.json"),
    file("data/parts.json"),
    file("data/palette.json"),
    file("data/missions.json"),
    file("data/shaders/scene.vert"),
    file("data/shaders/scene.frag"),
  ]);
  const copy = JSON.parse(copyText);
  fillCopy(copy);
  await init();
  const N = JSON.parse(numbers());

  let game;
  try {
    game = new Game(parts, palette, missions, "");
  } catch (e) {
    el("message").textContent = String(e);
    document.body.dataset.ready = "true";
    return;
  }

  const canvas = el("view");
  const renderer = makeRenderer(
    canvas,
    N,
    { vertex, fragment },
    (id) => mesh(id),
    game.uniforms(),
    game.clear_color(),
  );
  if (!renderer) el("message").textContent = copy.no_webgl;
  if (renderer) renderer.setStatic(game.static_instances());

  const hud = makeHud(copy);
  const overlay = el("overlay");
  const startButton = el("start");
  const input = makeInput(canvas, JSON.parse(controlsText), N.actions, () => {
    overlay.hidden = true;
  });
  startButton.addEventListener("click", () => input.engage());
  canvas.addEventListener("click", () => {
    if (!input.engaged()) input.engage();
  });

  function resize() {
    const [width, height] = game.resize(canvas.clientWidth, canvas.clientHeight, window.devicePixelRatio || 1);
    if (renderer) renderer.resize(width, height);
  }
  window.addEventListener("resize", resize);
  resize();

  let wasPaused = true;
  function frame(now) {
    const { bits, dx, dy } = input.read();
    game.advance(now, bits, dx, dy);
    if (renderer) renderer.frame(game.view(), game.instances(), game.draws());
    const h = JSON.parse(game.hud());
    hud.update(h);
    if (h.paused !== wasPaused) {
      wasPaused = h.paused;
      overlay.hidden = !h.paused;
      hud.show(!h.paused);
      startButton.textContent = copy.resume;
    }
    requestAnimationFrame(frame);
  }

  el("status").hidden = true;
  startButton.hidden = false;
  requestAnimationFrame(frame);
  document.body.dataset.ready = "true";
}

start();
