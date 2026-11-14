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
const PKG = "dist/printing-holes";
const work = mkdtempSync(join(tmpdir(), "printing-holes-e2e-"));

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

/** Area of the teardrop: phi from up to each tangent point, d = tip distance, optional flat cap at c. */
function area(r, { phiDeg = 45, d, c } = {}) {
  const phi = (phiDeg * PI) / 180;
  d ??= r / Math.cos(phi);
  let a = r * r * (PI - phi) + r * d * Math.sin(phi);
  if (c !== undefined) {
    const half = (r * Math.sin(phi) * (d - c)) / (d - r * Math.cos(phi));
    a -= half * (d - c);
  }
  return a;
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

const tail = (r2) => [
  { command: "sketch.inspect", params: {} },
  { command: "sketch.finish", params: {} },
  { command: "solid.extrude", params: { distance: DEPTH, operation: "cut", profiles: "all" } },
  { command: "inspect.measure", params: {} },
  { command: "parameters.change", params: { name: "hole_r", expression: `${r2} mm` } },
  { command: "inspect.measure", params: {} },
];

const scenarios = [
  {
    name: "45° from selection, up = auto (world +Z)",
    steps: [...base(), plugin({}), ...tail(6)],
    area: (r) => area(r),
    tipZ: (r) => CZ + r * Math.SQRT2,
  },
  {
    name: "explicit circle ids, overhang 50°",
    steps: [...base({ select: false }), plugin({ circles: ["c1"], overhang_angle: 50 }), ...tail(6)],
    area: (r) => area(r, { phiDeg: 50 }),
    tipZ: (r) => CZ + r / Math.cos((50 * PI) / 180),
  },
  {
    name: "tip_height override 1.2*r",
    steps: [...base(), plugin({ tip_height: "1.2*r" }), ...tail(6)],
    area: (r) => area(r, { phiDeg: (Math.acos(1 / 1.2) * 180) / PI, d: 1.2 * r }),
    tipZ: (r) => CZ + 1.2 * r,
  },
  {
    name: "oblique up: 300° in sketch = 30° off world +Z towards +X",
    steps: [...base(), plugin({ up: "300" }), ...tail(6)],
    area: (r) => area(r),
    tipZ: (r) => CZ + r * Math.SQRT2 * Math.cos(PI / 6),
    tipX: (r) => r * Math.SQRT2 * Math.sin(PI / 6),
  },
  {
    name: "flat top at r",
    steps: [...base(), plugin({ flat_top: "r" }), ...tail(6)],
    area: (r) => area(r, { c: r }),
    tipZ: (r) => CZ + r,
  },
  {
    name: "flat top 1.1*r with tip_height 1.5*r",
    steps: [...base(), plugin({ flat_top: "1.1*r", tip_height: "1.5*r" }), ...tail(6)],
    area: (r) => area(r, { phiDeg: (Math.acos(1 / 1.5) * 180) / PI, d: 1.5 * r, c: 1.1 * r }),
    tipZ: (r) => CZ + 1.1 * r,
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
  if (conv.warnings.length) console.log("  warnings:", conv.warnings);
  const sk = res[n - 6];
  if (sk.dof !== 0) failed++;
  console.log(`  ${sk.dof === 0 ? "ok  " : "FAIL"} dof ${sk.dof}`);
  const upEnd = sk.points.find((p) => p.id === `${conv.up_line}.end`);
  check("up-line end world Z", upEnd.world[2], s.tipZ(4));
  check("up-line end world X", upEnd.world[0], s.tipX ? s.tipX(4) : 0);
  check("volume r=4", res[n - 3].total.volume_mm3, BLOCK - s.area(4) * DEPTH);
  check("volume r=6 (parametric)", res[n - 1].total.volume_mm3, BLOCK - s.area(6) * DEPTH);
}

// Refusals: a horizontal sketch with up=auto, and a tip below the circle.
console.log("\n# refusals");
for (const [name, steps, needle] of [
  [
    "XY sketch with up=auto",
    [
      { command: "sketch.create", params: { plane: "XY" } },
      { command: "sketch.circle.center", params: { center: [0, 0], radius: 3 } },
      plugin({ circles: ["c1"] }),
    ],
    "horizontal",
  ],
  ["tip_height below the radius", [...base(), plugin({ tip_height: "0.9*r" })], "tip must lie above"],
]) {
  const r = run(steps);
  const ok = !!r.error && r.error.includes(needle);
  if (!ok) failed++;
  console.log(`  ${ok ? "ok  " : "FAIL"} ${name}: ${(r.error ?? "no error").split("\n")[0].slice(0, 300)}`);
}

console.log(failed ? `\n${failed} check(s) FAILED` : "\nall checks passed");
process.exit(failed ? 1 : 0);
