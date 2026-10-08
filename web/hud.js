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
    },
  };
}
