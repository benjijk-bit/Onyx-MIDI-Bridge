// One-time (re-run after dependency bumps) copy of the pre-built ESM files this
// static, bundler-less frontend needs from node_modules into src/vendor/.
// tauri.conf.json's frontendDist is "../src", so only files under src/ ship
// in the app — node_modules itself is never packaged.
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const files = [
  ["node_modules/@tauri-apps/api/core.js", "src/vendor/api/core.js"],
  ["node_modules/@tauri-apps/api/event.js", "src/vendor/api/event.js"],
  [
    "node_modules/@tauri-apps/api/external/tslib/tslib.es6.js",
    "src/vendor/api/external/tslib/tslib.es6.js",
  ],
  [
    "node_modules/@tauri-apps/plugin-dialog/dist-js/index.js",
    "src/vendor/plugin-dialog/index.js",
  ],
];

for (const [from, to] of files) {
  const dest = join(root, to);
  mkdirSync(dirname(dest), { recursive: true });
  copyFileSync(join(root, from), dest);
  console.log(`vendored ${from} -> ${to}`);
}
