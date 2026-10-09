// The heads-up display: it writes what the core's hud() reports into the
// page, touching an element only when its value changed.
import { plusText } from "./starmap.js";

const el = (id) => document.getElementById(id);

function setter() {
  const last = new Map();
  return (node, key, value, apply) => {
    if (last.get(key) === value) return;
    last.set(key, value);
    apply(node, value);
  };
}

const text = (node, v) => {
  node.textContent = v;
};
const pct = (node, v) => {
  node.style.setProperty("--pct", v);
};
const show = (node, v) => {
  node.hidden = !v;
};
const cls = (name) => (node, v) => node.classList.toggle(name, v);

function item(tag, cls, content) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (content !== undefined) node.textContent = content;
  return node;
}

export function makeHud(copy) {
  const set = setter();
  const q = (sel) => document.querySelector(sel);
  const nodes = {
    hud: el("hud"),
    speed: el("speed"),
    alt: el("alt"),
    kills: el("kills"),
    glide: el("glide"),
    burning: el("burning"),
    ap: el("ap"),
    apBar: el("ap-bar"),
    enBar: el("en-bar"),
    enOut: el("en-out"),
    qb: el("qb"),
    marks: el("marks"),
    waypoint: el("waypoint"),
    wpLabel: q("#waypoint .wp-label"),
    wpDist: q("#waypoint .wp-dist"),
    missionBox: el("mission-box"),
    missionName: el("mission-name"),
    objective: el("objective"),
    hostiles: q("#c-hostiles b"),
    structuresBox: el("c-structures"),
    structures: q("#c-structures b"),
    raceBox: el("c-race"),
    gate: q("#c-race .gate"),
    place: q("#c-race .place"),
    time: q("#c-time b"),
    leftBox: el("c-left"),
    left: q("#c-left b"),
    plus: el("plus-track"),
    frames: el("frames"),
    stagger: el("stagger-bar"),
    staggered: el("staggered"),
    weapons: ["w0", "w1", "w2"].map((id) => {
      const root = el(id);
      return { root, name: root.querySelector(".wname"), ammo: root.querySelector(".ammo"), bar: root.querySelector(".bar") };
    }),
  };
  const markPool = [];
  const framePool = [];

  function marks(list) {
    while (markPool.length < list.length) {
      const m = item("div", "mark");
      m.append(item("span"));
      nodes.marks.append(m);
      markPool.push(m);
    }
    markPool.forEach((m, i) => {
      const k = list[i];
      m.hidden = !k;
      if (!k) return;
      m.style.transform = `translate(${k.x}px, ${k.y}px)`;
      m.classList.toggle("locked", k.locked);
      m.firstChild.textContent = `${k.dist}${copy.metres}  ${copy.ap} ${k.ap_pct}%`;
    });
  }

  function frames(list) {
    while (framePool.length < list.length) {
      const row = item("div", "frame-row");
      const head = item("div", "head");
      head.append(item("span", "name"), item("span", "intent"));
      const ap = item("div", "bar ap");
      ap.append(item("i"));
      const st = item("div", "bar thin stagger");
      st.append(item("i"));
      row.append(head, ap, st);
      nodes.frames.append(row);
      framePool.push({ row, name: head.firstChild, intent: head.lastChild, ap, st });
    }
    framePool.forEach((f, i) => {
      const k = list[i];
      f.row.hidden = !k;
      if (!k) return;
      f.row.classList.toggle("racer", k.racer);
      f.name.textContent = k.name;
      f.intent.textContent = k.intent ? `${copy.ai_caption}: ${k.intent}` : "";
      f.ap.style.setProperty("--pct", k.ap_pct);
      f.st.style.setProperty("--pct", k.stagger_pct);
      f.st.classList.toggle("full", k.staggered);
      f.st.hidden = k.racer;
    });
  }

  function mission(m) {
    set(nodes.missionBox, "hasMission", Boolean(m), show);
    if (!m) return;
    set(nodes.missionName, "mName", m.name, text);
    // a staged mission names its own step; others say what their kind asks
    const objective = m.stage ? `${m.stage[0]} (${m.stage[1]}/${m.stage[2]})` : copy[m.objective];
    set(nodes.objective, "objective", objective, text);
    set(nodes.hostiles, "hostiles", m.hostiles, text);
    set(nodes.structuresBox, "hasStructures", Boolean(m.structures), show);
    if (m.structures) {
      const [up, all, may] = m.structures;
      set(nodes.structures, "structures", `${up}/${all} (${copy.may_lose} ${may})`, text);
    }
    set(nodes.raceBox, "hasRace", Boolean(m.race), show);
    if (m.race) {
      const [gate, of, place] = m.race;
      set(nodes.gate, "gate", `${gate}/${of}`, text);
      set(nodes.place, "place", place, text);
    }
    set(nodes.time, "time", m.seconds, text);
    set(nodes.leftBox, "hasLeft", m.time_left !== null, show);
    if (m.time_left !== null) set(nodes.left, "left", m.time_left, text);
    set(nodes.plus, "hasPlus", Boolean(m.plus), show);
    if (m.plus) {
      const [rule, value, now] = m.plus;
      set(nodes.plus, "plus", `${copy.plus_label} ${plusText(copy, rule, value)} · ${now}`, text);
    }
  }

  return {
    show(v) {
      nodes.hud.hidden = !v;
    },
    update(h) {
      set(nodes.speed, "speed", h.speed, text);
      set(nodes.alt, "alt", h.alt, text);
      set(nodes.kills, "kills", h.kills, text);
      set(nodes.glide, "glide", h.glide, show);
      set(nodes.burning, "burning", h.burning, show);
      set(nodes.ap, "ap", h.ap, text);
      set(nodes.apBar, "apBar", h.ap_pct, pct);
      set(nodes.enBar, "enBar", h.en_pct, pct);
      set(nodes.enBar, "enLocked", h.en_locked, cls("locked"));
      set(nodes.enOut, "enOut", h.en_locked, show);
      set(nodes.qb, "qb", !h.qb_ready, cls("off"));
      set(nodes.stagger, "stagger", h.stagger_pct, pct);
      set(nodes.stagger, "staggerFull", h.staggered, cls("full"));
      set(nodes.staggered, "staggered", h.staggered, show);
      h.weapons.forEach((w, i) => {
        const n = nodes.weapons[i];
        set(n.name, `wn${i}`, w.part, text);
        set(n.ammo, `wa${i}`, w.empty ? copy.ammo_empty : w.ammo, text);
        set(n.bar, `wr${i}`, w.ready_pct, pct);
        set(n.root, `we${i}`, w.empty, cls("empty"));
      });
      marks(h.marks);
      frames(h.frames);
      mission(h.mission);
      const w = h.waypoint;
      set(nodes.waypoint, "hasWaypoint", Boolean(w), show);
      if (w) {
        nodes.waypoint.style.transform = `translate(${w.x}px, ${w.y}px)`;
        set(nodes.wpLabel, "wpLabel", copy[w.label], text);
        set(nodes.wpDist, "wpDist", `${w.dist}${copy.metres}`, text);
        set(nodes.waypoint, "wpEdge", w.edge, cls("edge"));
      }
    },
  };
}
