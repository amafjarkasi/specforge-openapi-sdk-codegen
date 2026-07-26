import { defineConfig } from "tsup";

// Dual ESM/CJS output with type declarations. ESM is the primary format for
// tree-shaking; CJS keeps older consumers working.
export default defineConfig({
  entry: ["src/index.ts"],
  format: ["esm", "cjs"],
  dts: true,
  sourcemap: true,
  clean: true,
  treeshake: true,
});
