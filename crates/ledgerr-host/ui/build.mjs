import * as esbuild from "esbuild";
import { readFileSync } from "fs";

const isWatch = process.argv.includes("--watch");
const start = Date.now();

const ctx = await esbuild.context({
  entryPoints: ["src/main.ts"],
  bundle: true,
  format: "esm",
  outfile: "main.js",
  minify: false,
  sourcemap: "inline",
  external: ["cytoscape", "dagre", "cytoscape-dagre"],
  logLevel: "info",
});

if (isWatch) {
  await ctx.watch();
  console.log("[ui] watching for changes...");
} else {
  await ctx.rebuild();
  const size = readFileSync("main.js").length;
  const elapsed = Date.now() - start;
  console.log(`[ui] built main.js in ${elapsed}ms (${(size / 1024).toFixed(1)}kb)`);
  await ctx.dispose();
}
