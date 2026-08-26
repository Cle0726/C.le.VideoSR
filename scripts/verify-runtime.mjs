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

function approved(status) {
  return ["approved", "reviewed", "compatible"].includes(String(status ?? "").toLowerCase());
}

function directPair(directory, prefix = null) {
  if (!fs.existsSync(directory) || !fs.statSync(directory).isDirectory()) return false;
  const params = new Set();
  const bins = new Set();
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    const extension = path.extname(entry.name).toLowerCase();
    const stem = path.basename(entry.name, extension);
    if (prefix && !stem.startsWith(prefix)) continue;
    if (extension === ".param") params.add(stem);
    if (extension === ".bin") bins.add(stem);
  }
  return [...params].some((stem) => bins.has(stem));
}

function findPairDirectory(directory, prefix = null, depth = 2) {
  if (!fs.existsSync(directory) || !fs.statSync(directory).isDirectory()) return null;
  if (directPair(directory, prefix)) return directory;
  if (depth <= 0) return null;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const found = findPairDirectory(path.join(directory, entry.name), prefix, depth - 1);
    if (found) return found;
  }
  return null;
}

function realEsrganPayloadPrefix(model) {
  // AnimeVideo v3 stores one NCNN payload pair per requested scale while the CLI
  // still receives the shared model name via `-n realesr-animevideov3`.
  if (model.model_stem === "realesr-animevideov3") {
    return `${model.model_stem}-x${model.scale}`;
  }
  return model.model_stem;
}

function modelPayloadDirectory(model, modelDir) {
  const realEsrgan = model.engine === "realesrgan-ncnn-vulkan";
  const candidates = realEsrgan
    ? [modelDir, path.join(modelDir, model.model_stem)]
    : [path.join(modelDir, model.model_stem)];
  const prefix = realEsrgan ? realEsrganPayloadPrefix(model) : null;
  const depth = realEsrgan ? 2 : 1;

  for (const candidate of candidates) {
    const found = findPairDirectory(candidate, prefix, depth);
    if (found) return found;
  }
  return null;
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

const binaryRows = [];
let strictFailures = 0;
for (const binary of runtimeManifest.binaries ?? []) {
  const file = path.join(binDir, binaryName(binary.id));
  const available = fs.existsSync(file);
  const licenseReady = approved(binary.license_review);
  if (strict && binary.required && !available) strictFailures += 1;
  if (strict && available && !licenseReady) strictFailures += 1;
  binaryRows.push({
    component: binary.id,
    required: Boolean(binary.required),
    available,
    license: binary.license_review ?? "unspecified",
    location: path.relative(root, file),
  });
}

const binaryById = new Map(binaryRows.map((row) => [row.component, row]));
const modelRows = [];
for (const model of modelManifest.models ?? []) {
  const payloadDir = modelPayloadDirectory(model, modelDir);
  const available = Boolean(payloadDir);
  const licenseReady = approved(model.license_status);
  const engineAvailable = Boolean(binaryById.get(model.engine)?.available);
  if (strict && model.bundled && !available) strictFailures += 1;
  if (strict && model.bundled && !licenseReady) strictFailures += 1;
  if (strict && model.bundled && !engineAvailable) strictFailures += 1;
  modelRows.push({
    model: model.id,
    engine: model.engine,
    bundled: Boolean(model.bundled),
    available,
    engineAvailable,
    license: model.license_status ?? "unspecified",
    location: payloadDir ? path.relative(root, payloadDir) : "missing",
  });
}

console.log("C.le.VideoSR runtime verification");
console.log(`Runtime: ${runtimeDir}`);
console.log(`Models:  ${modelDir}`);
console.log("Runtime binaries:");
console.table(binaryRows);
console.log("AI model payloads:");
console.table(modelRows);

const requiredMissing = binaryRows.filter((row) => row.required && !row.available).length;
const readyModels = modelRows.filter((row) => row.available && row.engineAvailable).length;
console.log(`Ready model profiles: ${readyModels}/${modelRows.length}`);

if (!fs.existsSync(modelDir)) {
  console.log("Model payload directory is not present; this is expected in a source-only checkout.");
}

if (strict) {
  if (strictFailures > 0) {
    console.error(`${strictFailures} strict release-readiness check(s) failed.`);
    process.exitCode = 1;
  } else {
    console.log("Strict runtime release-readiness checks passed.");
  }
} else if (requiredMissing > 0) {
  console.log(`${requiredMissing} required runtime component(s) are not bundled. Non-strict source/preview validation will continue.`);
}
