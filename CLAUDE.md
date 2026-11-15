# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A SolidCraft CAD plugin (`solidcraft-3d-printing-holes`), written in Rust and compiled to a WebAssembly component. Its one entry so far, `teardrop`, turns selected sketch circles into constrained teardrop profiles (by default a low tip 5 % of the radius above the circle, `tip_extra`; a classic 45° overhang via `overhang_angle` or `tip_height`) so that horizontal holes print without supports. The README (English) is the user-facing doc: it covers fields, formulas and install steps. Its first picture (`docs/teardrop.png`, not SVG) is the card image on the plugin list at tharje.github.io/solidcraft-plugins.

## Commands

Windows only. `rust-toolchain.toml` pins `stable-x86_64-pc-windows-gnu` plus the `wasm32-unknown-unknown` target, so no MSVC build tools are needed. `wasm-tools` must be on `PATH`.

```powershell
.\scripts\build.ps1                 # wasm build -> dist/solidcraft-3d-printing-holes/{plugin.json,plugin.wasm} + dist/solidcraft-3d-printing-holes.zip
cargo test                          # native unit tests (geometry + param parsing)
cargo test flat_top_at_r            # run a single test by name
node tests/e2e/run.mjs              # e2e against headless solidcraft-cli; run build.ps1 first
```

The e2e test uses `C:/Program Files/SolidCraft/solidcraft-cli.exe` by default; set `SOLIDCRAFT_CLI` to point it at another CLI. It runs JSON command scripts through `solidcraft-cli run`. For each scenario it checks that the sketch ends at dof 0, where the up-line tip lands in world coordinates, and that the volume after a cut matches the analytic teardrop area. It then changes the `hole_r` parameter and checks the volume again, which shows that the shape follows the parameter without re-running the plugin.

## Architecture

- **The host abstraction is the key seam.** `host::Host` (`src/host.rs`) has a single method, `run(command, json) -> json`. In the wasm build it is implemented by `WasmHost` in `src/lib.rs`, which calls `craft:plugin/commands.run`. All of that code sits behind `#[cfg(target_arch = "wasm32")]`, and `wit-bindgen` is a wasm-only dependency. The crate is `cdylib` + `rlib`, so `cargo test` builds natively and never touches WIT.
- **`src/geometry.rs`** holds pure 2D maths: tangent points, tip, flat cap, area, and `auto_up`, which projects world +Z into the sketch plane (`None` on horizontal planes; the caller then uses sketch +Y and warns unless `warn_horizontal` is off, because CAD orientation is not always print orientation). It makes no host calls, so it is easy to unit test.
- **`src/teardrop.rs`** handles input parsing and drives the app. For each circle it inspects the sketch, computes the geometry, and draws a construction up-line, a center-point arc and the flanks with `sketch.line`. It adds tangency, deletes the circle, and moves the circle's radius/diameter *expression* onto the arc. Finally it constrains the up direction and the tip (an angle dimension, or a length expression), then checks `sketch.solve`.
- **Parametric intent matters.** User expressions such as `tip_height`/`flat_top` use `r`, which `substitute_r` replaces with the circle's own radius expression, so every dimension stays tied to the user's parameter. Keep new features expression-based in the same way rather than baking in evaluated numbers.
- **`plugin/plugin.json`** is the manifest: entries, the input JSON schema, and a **`commands` allowlist**. Any new app command called through `host.run` must be added to that list, or it fails at runtime. The input schema and `parse_options` must stay in sync. **SolidCraft generates the GUI form from `input_schema`**, so keep form fields to plain `number`/`boolean`/`string`/`enum` (no union types or `anyOf`). Script-only inputs (`circles`, `sketch`) are deliberately left out of the schema; the schema has no `additionalProperties: false`, so the CLI can still pass them. `slots[].name` is the **label** of the menu item/ribbon button (it appears as *Plugins › 3D Printing Holes › Teardrop…* and *Model › Plugins › Teardrop*); it is not the name of a target menu. Headless: `plugin.inspect` (to get the approval hash) followed by `plugin.run`.
- **`wit/plugin.wit`** is a pinned copy of SolidCraft's `craft:plugin@0.1.0` WIT. The host links no WASI, so the component must import only `craft:plugin` interfaces. Don't add dependencies that pull in WASI or std I/O. `build.ps1` prints the component's imports so you can check this.
- **Build script quirks** (keep them): it uses `$ErrorActionPreference = 'Continue'` with `$LASTEXITCODE` checks, because PowerShell 5.1 turns cargo's stderr into errors. It builds the zip with `tar.exe` and not `Compress-Archive`, because `plugin.install` rejects backslash entry names.

## Releasing

The plugin list (tharje.github.io/solidcraft-plugins) finds the repo through its GitHub topic `solidcraft-plugin` and reads `plugin/plugin.json` from the default branch; its download button is the zip of the latest release. Bump `version` in `Cargo.toml` and `plugin/plugin.json` together, then push a tag `v<version>`: `.github/workflows/release.yml` checks that the tag matches, runs the tests and `build.ps1`, and attaches the zip to the release.

## Adding an entry

Add a match arm in `lib.rs::execute`, add a module that implements `run(host, params)`, and add an entry to `plugin.json` with its schema. Extend the `commands` allowlist with any new app commands, and add an e2e scenario.
