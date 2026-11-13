# AGENTS.md

Guidance for AI agents (Claude Code and others) working in this repository.

## Confidentiality: this repo is mirrored to public GitHub

The primary remote is private GitLab (`gitlab.fetag.net`), but the repo is also
pushed to **public** GitHub (`github.com/fetag/lumiere`), and ordinary pushes
transfer the full commit history. Anything that ever lands in a tracked file —
or in a commit later rewritten away from `main` but still present during a
GitHub sync — becomes public.

**Never put machine-local or private operational detail in tracked files**
(docs/, CLAUDE.md, AGENTS.md, scripts, code comments), including:

- SSH key names or paths, credentials, tokens, certificates
- Build-machine internals (e.g. which non-default ssh identity a machine's
  `git pull` requires, local paths outside the repo, machine-specific gotchas)
- Anything whose disclosure would help someone reach or act on the
  maintainer's machines or accounts

Machine-local operational detail belongs in the agent's local memory or on
the machine itself — not in the repo. If a task seems to require documenting
such detail in the repo, put it in agent memory instead and tell the
maintainer.

**Precedent (2026-11-13):** a note naming a real SSH key file on a build
machine was committed to `docs/release-env.md` and had to be rewritten out of
history before the GitHub sync.

**Before committing any docs:** check that nothing private is included; when
in doubt, ask the maintainer. Note that earlier commits already published the
machine names and workspace path conventions in `docs/release-env.md`; do not
widen that surface, and scrubbing the existing content is a maintainer
decision — do not rewrite shared history unilaterally.
