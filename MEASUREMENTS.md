# Measurements

Stage 4, recorded by the agent. `B2 Require step-by-step justification`.

| What | Command | Result | When |
| --- | --- | --- | --- |
| Toolchain | `rustc --version; wasm-bindgen --version` | rustc 1.95.0, wasm-bindgen 0.2.127 | 2026-10-08 |
| Template before changes | copied from `builder-setting/template` | blank template, placeholder rule | 2026-10-08 |
| Core tests, stage A | `cargo test --workspace` | 70 core unit tests, 11 boundary tests, all pass, under 0.1 s | 2026-10-08 |
| Release wasm size | `scripts/build.sh` | 307 KB | 2026-10-08 |
| Frame rate | headless Chromium, 1280×720, 2 s of `requestAnimationFrame` after flying and firing | 60.2 fps (the display's rate) | 2026-10-08 |
| Full check | `./scripts/check.sh` | BUILDER CHECK PASSED in Chromium, Firefox, WebKit | 2026-10-08 |
