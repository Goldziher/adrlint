---
type: Decision
title: Version Synchronization
x-ai-rulez:
  kind: rule
  id: version-synchronization
  metadata:
    priority: high
---

## Version Synchronization

All version numbers are updated together when releasing.

### Version locations

1. `Cargo.toml` — `version` under `[package]`
2. `npm-package/package.json` — `version`
3. `pip-package/pyproject.toml` — `version` under `[project]`
4. `pip-package/adrlint/__init__.py` — `__version__`

### Rules

- All four carry the same version when preparing a release.
- Semantic versioning, `MAJOR.MINOR.PATCH`. The crate is pre-1.0: rule additions and CLI changes are
  minor, fixes are patch, and a breaking rename of a rule ID or a flag is minor until 1.0.
- Update all four in a single commit (`chore(release): vX.Y.Z`).
- The git tag is `vX.Y.Z`; PyPI uses `X.Y.ZrcN` for release candidates.
- Never tag before the four files agree; `.github/workflows/publish.yaml` verifies them and fails the
  release otherwise.
- The lock file's `canonical` field is versioned separately from the crate. Changing the serializer
  bumps it; the crate version does not substitute for it.
