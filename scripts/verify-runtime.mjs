import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const strict = process.argv.includes("--strict");
const runtimeManifestPath = path.join(root, "runtime", "manifest.json");
const modelManifestPath = path.join(root, "models", "manifest.json");

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch (error) {
    throw new Error(`Unable to parse ${path.relative(root, file)}: ${error.message}`);
  }
}

function binaryName(id) {
  return process.platform === "win32" && !id.toLowerCase().endsWith(".exe") ? `${id}.exe` : id;
}

function uniqueIds(items, label) {
  const seen = new Set();
  for (const item of items) {
    if (!item?.id) throw new Error(`${label} contains an item without an id`);
    if (seen.has(item.id)) throw new Error(`${label} contains duplicate id: ${item.id}`);
    seen.add(item.id);
  }
}

const runtimeManifest = readJson(runtimeManifestPath);
const modelManifest = readJson(modelManifestPath);

if (runtimeManifest.schema_version !== 1) throw new Error("Unsupported runtime manifest schema");
if (modelManifest.schema_version !== 1) throw new Error("Unsupported model manifest schema");

uniqueIds(runtimeManifest.binaries ?? [], "runtime manifest");
uniqueIds(modelManifest.models ?? [], "model manifest");

const runtimeDir = path.resolve(process.env.CLE_VIDEOSR_RUNTIME_DIR || path.join(root, "runtime"));
const modelDir = path.resolve(process.env.CLE_VIDEOSR_MODEL_DIR || path.join(runtimeDir, "models"));
const binDir = path.join(runtimeDir, "bin");

const rows = [];
let missingRequired = 0;
for (const binary of runtimeManifest.binaries ?? []) {
  const file = path.join(binDir, binaryName(binary.id));
  const available = fs.existsSync(file);
  if (binary.required && !available) missingRequired += 1;
  rows.push({
    component: binary.id,
    required: Boolean(binary.required),
    available,
    location: path.relative(root, file),
  });
}

console.log("C.le.VideoSR runtime verification");
console.log(`Runtime: ${runtimeDir}`);
console.log(`Models:  ${modelDir}`);
console.table(rows);
console.log(`Model profiles: ${(modelManifest.models ?? []).length}`);

if (!fs.existsSync(modelDir)) {
  console.log("Model payload directory is not present; this is expected in a source-only checkout.");
}

if (missingRequired > 0) {
  const message = `${missingRequired} required runtime component(s) are not bundled.`;
  if (strict) {
    console.error(message);
    process.exitCode = 1;
  } else {
    console.log(`${message} Non-strict source validation will continue.`);
  }
}
