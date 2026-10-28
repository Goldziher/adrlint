# Security Policy

## Supported Versions

Security fixes are applied to the latest release from the default branch.

## Reporting a Vulnerability

Do not open a public issue for a security vulnerability. Use GitHub's
[private vulnerability reporting](https://github.com/Goldziher/adrlint/security/advisories/new) to
contact the maintainers privately.

Include the affected version, reproduction steps, impact, and any suggested remediation.

## Scope

adrlint reads untrusted Markdown. Relevant reports include path traversal through `adr.dir`, symlink
escapes while scanning, panics on malformed front matter or byte offsets inside multi-byte text, and
a release binary that does not match its published checksum.
