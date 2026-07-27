import { defineConfig } from "tsup";

// Dual ESM/CJS output with type declarations. ESM is the primary format for
// tree-shaking; CJS keeps older consumers working.
//
// Two entry points: the root barrel (client + models) and the API barrel
// (convenience re-export of all tag modules). Consumers can also import
// individual tag files directly (e.g. "sdk/api/Pets") for maximum tree-shaking.
export default defineConfig({
  entry: ["src/index.ts", "src/api/index.ts"],
  format: ["esm", "cjs"],
  dts: true,
  sourcemap: true,
  clean: true,
  treeshake: true,
});
