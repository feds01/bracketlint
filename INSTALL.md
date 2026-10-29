# Installing Bracketlint

Bracketlint is a single binary, for macOS, Linux and Windows. Every
[GitHub release](https://github.com/feds01/bracketlint/releases) has prebuilt
binaries and installers, and is published to npm and to a Homebrew tap. None
of them needs a Rust toolchain.

## npm

To add Bracketlint to a project's dev dependencies, and run it with `npx`:

```sh
npm install --save-dev @feds01/bracketlint
npx bracketlint check templates/
```

Install it with `--global` to have `bracketlint` everywhere.

The binary for each platform is in a package of its own, such as
`@feds01/bracketlint-darwin-arm64` or `@feds01/bracketlint-linux-x64`, which
`@feds01/bracketlint` lists as optional dependencies, so the package manager
installs only the one for your platform. There is no install script, and
nothing is downloaded when Bracketlint first runs:

- bun installs it without an entry in `trustedDependencies`;
- the lockfile's integrity hashes cover the binary;
- it installs offline from the package manager's cache.

There are packages for macOS (Apple silicon and Intel), Linux (ARM64 and
x86-64, static binaries that run with glibc or musl) and Windows (x86-64).
Leaving optional dependencies out, e.g. with `npm install --omit=optional`,
leaves out the binary too, and `bracketlint` then says which package to add.
On other platforms, build Bracketlint [from source](#from-source) and set
`BRACKETLINT_BINARY` to the path of the binary.

## Homebrew

On macOS or Linux:

```sh
brew install feds01/tap/bracketlint
```

## Shell installer

On macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/feds01/bracketlint/releases/latest/download/bracketlint-installer.sh | sh
```

It puts `bracketlint` in `~/.local/bin`, or `$XDG_BIN_HOME` if that is set.

## PowerShell installer

On Windows:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/feds01/bracketlint/releases/latest/download/bracketlint-installer.ps1 | iex"
```

## Prebuilt binaries

Download the archive for your platform from the
[latest release](https://github.com/feds01/bracketlint/releases/latest), unpack
it, and put `bracketlint` on your `PATH`. In CI, the GitHub CLI fetches it:

```sh
gh release download --repo feds01/bracketlint --pattern 'bracketlint-x86_64-unknown-linux-musl.tar.xz'
```

## From source

With a Rust toolchain:

```sh
cargo install --locked --git https://github.com/feds01/bracketlint bracketlint
```

## Check the installation

```sh
bracketlint version
```
