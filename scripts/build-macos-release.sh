#!/usr/bin/env bash
# Build the distributable macOS app with a stable Developer ID identity.
#
# Accessibility permissions are bound to an application's code requirement.
# Allowing a release build to silently fall back to ad-hoc signing would make
# macOS regard every update as a different application and revoke that trust.

set -euo pipefail

readonly EXPECTED_BUNDLE_ID="pro.clipmaster.desktop"
readonly APP_PATH="src-tauri/target/release/bundle/macos/ClipMaster Pro.app"

fail() {
  printf 'release build aborted: %s\n' "$*" >&2
  exit 1
}

[[ "$(uname -s)" == "Darwin" ]] || fail "macOS is required to build the distributable app."
[[ -n "${APPLE_SIGNING_IDENTITY:-}" ]] || fail \
  "APPLE_SIGNING_IDENTITY is required. Refusing to create an ad-hoc-signed release."

if ! security find-identity -v -p codesigning | grep -Fq "\"${APPLE_SIGNING_IDENTITY}\""; then
  fail "the requested signing identity is not installed: ${APPLE_SIGNING_IDENTITY}"
fi

npx tauri build --bundles app

[[ -d "$APP_PATH" ]] || fail "expected app bundle was not produced: $APP_PATH"

signature_info="$(codesign -dvv "$APP_PATH" 2>&1)"
printf '%s\n' "$signature_info" | grep -Fq "Identifier=${EXPECTED_BUNDLE_ID}" || \
  fail "bundle identifier changed; expected ${EXPECTED_BUNDLE_ID}"
printf '%s\n' "$signature_info" | grep -Eq '^TeamIdentifier=.+$' || \
  fail "app has no TeamIdentifier; it was not signed with a stable Apple identity"
printf '%s\n' "$signature_info" | grep -Fq 'Authority=Developer ID Application:' || \
  fail "app is not signed with a Developer ID Application certificate"

codesign --verify --deep --strict --verbose=2 "$APP_PATH"

printf '\nRelease signature verified. Accessibility permission will remain valid across updates '\
  'that keep this Team ID and bundle identifier.\n'
