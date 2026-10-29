#!/usr/bin/env bash
#
# Re-download the vendored real-world ADR corpus at its pinned commits.
#
# Normal development does not run this: the files are committed under fixtures/corpus/ and the
# tests read them from disk, so the test suite never touches the network. This script exists to
# refresh the corpus deliberately, and to document exactly which commits it came from.
set -euo pipefail

MADRLINT_SHA=76da438b4bde3e583118836b338423f2ed56f2b2
ADR_MANAGER_SHA=ad80d5c19ccdd221c83499d9498f5eeb70dded66
LOG4BRAINS_SHA=17e32021a8c5130386f17e921d4efa6da7709a66

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
CORPUS="$ROOT/fixtures/corpus"

fetch() {
  local url=$1
  local destination=$2
  mkdir -p "$(dirname "$destination")"
  curl -sfL --retry 3 --max-time 60 "$url" -o "$destination"
}

fetch_madrlint() {
  local destination="$CORPUS/madrlint"
  local api="https://api.github.com/repos/adr/madrlint/git/trees/$MADRLINT_SHA"
  local samples
  samples=$(curl -sfL --retry 3 --max-time 60 "$api?recursive=1" |
    python3 -c 'import json,sys; print("\n".join(e["path"] for e in json.load(sys.stdin)["tree"] if e["path"].startswith("madr-samples/") and e["type"] == "blob" and e["path"].endswith(".md")))')
  if [ "$(printf '%s\n' "$samples" | wc -l | tr -d ' ')" != "25" ]; then
    echo "expected 25 madrlint samples, found: $samples" >&2
    exit 1
  fi
  while IFS= read -r path; do
    fetch "https://raw.githubusercontent.com/adr/madrlint/$MADRLINT_SHA/$path" "$destination/$(basename "$path")"
  done <<<"$samples"
  fetch "https://raw.githubusercontent.com/adr/madrlint/$MADRLINT_SHA/LICENSE" "$destination/LICENSE"
}

fetch_adr_manager() {
  local destination="$CORPUS/adr-manager"
  while IFS= read -r path; do
    fetch "https://raw.githubusercontent.com/adr/adr-manager/$ADR_MANAGER_SHA/docs/adr/$path" "$destination/$path"
  done <<'EOF'
0000-use-markdown-architectural-decision-records.md
0001-use-vue.js.md
0002-use-antlr-for-parsing-adrs.md
0003-use-pizzly-as-backend.md
0004-implement-a-global-store.md
EOF
  fetch "https://raw.githubusercontent.com/adr/adr-manager/$ADR_MANAGER_SHA/LICENSE" "$destination/LICENSE"
}

fetch_log4brains() {
  local destination="$CORPUS/log4brains"
  while IFS= read -r path; do
    fetch "https://raw.githubusercontent.com/thomvaill/log4brains/$LOG4BRAINS_SHA/$path" "$destination/$path"
  done <<'EOF'
docs/adr/20200924-use-markdown-architectural-decision-records.md
docs/adr/20200925-multi-packages-architecture-in-a-monorepo-with-yarn-and-lerna.md
docs/adr/20200925-use-prettier-eslint-airbnb-for-the-code-style.md
docs/adr/20200926-use-the-adr-number-as-its-unique-id.md
docs/adr/20200927-avoid-default-exports.md
docs/adr/20201016-use-the-adr-slug-as-its-unique-id.md
docs/adr/20201026-the-core-api-is-responsible-for-enhancing-the-adr-markdown-body-with-mdx.md
docs/adr/20201103-use-lunr-for-search.md
docs/adr/20210113-distribute-log4brains-as-a-global-npm-package.md
docs/adr/20240926-transition-to-simplified-git-flow.md
docs/adr/20241217-switch-back-to-github-flow.md
packages/core/docs/adr/20201002-use-explicit-architecture-and-ddd-for-the-core-api.md
packages/core/docs/adr/20201003-markdown-parsing-is-part-of-the-domain.md
packages/core/docs/adr/20201027-adr-link-resolver-in-the-domain.md
EOF
  fetch "https://raw.githubusercontent.com/thomvaill/log4brains/$LOG4BRAINS_SHA/LICENSE" "$destination/LICENSE"
}

fetch_madrlint
fetch_adr_manager
fetch_log4brains

echo "corpus refreshed under $CORPUS"
