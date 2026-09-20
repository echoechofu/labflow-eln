import { execFileSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const appDirectory = path.resolve(scriptDirectory, "..");
const manifestPath = path.join(appDirectory, "src-tauri", "Cargo.toml");
const targetTriple = execFileSync(
  "rustc",
  ["+1.98.0", "--print", "host-tuple"],
  { encoding: "utf8" },
).trim();
const extension = process.platform === "win32" ? ".exe" : "";

if (!targetTriple) throw new Error("Rust did not report a host target triple.");

const binariesDirectory = path.join(appDirectory, "src-tauri", "binaries");
const destination = path.join(
  binariesDirectory,
  `labflow-mcp-${targetTriple}${extension}`,
);
mkdirSync(binariesDirectory, { recursive: true });

// tauri-build validates every externalBin path even while Cargo is building
// the sidecar itself. Bootstrap that first build with an ignored placeholder;
// a successful Cargo build replaces it below before Tauri packages the app.
if (!existsSync(destination)) {
  writeFileSync(destination, "");
  if (process.platform !== "win32") chmodSync(destination, 0o755);
}

execFileSync(
  "cargo",
  [
    "+1.98.0",
    "build",
    "--release",
    "--manifest-path",
    manifestPath,
    "--bin",
    "labflow-mcp",
  ],
  { cwd: appDirectory, stdio: "inherit" },
);

const source = path.join(
  appDirectory,
  "src-tauri",
  "target",
  "release",
  `labflow-mcp${extension}`,
);
if (!existsSync(source)) {
  throw new Error(`MCP binary was not produced at ${source}`);
}

const sidecarChanged =
  !existsSync(destination) ||
  !readFileSync(source).equals(readFileSync(destination));
if (sidecarChanged) copyFileSync(source, destination);
if (process.platform !== "win32") chmodSync(destination, 0o755);

const sizeMiB = (statSync(destination).size / 1024 / 1024).toFixed(1);
console.log(
  `${sidecarChanged ? "Prepared" : "Verified"} LabFlow MCP sidecar: ${destination} (${sizeMiB} MiB)`,
);
