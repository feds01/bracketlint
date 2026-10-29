#!/usr/bin/env node
// The `bracketlint` command of the npm package. It runs the binary from the
// package for this platform, e.g. `@feds01/bracketlint-darwin-arm64`, which
// the package manager installed as one of its optional dependencies. The
// binary gets the arguments, stdin, stdout and stderr, and the command exits
// as the binary does.

"use strict";

const { spawn } = require("child_process");
const path = require("path");

const { name: main, version, optionalDependencies } = require("../package.json");

/** The exit code of a usage or internal error, as bracketlint's own. */
const ERROR_EXIT_CODE = 2;

/** The signals that are passed on to the binary, as when the launcher is stopped on its own. */
const FORWARDED_SIGNALS = ["SIGINT", "SIGTERM", "SIGHUP"];

/** Print `message` to stderr, and exit with an error. */
function fail(message) {
  process.stderr.write(`error: ${message}\n`);
  process.exit(ERROR_EXIT_CODE);
}

/** The path of the binary to run. */
function binaryPath() {
  // 1. A binary of the user's own, e.g. one built from source for a
  //    platform without a package.
  const override = process.env.BRACKETLINT_BINARY;
  if (override) {
    return override;
  }

  // 2. The package for this platform, which must be one that is published.
  const target = `${process.platform}-${process.arch}`;
  const platformPackage = `${main}-${target}`;
  if (!Object.prototype.hasOwnProperty.call(optionalDependencies, platformPackage)) {
    const supported = Object.keys(optionalDependencies)
      .map((name) => name.slice(main.length + 1))
      .join(", ");
    fail(
      `${main} has no binary for this platform, ${target}; it has binaries for ${supported}.\n` +
        "Build bracketlint from source with " +
        "`cargo install --locked --git https://github.com/feds01/bracketlint bracketlint`,\n" +
        "and set BRACKETLINT_BINARY to the path of the binary.",
    );
  }

  // 3. Where the package manager installed it.
  let manifest;
  try {
    manifest = require.resolve(`${platformPackage}/package.json`);
  } catch {
    fail(
      `${platformPackage}, which has the binary for this platform, isn't installed.\n` +
        `It is an optional dependency of ${main}, so it is left out when optional dependencies are,\n` +
        "e.g. with `npm install --omit=optional`. Install again without leaving them out,\n" +
        `or add ${platformPackage}@${version} to your dependencies.`,
    );
  }
  return path.join(
    path.dirname(manifest),
    process.platform === "win32" ? "bracketlint.exe" : "bracketlint",
  );
}

// 1. Run the binary with the launcher's arguments and streams.
const binary = binaryPath();
const child = spawn(binary, process.argv.slice(2), { stdio: "inherit" });

// 2. Pass on the signals that would stop the launcher alone, so that the
//    binary stops with it rather than running on.
const forward = (signal) => child.kill(signal);
for (const signal of FORWARDED_SIGNALS) {
  process.on(signal, forward);
}

child.on("error", (error) => fail(`could not run ${binary}: ${error.message}`));

// 3. Exit as the binary did: with its exit code, or by the signal that
//    stopped it.
child.on("exit", (code, signal) => {
  for (const forwarded of FORWARDED_SIGNALS) {
    process.removeListener(forwarded, forward);
  }
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code);
  }
});
