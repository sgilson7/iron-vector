// The garage screen. Everything it shows comes from the core's garage()
// view; a click or a hover is passed to the core, and the screen is drawn
// again from what comes back.

const el = (id) => document.getElementById(id);

function item(tag, cls, text) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined) node.textContent = text;
  return node;
}

// Where a locked part is won, in words from the copy file.
function lockText(copy, part) {
  if (!part || !part.locked || !part.hint) return "";
  const [mission, plus] = part.hint;
  return (plus ? copy.unlock_plus : copy.unlock_hint).replace("{mission}", mission);
}

function sign(better) {
  if (better > 0) return "up";
  if (better < 0) return "down";
  return "";
}

export function makeGarage(game, copy, onSave) {
  function drawDetail(v) {
    el("detail-name").textContent = v.detail.name;
    el("detail-maker").textContent = v.detail.maker;
    el("detail-blurb").textContent = v.detail.blurb;
    el("detail-kind").textContent = v.detail.kind ? copy[v.detail.kind] : "";
    el("detail-lock").textContent = lockText(copy, v.parts.find((p) => p.hovered));
    const rows = el("detail-rows");
    rows.replaceChildren();
    for (const [key, value] of v.detail.rows) {
      const tr = rows.insertRow();
      tr.insertCell().textContent = copy[key];
      const td = tr.insertCell();
      td.className = "num";
      td.textContent = value;
    }
  }

  function drawStats(v) {
    const table = el("stats");
    table.replaceChildren();
    for (const r of v.stats) {
      const tr = table.insertRow();
      tr.insertCell().textContent = copy[r.key];
      const now = tr.insertCell();
      now.className = "num";
      now.textContent = r.value;
      const next = tr.insertCell();
      next.className = `num next ${sign(r.better)}`;
      next.textContent = r.next === r.value ? "" : `→ ${r.next}`;
    }
    const warnings = el("warnings");
    warnings.replaceChildren(...v.warnings.map((k) => item("li", "", copy[k])));
  }

  function drawLists(v) {
    el("slots").replaceChildren(
      ...v.slots.map((s, i) => {
        const li = item("li", s.selected ? "selected" : "");
        li.append(item("span", "k", copy[s.key]), item("span", "v", s.part));
        li.addEventListener("click", () => {
          game.garage_select(i);
          draw(true);
        });
        return li;
      }),
    );
    el("parts").replaceChildren(
      ...v.parts.map((p, i) => {
        const li = item("li", [p.equipped ? "equipped" : "", p.locked ? "locked" : ""].join(" "));
        li.title = lockText(copy, p);
        const name = item("span", "v", p.name);
        name.dataset.fitted = copy.equipped;
        li.append(item("span", "k", p.maker), name);
        li.addEventListener("mouseenter", () => {
          game.garage_hover(i);
          draw(false);
        });
        li.addEventListener("mouseleave", () => {
          game.garage_hover(-1);
          draw(false);
        });
        li.addEventListener("click", () => {
          game.garage_equip(i);
          onSave(game.saved());
          draw(true);
        });
        return li;
      }),
    );
    el("paints").replaceChildren(
      ...v.paints.map((p, i) => {
        const b = item("button", p.selected ? "paint selected" : "paint");
        b.type = "button";
        b.title = p.name;
        for (const c of p.swatch) {
          const s = item("span");
          s.style.background = c;
          b.append(s);
        }
        b.append(item("em", "", p.name));
        b.addEventListener("click", () => {
          game.garage_paint(i);
          onSave(game.saved());
          draw(true);
        });
        return b;
      }),
    );
  }

  // `lists` redraws the slot, part and paint lists too; a hover redraws only
  // the card and the comparison, so the row under the pointer stays put.
  function draw(lists) {
    const v = JSON.parse(game.garage());
    if (lists) drawLists(v);
    drawDetail(v);
    drawStats(v);
  }

  return { draw };
}
