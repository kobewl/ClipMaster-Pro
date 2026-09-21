#!/usr/bin/env bash
# 用稳定签名身份重签 .app，使辅助功能授权在更新后仍然有效。
#
# 优先顺序：
#   1. 环境变量 APPLE_SIGNING_IDENTITY（Developer ID 或自签名 CN）
#   2. 钥匙串里的「ClipMaster Pro Code Signing」
#
# 用法：
#   bash scripts/sign-macos-app.sh "/Applications/ClipMaster Pro.app"
#   bash scripts/sign-macos-app.sh src-tauri/target/release/bundle/macos/"ClipMaster Pro.app"

set -euo pipefail

readonly EXPECTED_BUNDLE_ID="pro.clipmaster.desktop"
readonly DEFAULT_CERT_NAME="${CLIPMASTER_SIGNING_CERT_NAME:-ClipMaster Pro Code Signing}"
readonly KEYCHAIN="${CLIPMASTER_KEYCHAIN:-$HOME/Library/Keychains/login.keychain-db}"

fail() {
  printf 'sign-macos-app aborted: %s\n' "$*" >&2
  exit 1
}

[[ "$(uname -s)" == "Darwin" ]] || fail "macOS is required"
[[ $# -eq 1 ]] || fail "usage: $0 /path/to/App.app"

APP_PATH="$1"
[[ -d "$APP_PATH" ]] || fail "app bundle not found: $APP_PATH"

BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP_PATH/Contents/Info.plist" 2>/dev/null || true)"
[[ "$BUNDLE_ID" == "$EXPECTED_BUNDLE_ID" ]] || \
  fail "unexpected CFBundleIdentifier='$BUNDLE_ID' (expected $EXPECTED_BUNDLE_ID)"

resolve_identity() {
  if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    printf '%s\n' "$APPLE_SIGNING_IDENTITY"
    return
  fi
  local line
  line="$(security find-identity -p codesigning "$KEYCHAIN" 2>/dev/null | grep -F "$DEFAULT_CERT_NAME" | head -n1 || true)"
  [[ -n "$line" ]] || return 1
  # 行格式：  1) ABCDEF... "ClipMaster Pro Code Signing"
  printf '%s\n' "$line" | sed -n 's/.*"\(.*\)".*/\1/p'
}

IDENTITY="$(resolve_identity)" || fail \
  "no signing identity. Run: bash scripts/create-stable-signing-cert.sh
Or export APPLE_SIGNING_IDENTITY='Developer ID Application: … (TEAMID)'"

printf 'Signing with identity: %s\n' "$IDENTITY"
printf 'Target: %s\n' "$APP_PATH"

# 去掉隔离属性，避免 Gatekeeper 把「覆盖安装」当成全新下载物。
xattr -cr "$APP_PATH" 2>/dev/null || true

# 显式指定 identifier，避免继承 linker-signed 的随机 Identifier（clipmaster_pro-xxxx）。
codesign --force --deep --sign "$IDENTITY" \
  --identifier "$EXPECTED_BUNDLE_ID" \
  --options runtime \
  "$APP_PATH"

codesign --verify --deep --strict --verbose=2 "$APP_PATH"

DR="$(codesign -d -r- "$APP_PATH" 2>&1)"
printf '\nDesignated Requirement:\n%s\n' "$DR"

printf '%s\n' "$DR" | grep -Eq 'certificate leaf|Authority=|anchor apple' || \
  printf '%s\n' "$DR" | grep -Fq "identifier \"$EXPECTED_BUNDLE_ID\"" || \
  fail "signature still looks cdhash-pinned; Accessibility will break on the next update"

if printf '%s\n' "$DR" | grep -Fq 'cdhash H"' && ! printf '%s\n' "$DR" | grep -Fq 'certificate leaf'; then
  fail "Designated Requirement is still only a cdhash — signing did not produce a stable identity"
fi

printf '\nOK. First time after switching identity: remove ClipMaster Pro from\n'
printf 'System Settings → Privacy & Security → Accessibility, add it back, then relaunch.\n'
printf 'Later updates signed with the SAME certificate will keep the grant.\n'
