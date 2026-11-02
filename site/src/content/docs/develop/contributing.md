---
title: Contributing
description: How to propose changes to Dogubako, the checks to run, and what makes a good tool contribution.
sidebar:
  order: 6
---

Dogubako is free software under the AGPL-3.0-or-later, developed on
[GitHub](https://github.com/mischa-dev/dogubako). Bug reports, tool ideas,
fixes and docs improvements are all welcome.

## Reporting

- **Bugs:** open an issue with what you ran (app, CLI command or API call),
  what you expected, what happened, and the output of `dogubako --version` and
  `dogubako deps`.
- **Security issues:** report them privately through
  [GitHub security advisories](https://github.com/mischa-dev/dogubako/security/advisories/new),
  not public issues. See [Security](/project/security/#reporting-vulnerabilities).
- **Tool ideas:** describe the job to be done and which mature library or
  program could do it well.

## Making a change

1. Fork the repository and create a branch.
2. Make the change, with tests. See [Building](/develop/building/) to get set
   up.
3. Run the checks for what you touched:

   ```sh
   cargo fmt --all
   cargo clippy -p <crate> --all-targets -- -D warnings
   cargo test -p <crate>
   cd web && npx vitest run && npm run build      # web changes
   cd site && npm run build                       # docs changes
   ```

4. Open a pull request describing what changed and why. CI runs the web,
   Rust (three operating systems), desktop and Docker builds.

## What makes a good tool

- It **really works**. No placeholders, no fake results. If part of it can't
  be done well, leave that part out and say so.
- It reuses a **mature library or program** with an AGPL-compatible license,
  rather than reimplementing one.
- It is **safe**: no shell, guarded arguments, limits on decompression and
  image size, SSRF-guarded network access, previews before destructive
  changes. See [Writing a Rust tool](/develop/writing-tools/#rules-of-the-house).
- It has a **clear spec**: a one-sentence description, keywords people
  actually search for, sensible defaults and rare options under Advanced.
- It has **tests** that generate their own fixtures and skip gracefully when
  an engine isn't installed.
- The [tool catalog](/tools/) is regenerated (`npm run gen:tools` in `site/`)
  so the docs list it.

## Licensing of contributions

Contributions are accepted under the project's license, AGPL-3.0-or-later.
New dependencies must have compatible licenses: MIT, Apache-2.0, BSD, ISC,
Zlib, MPL-2.0, LGPL and GPL are fine; non-commercial or proprietary licenses
are not. See [Licensing](/project/licensing/).
