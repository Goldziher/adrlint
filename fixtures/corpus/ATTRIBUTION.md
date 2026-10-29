# Vendored ADR corpus

These files are real-world Architecture Decision Records, copied byte-for-byte from the upstream
repositories below. They are test data only: nothing in this tree is edited, reformatted or
regenerated, so the corpus exercises adrlint against Markdown that other projects actually wrote
rather than against fixtures shaped to pass.

Refresh the trees with `scripts/fetch-corpus.sh`; normal development uses the committed copies and
needs no network.

## `madrlint/`

| | |
| --- | --- |
| Upstream | <https://github.com/adr/madrlint> |
| Commit | `76da438b4bde3e583118836b338423f2ed56f2b2` |
| Source path | `madr-samples/*.md` (25 files) |
| License | MIT, see `madrlint/LICENSE` |
| Contents | The sample ADR directory used as madrlint's conformance baseline |

The Java sources are deliberately not vendored; only the Markdown samples are.

## `adr-manager/`

| | |
| --- | --- |
| Upstream | <https://github.com/adr/adr-manager> |
| Commit | `ad80d5c19ccdd221c83499d9498f5eeb70dded66` |
| Source path | `docs/adr/*.md` (5 files) |
| License | MIT, see `adr-manager/LICENSE` |

## `log4brains/`

| | |
| --- | --- |
| Upstream | <https://github.com/thomvaill/log4brains> |
| Commit | `17e32021a8c5130386f17e921d4efa6da7709a66` |
| Source paths | `docs/adr/20*.md` (11 files) and `packages/core/docs/adr/*.md` (3 files) |
| License | Apache-2.0, see `log4brains/LICENSE` and `log4brains/NOTICE` |

`README.md`, `index.md` and `template.md` are upstream scaffolding rather than decisions and are
excluded. The original directory layout is preserved so relative links keep the meaning they had
upstream.

## Files

All Markdown files are copied unmodified. They are valid UTF-8. Where a tree is not appropriate as
an ADR directory (log4brains names files by date, `madrlint/` contains duplicate numbers) that is
the upstream reality and is exactly what the corpus is meant to test.
