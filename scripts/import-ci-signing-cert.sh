#!/usr/bin/env bash
# GitHub Actions / 本机 CI：导入 PKCS#12 后设置 APPLE_SIGNING_IDENTITY。
# 供 .github/workflows/release.yml 在 tauri build 之前调用。

set -euo pipefail
umask 077

fail() {
  printf 'import-ci-signing-cert aborted: %s\n' "$*" >&2
  exit 1
}

[[ -n "${MACOS_CODESIGN_P12:-}" ]] || fail "MACOS_CODESIGN_P12 secret is empty"
[[ -n "${MACOS_CODESIGN_P12_PASSWORD:-}" ]] || fail "MACOS_CODESIGN_P12_PASSWORD secret is empty"

IDENTITY="${MACOS_CODESIGN_IDENTITY:-ClipMaster Pro Code Signing}"
KEYCHAIN="${RUNNER_TEMP:-/tmp}/clipmaster-signing.keychain-db"
KEYCHAIN_PW="$(openssl rand -hex 16)"
P12_PATH="${RUNNER_TEMP:-/tmp}/clipmaster-signing.p12"
trap 'test ! -f "$P12_PATH" || unlink "$P12_PATH"' EXIT

printf '%s' "$MACOS_CODESIGN_P12" | base64 --decode > "$P12_PATH"

security create-keychain -p "$KEYCHAIN_PW" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PW" "$KEYCHAIN"
security import "$P12_PATH" -k "$KEYCHAIN" -P "$MACOS_CODESIGN_P12_PASSWORD" \
  -T /usr/bin/codesign -T /usr/bin/security
EXISTING_KEYCHAINS=()
while IFS= read -r listed_keychain; do
  listed_keychain="${listed_keychain#\"}"
  listed_keychain="${listed_keychain%\"}"
  EXISTING_KEYCHAINS+=("$listed_keychain")
done < <(security list-keychains -d user)
security list-keychains -d user -s "$KEYCHAIN" "${EXISTING_KEYCHAINS[@]}"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PW" "$KEYCHAIN"

if ! security find-identity -p codesigning "$KEYCHAIN" | grep -qF "$IDENTITY"; then
  fail "imported keychain does not contain identity: $IDENTITY"
fi

# 写到 GITHUB_ENV，后续步骤的 tauri build 会读到 APPLE_SIGNING_IDENTITY。
if [[ -n "${GITHUB_ENV:-}" ]]; then
  {
    echo "APPLE_SIGNING_IDENTITY=$IDENTITY"
  } >> "$GITHUB_ENV"
fi

export APPLE_SIGNING_IDENTITY="$IDENTITY"
printf 'Imported signing identity for CI: %s\n' "$IDENTITY"
