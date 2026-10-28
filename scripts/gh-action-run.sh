#!/usr/bin/env bash
# Builds the adrlint invocation from the action's inputs and runs it, with a problem matcher
# registered around the call so violations land as inline annotations.
set -euo pipefail

# ARGS is a space-separated argument string (e.g. "lint . --format github"). Splitting it on
# whitespace is the point, not an accident. Globbing is disabled around it so a glob adrlint itself
# understands is not pre-expanded by the shell first.
set -f
# shellcheck disable=SC2206
parsed_args=(${ARGS})
set +f

echo "::add-matcher::${MATCHER_PATH}"

cd "$WORKING_DIRECTORY"
echo "Running: adrlint ${parsed_args[*]}"
set +e
adrlint "${parsed_args[@]}"
status=$?
set -e

echo "::remove-matcher owner=adrlint::"

exit "$status"
