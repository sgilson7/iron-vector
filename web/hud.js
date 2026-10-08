// The heads-up display: it writes what the core's hud() reports into the
// page, touching an element only when its value changed.

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

export function makeHud(copy) {
  const set = setter();
  const nodes = {
    hud: el("hud"),
    speed: el("speed"),
    alt: el("alt"),
    kills: el("kills"),
    glide: el("glide"),
    ap: el("ap"),
    apBar: el("ap-bar"),
    enBar: el("en-bar"),
    enOut: el("en-out"),
    qb: el("qb"),
    marks: el("marks"),
    waypoint: el("waypoint"),
    wpDist: document.querySelector("#waypoint .wp-dist"),
    objective: el("objective"),
    objectiveBox: el("objective-box"),
    timer: el("timer"),
    boss: el("boss"),
    bossName: el("boss-name"),
    bossIntent: el("boss-intent"),
    bossAp: el("boss-ap"),
    bossStagger: el("boss-stagger"),
    stagger: el("stagger-bar"),
    staggered: el("staggered"),
    weapons: ["w0", "w1", "w2"].map((id) => {
      const root = el(id);
      return {
        root,
        name: root.querySelector(".wname"),
        ammo: root.querySelector(".ammo"),
        bar: root.querySelector(".bar"),
      };
    }),
  };
  const markPool = [];

  function marks(list) {
    while (markPool.length < list.length) {
      const m = document.createElement("div");
      m.className = "mark";
      m.append(document.createElement("span"));
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

  return {
    show(v) {
      nodes.hud.hidden = !v;
    },
    update(h) {
      set(nodes.speed, "speed", h.speed, text);
      set(nodes.alt, "alt", h.alt, text);
      set(nodes.kills, "kills", h.kills, text);
      set(nodes.glide, "glide", h.glide, show);
      set(nodes.ap, "ap", h.ap, text);
      set(nodes.apBar, "apBar", h.ap_pct, pct);
      set(nodes.enBar, "enBar", h.en_pct, pct);
      set(nodes.enBar, "enLocked", h.en_locked, cls("locked"));
      set(nodes.enOut, "enOut", h.en_locked, show);
      set(nodes.qb, "qb", !h.qb_ready, cls("off"));
      h.weapons.forEach((w, i) => {
        const n = nodes.weapons[i];
        set(n.name, `wn${i}`, w.part, text);
        set(n.ammo, `wa${i}`, w.empty ? copy.ammo_empty : w.ammo, text);
        set(n.bar, `wr${i}`, w.ready_pct, pct);
        set(n.root, `we${i}`, w.empty, cls("empty"));
      });
      marks(h.marks);
      set(nodes.stagger, "stagger", h.stagger_pct, pct);
      set(nodes.stagger, "staggerFull", h.staggered, cls("full"));
      set(nodes.staggered, "staggered", h.staggered, show);
      set(nodes.objectiveBox, "hasObjective", Boolean(h.objective), show);
      set(nodes.objective, "objective", h.objective ? copy[h.objective] : "", text);
      set(nodes.timer, "timer", h.seconds, text);
      const w = h.waypoint;
      set(nodes.waypoint, "hasWaypoint", Boolean(w), show);
      if (w) {
        nodes.waypoint.style.transform = `translate(${w.x}px, ${w.y}px)`;
        set(nodes.wpDist, "wpDist", `${w.dist}${copy.metres}`, text);
        set(nodes.waypoint, "wpEdge", w.edge, cls("edge"));
      }
      const b = h.boss;
      set(nodes.boss, "hasBoss", Boolean(b), show);
      if (b) {
        set(nodes.bossName, "bossName", b.name, text);
        set(nodes.bossIntent, "bossIntent", b.intent ? `${copy.ai_caption}: ${copy[b.intent]}` : "", text);
        set(nodes.bossAp, "bossAp", b.ap_pct, pct);
        set(nodes.bossStagger, "bossStagger", b.stagger_pct, pct);
        set(nodes.bossStagger, "bossStaggered", b.staggered, cls("full"));
      }
    },
  };
}
