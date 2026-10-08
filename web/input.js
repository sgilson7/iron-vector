// Keyboard, mouse buttons, mouse movement and pointer capture, turned into
// the action bits the core defines. data/controls.json names the action for
// each key; the core's numbers() gives each action its bit. Nothing here
// decides what an action does.

export function makeInput(canvas, controls, actions, onEngage) {
  const state = { held: 0, dx: 0, dy: 0, engaged: false, fallback: false };
  const bitOf = (code) => actions[controls[code]] || 0;

  const locked = () => document.pointerLockElement === canvas;
  const engaged = () => locked() || state.fallback;

  function release() {
    state.held = 0;
    state.fallback = false;
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
    if (e.code === "Escape") return release();
    const bit = bitOf(e.code);
    if (!bit || !engaged()) return;
    state.held |= bit;
    e.preventDefault();
  });
  window.addEventListener("keyup", (e) => {
    state.held &= ~bitOf(e.code);
  });
  canvas.addEventListener("mousedown", (e) => {
    if (!engaged()) return;
    state.held |= bitOf(`Mouse${e.button}`);
    e.preventDefault();
  });
  window.addEventListener("mouseup", (e) => {
    state.held &= ~bitOf(`Mouse${e.button}`);
  });
  canvas.addEventListener("contextmenu", (e) => e.preventDefault());
  window.addEventListener("mousemove", (e) => {
    if (!engaged()) return;
    state.dx += e.movementX;
    state.dy += e.movementY;
  });
  window.addEventListener("blur", release);

  return {
    engage,
    engaged,
    // The held bits plus focus, and the movement since the last read.
    read() {
      const bits = engaged() ? state.held | actions.focus : state.held;
      const out = { bits, dx: state.dx, dy: state.dy };
      state.dx = 0;
      state.dy = 0;
      return out;
    },
  };
}
