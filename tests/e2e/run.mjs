// End-to-end test of the built plugin against headless SolidCraft (solidcraft-cli run).
// Usage (from the repo root, after scripts/build.ps1):  node tests/e2e/run.mjs
// Each scenario: a 20 x 10 x 20 block standing on XY, a sketch on the XZ wall plane with a circle
// of radius hole_r, the plugin's teardrop, a 10 mm cut through the wall, then volume checks,
// also after changing hole_r (the teardrop must follow without re-running the plugin).

import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CLI = process.env.SOLIDCRAFT_CLI ?? "C:/Program Files/SolidCraft/solidcraft-cli.exe";
const PKG = "dist/solidcraft-3d-printing-holes";
const work = mkdtempSync(join(tmpdir(), "solidcraft-3d-printing-holes-e2e-"));

function run(steps, { allowErrors = false } = {}) {
  const script = join(work, "script.json");
  const out = join(work, "out.json");
  writeFileSync(script, JSON.stringify(steps, null, 1));
  const args = ["run", script, "--results", "--json-out", out];
  if (allowErrors) args.push("--allow-errors");
  try {
    execFileSync(CLI, args, { stdio: ["ignore", "pipe", "pipe"] });
  } catch (e) {
    return { error: (e.stderr?.toString() || e.stdout?.toString() || String(e)).trim() };
  }
  return { results: JSON.parse(readFileSync(out, "utf8")) };
}

const approval = (() => {
  const r = run([{ command: "plugin.inspect", params: { path: PKG } }]);
  if (r.error) throw new Error("plugin.inspect failed: " + r.error);
  const ins = r.results[0];
  if (!ins.within_host_limits) throw new Error("outside host limits: " + JSON.stringify(ins));
  return ins.approval;
})();

const PI = Math.PI;
const BLOCK = 20 * 10 * 20;
const DEPTH = 10;
const CZ = 10; // world Z of the hole centre

/** Area of the teardrop: phi from up to each tangent point, d = tip distance. */
function area(r, { phiDeg = 45, d } = {}) {
  const phi = (phiDeg * PI) / 180;
  d ??= r / Math.cos(phi);
  return r * r * (PI - phi) + r * d * Math.sin(phi);
}

function base({ select = true } = {}) {
  return [
    { command: "parameters.add", params: { name: "hole_r", expression: "4 mm" } },
    { command: "solid.box", params: { length: 20, width: 10, height: 20, corner: [-10, 0, 0], name: "Wall" } },
    { command: "sketch.create", params: { plane: "XZ" } },
    { command: "sketch.circle.center", params: { center: [0, -CZ], radius: "hole_r" } },
    { command: "sketch.dimension", params: { entities: ["c1.center", "origin"], type: "horizontal", value: 0 } },
    { command: "sketch.dimension", params: { entities: ["c1.center", "origin"], type: "vertical", value: CZ } },
    ...(select ? [{ command: "select.set", params: { items: [{ type: "sketch_curve", sketch: 2, id: "c1" }] } }] : []),
  ];
}

const plugin = (params) => ({
  command: "plugin.run",
  params: { path: PKG, approval, entry: "teardrop", params },
});

const tail = (r2, extrude = {}) => [
  { command: "sketch.inspect", params: {} },
  { command: "sketch.finish", params: {} },
  { command: "solid.extrude", params: { distance: DEPTH, operation: "cut", profiles: "all", ...extrude } },
  { command: "inspect.measure", params: {} },
  { command: "parameters.change", params: { name: "hole_r", expression: `${r2} mm` } },
  { command: "inspect.measure", params: {} },
];

/** Every form field at its manifest default, as the generated dialog submits them. */
const FORM = { apply_to_all: false, up: "world +Z (print up)", up_angle: 90, warn_horizontal: true, tip_extra_d: 2.5, tip_height: "" };

/** A pointed tip at k * r: the flanks touch the circle at acos(1/k) from up. */
const low = (k) => (r) => area(r, { phiDeg: (Math.acos(1 / k) * 180) / PI, d: k * r });
const S30 = Math.sin(PI / 6), C30 = Math.cos(PI / 6);

const scenarios = [
  {
    name: "default: tip 2.5 % of the diameter above the circle, from selection, tip direction = world +Z",
    steps: [...base(), plugin({}), ...tail(6)],
    tipExpr: "hole_r + 2 * hole_r * 2.5 / 100",
    area: low(1.05),
    tipZ: (r) => CZ + 1.05 * r,
  },
  {
    name: "tip_extra_d 6 % (= 12 % of r)",
    steps: [...base(), plugin({ tip_extra_d: 6 }), ...tail(6)],
    area: low(1.12),
    tipZ: (r) => CZ + 1.12 * r,
  },
  {
    name: "older tip_extra 12 % of r (scripts)",
    steps: [...base(), plugin({ tip_extra: 12 }), ...tail(6)],
    area: low(1.12),
    tipZ: (r) => CZ + 1.12 * r,
  },
  {
    name: "explicit circle ids, overhang 50° (scripts)",
    steps: [...base({ select: false }), plugin({ circles: ["c1"], overhang_angle: 50 }), ...tail(6)],
    area: (r) => area(r, { phiDeg: 50 }),
    tipZ: (r) => CZ + r / Math.cos((50 * PI) / 180),
  },
  {
    name: "tip_height override 1.2*r",
    steps: [...base(), plugin({ tip_height: "1.2*r" }), ...tail(6)],
    area: low(1.2),
    tipZ: (r) => CZ + 1.2 * r,
  },
  {
    name: "tip_height override with d: 0.6*d",
    steps: [...base(), plugin({ tip_height: "0.6*d" }), ...tail(6)],
    area: low(1.2),
    tipZ: (r) => CZ + 1.2 * r,
  },
  {
    name: "oblique up: 300° in sketch = 30° off world +Z towards +X",
    steps: [...base(), plugin({ up: "300" }), ...tail(6)],
    area: low(1.05),
    tipZ: (r) => CZ + 1.05 * r * C30,
    tipX: (r) => 1.05 * r * S30,
  },
  {
    name: "classic 45° (overhang_angle)",
    steps: [...base(), plugin({ overhang_angle: 45 }), ...tail(6)],
    area: (r) => area(r),
    tipZ: (r) => CZ + r * Math.SQRT2,
  },
  {
    name: "form defaults (what the GUI dialog sends)",
    steps: [...base(), plugin(FORM), ...tail(6)],
    area: low(1.05),
    tipZ: (r) => CZ + 1.05 * r,
  },
  {
    name: "form: Apply to all circles in sketch, nothing selected",
    steps: [...base({ select: false }), plugin({ ...FORM, apply_to_all: true }), ...tail(6)],
    area: low(1.05),
    tipZ: (r) => CZ + 1.05 * r,
  },
  {
    // The GUI selects a circle of another sketch than the one being edited as an
    // external_sketch_curve: the plugin switches to the circle's sketch and back again.
    name: "circle in another sketch than the open one (external_sketch_curve)",
    steps: [
      ...base({ select: false }),
      { command: "sketch.finish", params: {} },
      { command: "sketch.create", params: { plane: "XZ" } },
      { command: "select.set", params: { items: [{ type: "external_sketch_curve", sketch: 2, id: "c1" }] } },
      plugin(FORM),
      { command: "document.inspect", params: { timeline: false, bodies: false, measure: false } },
      { command: "sketch.finish", params: {} },
      { command: "sketch.edit", params: { sketch: 2 } },
      ...tail(6, { sketch: 2 }),
    ],
    activeAfter: 3,
    area: low(1.05),
    tipZ: (r) => CZ + 1.05 * r,
  },
  {
    name: "form: Tip direction = angle in sketch, 300°",
    steps: [...base(), plugin({ ...FORM, up: "angle in sketch", up_angle: 300 }), ...tail(6)],
    area: low(1.05),
    tipZ: (r) => CZ + 1.05 * r * C30,
    tipX: (r) => 1.05 * r * S30,
  },
];

let failed = 0;
const check = (label, got, want, tol = 1e-3) => {
  const ok = Math.abs(got - want) <= tol * Math.max(1, Math.abs(want));
  if (!ok) failed++;
  console.log(`  ${ok ? "ok  " : "FAIL"} ${label}: ${got.toFixed(4)} (expected ${want.toFixed(4)})`);
};

for (const s of scenarios) {
  console.log(`\n# ${s.name}`);
  const r = run(s.steps);
  if (r.error) {
    failed++;
    console.log("  FAIL run: " + r.error.slice(0, 1500));
    continue;
  }
  const res = r.results;
  const n = s.steps.length;
  const pluginResult = res[s.steps.findIndex((x) => x.command === "plugin.run")];
  const conv = pluginResult.result.converted[0];
  if (s.activeAfter !== undefined) {
    const doc = res[s.steps.findIndex((x) => x.command === "plugin.run") + 1];
    const ok = doc.active_sketch === s.activeAfter;
    if (!ok) failed++;
    console.log(`  ${ok ? "ok  " : "FAIL"} sketch being edited afterwards: ${doc.active_sketch} (expected ${s.activeAfter})`);
  }
  if (conv.warnings.length) console.log("  warnings:", conv.warnings);
  if (s.tipExpr !== undefined) {
    const ok = conv.tip_expression === s.tipExpr;
    if (!ok) failed++;
    console.log(`  ${ok ? "ok  " : "FAIL"} tip expression: ${conv.tip_expression} (expected ${s.tipExpr})`);
  }
  const sk = res[n - 6];
  if (sk.dof !== 0) failed++;
  console.log(`  ${sk.dof === 0 ? "ok  " : "FAIL"} dof ${sk.dof}`);
  const upEnd = sk.points.find((p) => p.id === `${conv.up_line}.end`);
  check("up-line end world Z", upEnd.world[2], s.tipZ(4));
  check("up-line end world X", upEnd.world[0], s.tipX ? s.tipX(4) : 0);
  check("volume r=4", res[n - 3].total.volume_mm3, BLOCK - s.area(4) * DEPTH);
  check("volume r=6 (parametric)", res[n - 1].total.volume_mm3, BLOCK - s.area(6) * DEPTH);
}

// A horizontal sketch with tip direction = world +Z: no refusal, the tip goes to sketch +Y (here world +Y),
// with a warning unless warn_horizontal is off.
console.log("\n# horizontal sketch");
for (const [name, extra, wantWarning] of [
  ["XY sketch, tip direction = world +Z: warns", {}, true],
  ["XY sketch, tip direction = world +Z, warn_horizontal off: silent", { warn_horizontal: false }, false],
]) {
  const steps = [
    { command: "sketch.create", params: { plane: "XY" } },
    { command: "sketch.circle.center", params: { center: [0, 0], radius: 3 } },
    { command: "sketch.dimension", params: { entities: ["c1"], type: "radius", value: 3 } },
    plugin({ circles: ["c1"], ...extra }), // the centre snaps to the origin
    { command: "sketch.inspect", params: {} },
  ];
  const r = run(steps);
  if (r.error) {
    failed++;
    console.log(`  FAIL ${name}: ${r.error.split("\n")[0].slice(0, 300)}`);
    continue;
  }
  const conv = r.results[3].result.converted[0];
  const warned = conv.warnings.some((w) => w.includes("horizontal"));
  if (warned !== wantWarning) failed++;
  console.log(`  ${warned === wantWarning ? "ok  " : "FAIL"} ${name}: warnings ${JSON.stringify(conv.warnings)}`);
  const sk = r.results[4];
  if (sk.dof !== 0) failed++;
  console.log(`  ${sk.dof === 0 ? "ok  " : "FAIL"} dof ${sk.dof}`);
  const upEnd = sk.points.find((p) => p.id === `${conv.up_line}.end`);
  check("up-line end world Y", upEnd.world[1], 1.05 * 3);
}

// Refusals: a tip below the circle.
console.log("\n# refusals");
for (const [name, steps, needle] of [
  ["tip_height below the radius", [...base(), plugin({ tip_height: "0.9*r" })], "tip must lie above"],
  ["tip_extra_d 0", [...base(), plugin({ tip_extra_d: 0 })], "tip_extra_d"],
  ["form, nothing selected", [...base({ select: false }), plugin(FORM)], "nothing is selected"],
]) {
  const r = run(steps);
  const ok = !!r.error && r.error.includes(needle);
  if (!ok) failed++;
  console.log(`  ${ok ? "ok  " : "FAIL"} ${name}: ${(r.error ?? "no error").split("\n")[0].slice(0, 300)}`);
}

console.log(failed ? `\n${failed} check(s) FAILED` : "\nall checks passed");
process.exit(failed ? 1 : 0);
