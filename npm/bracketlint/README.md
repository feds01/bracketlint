# bracketlint

A linter and formatter for Django, Jinja, Liquid, Nunjucks and Twig templates.

```sh
npm install --save-dev @feds01/bracketlint
npx bracketlint check templates/
npx bracketlint fmt templates/
```

See the [README](https://github.com/feds01/bracketlint#readme) for the
commands and their options.

## How it is installed

Bracketlint is a native binary. This package has none itself, nor any install
script: the binary for each platform is in a package of its own, which this
one lists as an optional dependency, and the package manager installs only
the one for your platform:

| Package | Platform |
|---|---|
| `@feds01/bracketlint-darwin-arm64` | macOS, Apple silicon |
| `@feds01/bracketlint-darwin-x64` | macOS, Intel |
| `@feds01/bracketlint-linux-arm64` | Linux, ARM64 (static, any libc) |
| `@feds01/bracketlint-linux-x64` | Linux, x86-64 (static, any libc) |
| `@feds01/bracketlint-win32-x64` | Windows, x86-64 |

So it installs offline from a package manager's cache, the lockfile's
integrity hashes cover the binary, and bun needs no `trustedDependencies`
entry for it. Don't leave optional dependencies out of the install, e.g. with
`npm install --omit=optional`, or the `bracketlint` command has no binary to
run.

On another platform, build bracketlint from source with
`cargo install --locked --git https://github.com/feds01/bracketlint bracketlint`,
and set `BRACKETLINT_BINARY` to the path of the binary.

## Licence

MIT
