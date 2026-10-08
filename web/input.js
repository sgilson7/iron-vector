// Keyboard, mouse buttons, mouse movement and pointer capture, turned into
// the action bits the core defines. data/controls.json names the action for
// each key; the core's numbers() gives each action its bit. Nothing here
// decides what an action does.

export function makeInput(canvas, controls, actions, onEngage) {
  // `tapped` keeps a press that is released before the next frame reads it
  const state = { held: 0, tapped: 0, dx: 0, dy: 0, engaged: false, fallback: false, dragging: false };
  const bitOf = (code) => actions[controls[code]] || 0;

  const locked = () => document.pointerLockElement === canvas;
  const engaged = () => locked() || state.fallback;

  function release() {
    state.held = 0;
    state.tapped = 0;
    state.fallback = false;
  }

  function letGo() {
    release();
    if (document.pointerLockElement && document.exitPointerLock) document.exitPointerLock();
  }

  function engage() {
    // Raw mouse input where the browser offers it; plain pointer lock where
    // it does not; and where neither works (an embedded frame, a headless
    // browser), the game still takes the keyboard and the mouse moves.
    const fallBack = () => {
      state.fallback = true;
      onEngage();
    };
    if (!canvas.requestPointerLock) return fallBack();
    const tryPlain = () => {
      const plain = canvas.requestPointerLock();
      if (plain && plain.catch) plain.catch(fallBack);
    };
    const raw = canvas.requestPointerLock({ unadjustedMovement: true });
    if (raw && raw.catch) raw.catch(tryPlain);
    onEngage();
  }

  document.addEventListener("pointerlockchange", () => {
    if (!locked()) release();
  });
  document.addEventListener("pointerlockerror", () => {
    state.fallback = true;
  });

  window.addEventListener("keydown", (e) => {
    // Esc always pauses: it lets go of a real pointer lock as well as the fallback
    if (e.code === "Escape") return letGo();
    const bit = bitOf(e.code);
    if (!bit || !engaged()) return;
    state.held |= bit;
    state.tapped |= bit;
    e.preventDefault();
  });
  window.addEventListener("keyup", (e) => {
    state.held &= ~bitOf(e.code);
  });
  canvas.addEventListener("mousedown", (e) => {
    // outside a sortie, dragging the view turns the garage camera
    state.dragging = !engaged();
    if (!engaged()) return;
    state.held |= bitOf(`Mouse${e.button}`);
    state.tapped |= bitOf(`Mouse${e.button}`);
    e.preventDefault();
  });
  window.addEventListener("mouseup", (e) => {
    state.dragging = false;
    state.held &= ~bitOf(`Mouse${e.button}`);
  });
  canvas.addEventListener("contextmenu", (e) => e.preventDefault());
  window.addEventListener("mousemove", (e) => {
    if (!engaged() && !state.dragging) return;
    state.dx += e.movementX;
    state.dy += e.movementY;
  });
  window.addEventListener("blur", release);

  return {
    engage,
    engaged,
    // Lets go of the pointer, as the debrief and the garage need.
    release: letGo,
    // The held bits plus focus, and the movement since the last read.
    read() {
      const drag = state.dragging ? actions.drag : 0;
      const down = state.held | state.tapped;
      const bits = (engaged() ? down | actions.focus : down) | drag;
      const out = { bits, dx: state.dx, dy: state.dy };
      state.tapped = 0;
      state.dx = 0;
      state.dy = 0;
      return out;
    },
  };
}
