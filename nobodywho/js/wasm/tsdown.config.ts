import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/index.ts"],
  platform: "neutral",
  target: "baseline-widely-available",
  dts: true,
  sourcemap: true,
  copy: ["src/uniffi.wasm"],
});
