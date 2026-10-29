#!/usr/bin/env bash
# Downloads and verifies the release asset resolved by gh-action-resolve.sh, unless the
# actions/cache step already restored it. Always exposes the binary on PATH and sets the action's
# `version` / `path` outputs, cache hit or not.
set -euo pipefail

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

bin_name="adrlint"
if [ "$RUNNER_OS" = "Windows" ]; then
  bin_name="adrlint.exe"
fi

if [ "${CACHE_HIT:-}" != "true" ]; then
  mkdir -p "$BIN_DIR"
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  (
    cd "$work"

    echo "Downloading ${ASSET} (${TAG}) from ${REPO}..."
    gh release download "$TAG" --repo "$REPO" -p "$ASSET" -p "$CHECKSUMS"

    expected="$(awk -v f="$ASSET" '$2 == f { print $1 }' "$CHECKSUMS")"
    if [ -z "$expected" ]; then
      echo "::error::No checksum entry for ${ASSET} in ${CHECKSUMS}"
      exit 1
    fi

    actual="$(sha256 "$ASSET")"
    if [ "$actual" != "$expected" ]; then
      echo "::error::Checksum mismatch for ${ASSET}: expected ${expected}, got ${actual}"
      exit 1
    fi
    echo "Checksum OK (sha256): ${actual}"

    # Unpack into a private directory and publish only the expected member. Extracting the whole
    # archive into BIN_DIR would let a malicious or malformed archive drop arbitrary paths into a
    # directory that is then appended to PATH.
    extract_dir="$work/extract"
    mkdir -p "$extract_dir"

    case "$EXT" in
    tar.gz)
      tar -xzf "$ASSET" -C "$extract_dir" "$bin_name"
      ;;
    zip)
      if command -v unzip >/dev/null 2>&1; then
        unzip -q -o "$ASSET" "$bin_name" -d "$extract_dir"
      else
        python3 - "$ASSET" "$extract_dir" "$bin_name" <<'PY'
import os
import shutil
import sys
import zipfile

archive, destination, member = sys.argv[1:4]
with zipfile.ZipFile(archive) as handle:
    for name in handle.namelist():
        if os.path.basename(name) != member:
            continue
        with handle.open(name) as source, open(os.path.join(destination, member), "wb") as target:
            shutil.copyfileobj(source, target)
        break
    else:
        sys.exit(f"{member} not found in {archive}")
PY
      fi
      ;;
    *)
      echo "::error::Unknown archive extension: ${EXT}"
      exit 1
      ;;
    esac

    extracted="$(find "$extract_dir" -type f -name "$bin_name" | head -n 1)"
    if [ -z "$extracted" ]; then
      echo "::error::${ASSET} does not contain ${bin_name}"
      exit 1
    fi
    mv -f "$extracted" "${BIN_DIR}/${bin_name}"
  )

  if [ "$RUNNER_OS" != "Windows" ]; then
    chmod +x "${BIN_DIR}/${bin_name}"
  fi
else
  echo "adrlint ${TAG} already cached at ${BIN_DIR}"
fi

echo "$BIN_DIR" >>"$GITHUB_PATH"
{
  echo "version=${TAG#v}"
  echo "path=${BIN_DIR}/${bin_name}"
} >>"$GITHUB_OUTPUT"
