# Security Policy

## Supported versions

Only the latest release receives security fixes.

| Version | Supported |
| ------- | --------- |
| Latest release (see [Releases](https://github.com/fetag/lumiere/releases)) | ✅ |
| Older releases | ❌ |

## Reporting a vulnerability

The preferred way to report a security issue is through
[GitHub's private vulnerability reporting](https://github.com/fetag/lumiere/security/advisories/new),
which keeps the issue hidden until a fix is available.

Alternatively, email the maintainer (see [README](README.md)) with:

- the affected version(s),
- the platform (macOS / Linux / Windows) and OS version,
- steps to reproduce or a minimal example folder/image set,
- any impact assessment you have.

Please do not open a public issue for a security concern until it has been
discussed privately or a fix is released.

## Response guidelines

- Acknowledgement within 3 business days.
- A fix targets the next patch release; urgent issues may get an out-of-band
  release.

## Notes on the threat surface

Lumiere reads and writes files in folders you open, decodes untrusted image
data (including via the `image` crate and macOS ImageIO), and moves items to
the system Trash after confirmation. Keep those permissions in mind when
assessing impact.
