# Maintainers

This file lists the people responsible for reviewing changes, triaging issues,
and cutting releases of Project Lifeline. It is the canonical list referenced by
[`SECURITY.md`](SECURITY.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Current maintainers

| Name | GitHub | Areas |
|---|---|---|
| Project Lifeline maintainers | [@nometria](https://github.com/nometria) | Everything — core protocol, crypto, transport, apps |

> This is an early-stage project. As contributors take ownership of areas, add
> them here with the subsystems they steward (e.g. `crates/core` crypto,
> `crates/transport` bearers, `saas/`, `mobile/`).

## Responsibilities

- **Review** pull requests for correctness, security, and the project's bar for
  honesty about limitations (see [`CONTRIBUTING.md`](CONTRIBUTING.md)).
- **Triage** issues: apply labels, confirm reproductions, set priority.
- **Security**: respond to private vulnerability reports within 72 hours (see
  [`SECURITY.md`](SECURITY.md)); no security discussion happens in public issues.
- **Releases**: keep [`CHANGELOG.md`](CHANGELOG.md) current and tag releases.

## How decisions are made

Until a formal governance model is adopted, changes are accepted by maintainer
review and consensus. Anything that changes the wire protocol, the cryptography,
or a documented security property requires explicit sign-off and a description of
the threat-model impact in the PR.

## Reaching us

- **General questions / ideas:** open a [GitHub Discussion](https://github.com/nometria/project-lifeline/discussions).
- **Bugs:** open a [GitHub Issue](https://github.com/nometria/project-lifeline/issues) (not for security — see below).
- **Security vulnerabilities:** use private reporting per [`SECURITY.md`](SECURITY.md).
