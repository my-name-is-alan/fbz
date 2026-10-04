import { mkdir, cp, readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { build } from "esbuild";
const root = fileURLToPath(new URL("../", import.meta.url));
const output = path.join(root, "public/webplayer");
const stamp = createHash("sha256")
  .update(await readFile(new URL(import.meta.url)))
  .update(await readFile(path.join(root, "pnpm-lock.yaml")))
  .update(await readFile(path.join(root, "vendor/webplayer/src/player.js")))
  .digest("hex");
try {
  if ((await readFile(path.join(output, "build-id"), "utf8")) === stamp) process.exit(0);
} catch {}
await mkdir(path.join(output, "vendor"), { recursive: true });
await cp(path.join(root, "vendor/webplayer/src"), path.join(output, "src"), { recursive: true });
await cp(path.join(root, "vendor/webplayer/LICENSE"), path.join(output, "LICENSE"));
// Resolve software decoder assets relative to its module, independently of the SPA route.
const playerPath = path.join(output, "src/player.js");
let player = await readFile(playerPath, "utf8");
player = player.replace(
  "new SoftwareAudioDecoder({ log:",
  "new SoftwareAudioDecoder({ vendor: new URL('../vendor/', import.meta.url).href, log:",
);
player = player.replace(
  "dispose() { return this._teardown(); }",
  "async dispose() { await this._teardown(); this._worker?.terminate(); this._worker = null; await this.audioDecoder?.destroy(); this.audioDecoder = null; }",
);
await writeFile(playerPath, player);
const nodeModules = path.join(root, "node_modules");
await build({
  entryPoints: {
    jassub: path.join(nodeModules, "jassub/dist/jassub.js"),
    "jassub-worker": path.join(nodeModules, "jassub/dist/worker/worker.js"),
    ffmpeg: path.join(nodeModules, "@ffmpeg/ffmpeg/dist/esm/index.js"),
    "ffmpeg-worker": path.join(nodeModules, "@ffmpeg/ffmpeg/dist/esm/worker.js"),
  },
  outdir: path.join(output, "vendor"),
  bundle: true,
  format: "esm",
  target: "es2022",
  external: ["*.wasm"],
  logLevel: "warning",
});
for (const file of ["wasm/jassub-worker.wasm", "wasm/jassub-worker-modern.wasm", "default.woff2"])
  await cp(
    path.join(nodeModules, "jassub/dist", file),
    path.join(output, "vendor", path.basename(file)),
  );
for (const file of ["libpgs.js", "libpgs.worker.js"])
  await cp(path.join(nodeModules, "libpgs/dist", file), path.join(output, "vendor", file));
for (const file of ["ffmpeg-core.js", "ffmpeg-core.wasm"])
  await cp(
    path.join(nodeModules, "@ffmpeg/core/dist/esm", file),
    path.join(output, "vendor", file),
  );
for (const [pkg, name] of [
  ["@ffmpeg/core", "ffmpeg-core"],
  ["@ffmpeg/ffmpeg", "ffmpeg-wrapper"],
  ["jassub", "jassub"],
  ["libpgs", "libpgs"],
]) {
  try {
    await cp(path.join(nodeModules, pkg, "LICENSE"), path.join(output, `${name}-LICENSE`));
  } catch {}
}
await cp(path.join(root, "vendor/webplayer/NOTICE.md"), path.join(output, "NOTICE.md"));
await writeFile(path.join(output, "build-id"), stamp);
console.log("FBZ webplayer assets ready.");

await cp(path.join(root, "vendor/webplayer/licenses"), path.join(output, "licenses"), {
  recursive: true,
});
