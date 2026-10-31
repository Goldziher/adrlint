#!/usr/bin/env bash
set -euo pipefail

# update-scoop-manifest.sh <version> <checksums-file>
# Reads the release checksums file (`adrlint_<version>_checksums.txt`) and prints the Scoop
# manifest. Both Windows MSVC archives are published, so the manifest has a 64-bit and an arm64 entry.

if [ $# -ne 2 ]; then
  echo "Usage: $0 <version> <checksums-file>" >&2
  exit 1
fi

VERSION="$1"
SUMS="$2"

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
  echo "invalid version '$VERSION': expected <major>.<minor>.<patch> with an optional -<suffix>" >&2
  exit 1
fi

if [ ! -r "$SUMS" ]; then
  echo "cannot read checksums file '$SUMS'" >&2
  exit 1
fi

TRIPLE="x86_64-pc-windows-msvc"
ARCHIVE="adrlint-${TRIPLE}.zip"
ARM64_TRIPLE="aarch64-pc-windows-msvc"
ARM64_ARCHIVE="adrlint-${ARM64_TRIPLE}.zip"
BASE="https://github.com/Goldziher/adrlint/releases/download/v${VERSION}"

# Checksum lines are `<sha256>  <name>`, optionally with a `*` binary-mode marker before the name.
HASH="$(awk -v f="$ARCHIVE" '{ n = $NF; sub(/^[*]/, "", n); if (n == f) print $1 }' "$SUMS")"
ARM64_HASH="$(awk -v f="$ARM64_ARCHIVE" '{ n = $NF; sub(/^[*]/, "", n); if (n == f) print $1 }' "$SUMS")"

if [ -z "$HASH" ]; then
  echo "missing checksum for ${ARCHIVE} in ${SUMS}" >&2
  exit 1
fi

if [ -z "$ARM64_HASH" ]; then
  echo "missing checksum for ${ARM64_ARCHIVE} in ${SUMS}" >&2
  exit 1
fi

if ! [[ "$HASH" =~ ^[0-9a-f]{64}$ ]]; then
  echo "malformed checksum for ${ARCHIVE} in ${SUMS}: '${HASH}' is not a sha256 digest" >&2
  exit 1
fi

if ! [[ "$ARM64_HASH" =~ ^[0-9a-f]{64}$ ]]; then
  echo "malformed checksum for ${ARM64_ARCHIVE} in ${SUMS}: '${ARM64_HASH}' is not a sha256 digest" >&2
  exit 1
fi

# `\$version` is a Scoop placeholder that Scoop substitutes during `scoop update`; only the
# literal manifest values come from the shell.
cat <<EOF
{
    "version": "${VERSION}",
    "description": "A fast, opinionated linter and formatter for MADR architecture decision records",
    "homepage": "https://github.com/Goldziher/adrlint",
    "license": "MIT",
    "architecture": {
        "64bit": {
            "url": "${BASE}/${ARCHIVE}",
            "hash": "${HASH}"
        },
        "arm64": {
            "url": "${BASE}/${ARM64_ARCHIVE}",
            "hash": "${ARM64_HASH}"
        }
    },
    "bin": "adrlint.exe",
    "checkver": {
        "github": "https://github.com/Goldziher/adrlint"
    },
    "autoupdate": {
        "architecture": {
            "64bit": {
                "url": "https://github.com/Goldziher/adrlint/releases/download/v\$version/${ARCHIVE}"
            },
            "arm64": {
                "url": "https://github.com/Goldziher/adrlint/releases/download/v\$version/${ARM64_ARCHIVE}"
            }
        },
        "hash": {
            "url": "https://github.com/Goldziher/adrlint/releases/download/v\$version/adrlint_\$version_checksums.txt"
        }
    }
}
EOF
