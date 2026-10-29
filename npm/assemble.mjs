#!/usr/bin/env node
// Assembles the npm packages of a release from its archives: a package per
// platform with just that platform's binary, e.g.
// `@feds01/bracketlint-darwin-arm64`, and `@feds01/bracketlint`, which depends
// on them all, optionally, and runs the one that the package manager installed.
//
//     node npm/assemble.mjs <version> <archives> <out> [target...]
//
// `archives` is a directory with dist's archives of the release, e.g.
// `bracketlint-aarch64-apple-darwin.tar.xz`, and each package is written to a
// directory of `out`: `bracketlint` and `bracketlint-<os>-<cpu>`. With
// targets, only their platform packages are assembled; the main package still
// depends on every platform's.

import { execFileSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** The platform of each target that a release has a binary for, as Node names it. */
const PLATFORMS = {
  "aarch64-apple-darwin": { os: "darwin", cpu: "arm64", label: "macOS on Apple silicon" },
  "x86_64-apple-darwin": { os: "darwin", cpu: "x64", label: "macOS on Intel" },
  // The Linux binaries are static, so they run with glibc as well as musl,
  // and their packages have no `libc` field.
  "aarch64-unknown-linux-musl": { os: "linux", cpu: "arm64", label: "Linux on ARM64" },
  "x86_64-unknown-linux-musl": { os: "linux", cpu: "x64", label: "Linux on x86-64" },
  "x86_64-pc-windows-msvc": { os: "win32", cpu: "x64", label: "Windows on x86-64" },
};

/** A version as npm takes it, e.g. `0.1.0` or `0.2.0-beta.1`. */
const VERSION = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/;

const here = dirname(fileURLToPath(import.meta.url));
const licence = join(here, "..", "LICENSE");

/** Print `message` to stderr, and exit with an error. */
function fail(message) {
  process.stderr.write(`error: ${message}\n`);
  process.exit(1);
}

/** Write `value` to `path` as formatted JSON. */
function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

/** The path of the file named `name` in `dir` or below it, if there is one. */
function findFile(dir, name) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      const found = findFile(path, name);
      if (found) {
        return found;
      }
    } else if (entry.name === name) {
      return path;
    }
  }
  return undefined;
}

/** Copy the binary named `binary` out of the archive at `archive`, to `to`. */
function extractBinary(archive, binary, to) {
  const scratch = mkdtempSync(join(tmpdir(), "bracketlint-npm-"));
  try {
    if (archive.endsWith(".zip")) {
      execFileSync("unzip", ["-q", archive, "-d", scratch]);
    } else {
      execFileSync("tar", ["-xf", archive, "-C", scratch]);
    }
    const found = findFile(scratch, binary);
    if (!found) {
      fail(`${archive} has no \`${binary}\``);
    }
    copyFileSync(found, to);
    chmodSync(to, 0o755);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

// 1. Read the arguments.
const [version, archives, out, ...targets] = process.argv.slice(2);
if (!version || !archives || !out) {
  fail("usage: node npm/assemble.mjs <version> <archives> <out> [target...]");
}
if (!VERSION.test(version)) {
  fail(`\`${version}\` isn't a version, like 0.1.0`);
}
for (const target of targets) {
  if (!(target in PLATFORMS)) {
    fail(`\`${target}\` isn't a target with an npm package, which are: ${Object.keys(PLATFORMS).join(", ")}`);
  }
}

// 2. The main package, from its template, depending on every platform's
//    package at exactly its own version.
const template = join(here, "bracketlint");
const main = JSON.parse(readFileSync(join(template, "package.json"), "utf8"));
const mainDir = resolve(out, "bracketlint");
rmSync(mainDir, { recursive: true, force: true });
mkdirSync(join(mainDir, "bin"), { recursive: true });
copyFileSync(join(template, "bin", "bracketlint.js"), join(mainDir, "bin", "bracketlint.js"));
chmodSync(join(mainDir, "bin", "bracketlint.js"), 0o755);
copyFileSync(join(template, "README.md"), join(mainDir, "README.md"));
copyFileSync(licence, join(mainDir, "LICENSE"));
main.version = version;
main.optionalDependencies = Object.fromEntries(
  Object.values(PLATFORMS).map(({ os, cpu }) => [`${main.name}-${os}-${cpu}`, version]),
);
writeJson(join(mainDir, "package.json"), main);
console.log(`${main.name}@${version}: ${mainDir}`);

// 3. A package for each platform, with the binary from its archive.
for (const target of targets.length > 0 ? targets : Object.keys(PLATFORMS)) {
  const { os, cpu, label } = PLATFORMS[target];
  const binary = os === "win32" ? "bracketlint.exe" : "bracketlint";
  const archive = ["tar.xz", "zip"]
    .map((extension) => resolve(archives, `bracketlint-${target}.${extension}`))
    .find((path) => existsSync(path));
  if (!archive) {
    fail(`there is no archive for ${target} in ${archives}, \`bracketlint-${target}.tar.xz\` or \`.zip\``);
  }

  const name = `${main.name}-${os}-${cpu}`;
  const dir = resolve(out, `bracketlint-${os}-${cpu}`);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  extractBinary(archive, binary, join(dir, binary));
  copyFileSync(licence, join(dir, "LICENSE"));
  writeFileSync(
    join(dir, "README.md"),
    `# ${name}\n\nThe bracketlint binary for ${label}. Install [\`${main.name}\`](https://www.npmjs.com/package/${main.name}) instead, which installs this package on this platform and runs its binary.\n`,
  );
  writeJson(join(dir, "package.json"), {
    name,
    version,
    description: `The bracketlint binary for ${label}`,
    homepage: main.homepage,
    repository: main.repository,
    license: main.license,
    author: main.author,
    os: [os],
    cpu: [cpu],
    // Yarn's Plug'n'Play leaves the binary on disk, where it can run.
    preferUnplugged: true,
  });
  console.log(`${name}@${version}: ${dir}`);
}
