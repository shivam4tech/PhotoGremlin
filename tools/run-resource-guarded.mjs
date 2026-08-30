import { existsSync, readFileSync } from "node:fs";
import { freemem } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const GIB = 1024 ** 3;
const MINIMUM_AVAILABLE_BYTES = 6 * GIB;
const TOOL_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(TOOL_DIR);
const TAURI_CLI = path.join(REPO_ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js");

const TASKS = {
  "rust-test": {
    command: "cargo",
    args: ["test"],
    cwd: path.join(REPO_ROOT, "src-tauri"),
    label: "Rust test suite",
  },
  "app-debug": {
    command: process.execPath,
    args: [TAURI_CLI, "build", "--debug"],
    cwd: REPO_ROOT,
    label: "debug app bundle",
  },
  "app-release": {
    command: process.execPath,
    args: [TAURI_CLI, "build"],
    cwd: REPO_ROOT,
    label: "release app bundle",
  },
};

export function parseMemAvailable(contents) {
  const match = /^MemAvailable:\s+(\d+)\s+kB$/m.exec(contents);
  return match ? Number(match[1]) * 1024 : null;
}

export function availableMemoryBytes() {
  if (process.platform === "linux") {
    try {
      const available = parseMemAvailable(readFileSync("/proc/meminfo", "utf8"));
      if (available !== null) return available;
    } catch {
      // Fall back to Node's cross-platform free-memory reading.
    }
  }
  return freemem();
}

export function shouldBlockForMemory(available, overrideValue) {
  return available < MINIMUM_AVAILABLE_BYTES && overrideValue !== "1";
}

export function formatGib(bytes) {
  return (bytes / GIB).toFixed(1);
}

export function resolveTask(taskName) {
  const task = TASKS[taskName];
  if (!task) {
    throw new Error(`Unknown guarded task: ${taskName ?? "(missing)"}`);
  }
  return task;
}

export function buildSystemdArgs(task) {
  return [
    "--user",
    "--scope",
    "--collect",
    "--quiet",
    "--property=MemoryAccounting=yes",
    "--property=MemoryHigh=3G",
    "--property=MemoryMax=4G",
    "--property=MemorySwapMax=1G",
    "--",
    task.command,
    ...task.args,
  ];
}

function findSystemdRun() {
  if (process.platform !== "linux") return null;
  const memoryController = "/sys/fs/cgroup/cgroup.controllers";
  const runtimeDirectory = process.env.XDG_RUNTIME_DIR;
  const hasUserManager = runtimeDirectory
    ? existsSync(path.join(runtimeDirectory, "systemd", "private"))
    : false;
  if (!hasUserManager || !existsSync(memoryController)) return null;

  for (const candidate of ["/usr/bin/systemd-run", "/bin/systemd-run"]) {
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

function run(task) {
  const available = availableMemoryBytes();
  if (shouldBlockForMemory(available, process.env.PHOTOGREMLIN_ALLOW_LOW_MEMORY)) {
    console.error(
      `Refusing to start the ${task.label}: only ${formatGib(available)} GiB ` +
        "is available; 6.0 GiB is required. Close memory-heavy apps and retry, " +
        "or set PHOTOGREMLIN_ALLOW_LOW_MEMORY=1 to bypass this preflight check.",
    );
    return 2;
  }

  const systemdRun = findSystemdRun();
  const guarded = systemdRun !== null;
  if (guarded) {
    console.log(
      `Starting the ${task.label} with ${formatGib(available)} GiB available ` +
        "(Linux build scope: high 3 GiB, max 4 GiB, swap max 1 GiB).",
    );
  } else {
    console.warn(
      `Starting the ${task.label} with ${formatGib(available)} GiB available. ` +
        "A compatible user-systemd memory scope is unavailable; Cargo's single-job " +
        "and lean-debug guards still apply.",
    );
  }

  const result = guarded
    ? spawnSync(systemdRun, buildSystemdArgs(task), { cwd: task.cwd, stdio: "inherit" })
    : spawnSync(task.command, task.args, { cwd: task.cwd, stdio: "inherit" });

  if (result.error) {
    console.error(`Could not start the ${task.label}: ${result.error.message}`);
    return 1;
  }
  if (result.signal) {
    console.error(
      `${task.label} stopped by signal ${result.signal}. ` +
        (guarded ? "The build may have reached its 4 GiB memory ceiling." : ""),
    );
    return 1;
  }
  if (result.status !== 0 && guarded) {
    console.error(
      `${task.label} failed inside the guarded scope (exit ${result.status}). ` +
        "If the log reports an out-of-memory kill, reduce other memory use; the guard " +
        "contained the failure so Codex and other applications remain responsive.",
    );
  }
  return result.status ?? 1;
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = run(resolveTask(process.argv[2]));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  }
}
