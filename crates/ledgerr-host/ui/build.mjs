import * as esbuild from "esbuild";
import { readFileSync, copyFileSync, mkdirSync } from "fs";

const isWatch = process.argv.includes("--watch");
const start = Date.now();

// Self-hosted fonts (CSP is script-src/default-src 'self' -- no CDN fonts,
// same reason cytoscape/dagre got vendored instead of loaded from unpkg.com;
// this is also a local-first app, no runtime dependency on fonts.googleapis.com).
const FONTS = [
  ["@fontsource/space-grotesk", "space-grotesk-latin-400-normal.woff2"],
  ["@fontsource/space-grotesk", "space-grotesk-latin-500-normal.woff2"],
  ["@fontsource/space-grotesk", "space-grotesk-latin-600-normal.woff2"],
  ["@fontsource/space-grotesk", "space-grotesk-latin-700-normal.woff2"],
  ["@fontsource/public-sans", "public-sans-latin-400-normal.woff2"],
  ["@fontsource/public-sans", "public-sans-latin-600-normal.woff2"],
  ["@fontsource/jetbrains-mono", "jetbrains-mono-latin-400-normal.woff2"],
  ["@fontsource/jetbrains-mono", "jetbrains-mono-latin-500-normal.woff2"],
];
mkdirSync("fonts", { recursive: true });
for (const [pkg, file] of FONTS) {
  copyFileSync(`node_modules/${pkg}/files/${file}`, `fonts/${file}`);
}
console.log(`[ui] copied ${FONTS.length} font files`);

const ctx = await esbuild.context({
  entryPoints: ["src/main.ts"],
  bundle: true,
  format: "esm",
  outfile: "main.js",
  minify: false,
  sourcemap: "inline",
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
