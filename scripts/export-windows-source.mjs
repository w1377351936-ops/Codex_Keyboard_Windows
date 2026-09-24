import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, lstatSync, mkdirSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";

const root = resolve(import.meta.dirname, "..");
const destination = resolve(root, "artifacts", "Codex_Keyboard_Windows_source");
if (existsSync(destination)) {
  throw new Error(`Destination already exists: ${destination}`);
}

const rootFiles = new Set([
  ".gitignore",
  ".prettierignore",
  "Cargo.lock",
  "Cargo.toml",
  "LICENSE",
  "README.md",
  "package.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "rustfmt.toml",
  "tsconfig.base.json",
]);
const allowedPrefixes = [".github/", "app/", "firmware/", "scripts/", "docs/"];
const excludedFiles = new Set(["docs/Windows作业操作.md"]);
const entries = execFileSync(
  "git",
  ["-c", "core.quotepath=false", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
  { cwd: root },
)
  .toString("utf8")
  .split("\0")
  .filter(Boolean);

let copied = 0;
for (const path of entries) {
  const normalized = path.replaceAll("\\", "/");
  if (!rootFiles.has(normalized) && !allowedPrefixes.some((prefix) => normalized.startsWith(prefix))) continue;
  if (excludedFiles.has(normalized) || normalized.split("/").includes("__pycache__")) continue;
  if (isAbsolute(normalized) || normalized.split("/").includes("..")) throw new Error(`Unsafe path: ${normalized}`);
  const source = resolve(root, normalized);
  const target = resolve(destination, normalized);
  if (relative(root, source).startsWith(".." + sep) || relative(destination, target).startsWith(".." + sep)) {
    throw new Error(`Path escapes export root: ${normalized}`);
  }
  if (!lstatSync(source).isFile()) throw new Error(`Only regular files may be exported: ${normalized}`);
  mkdirSync(dirname(target), { recursive: true });
  copyFileSync(source, target);
  copied++;
}
console.log(`source_export=${destination}`);
console.log(`files_copied=${copied}`);
