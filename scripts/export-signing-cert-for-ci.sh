#!/usr/bin/env bash
# 把「ClipMaster Pro Code Signing」证书导出成 GitHub Actions Secret。
# 优先读取 create-stable-signing-cert.sh 留下的本地备份；没有备份则提示手动导出。
#
# 用法：
#   bash scripts/export-signing-cert-for-ci.sh
#   bash scripts/export-signing-cert-for-ci.sh --set-github-secrets

set -euo pipefail

readonly CERT_NAME="${CLIPMASTER_SIGNING_CERT_NAME:-ClipMaster Pro Code Signing}"
readonly BACKUP_DIR="${CLIPMASTER_CERT_BACKUP_DIR:-$HOME/.clipmaster-signing}"
readonly P12_FILE="$BACKUP_DIR/ClipMasterPro-CodeSigning.p12"
readonly PW_FILE="$BACKUP_DIR/ClipMasterPro-CodeSigning.password"
readonly SET_GH="${1:-}"

fail() {
  printf 'export-signing-cert aborted: %s\n' "$*" >&2
  exit 1
}

[[ -f "$P12_FILE" && -f "$PW_FILE" ]] || fail \
  "missing $P12_FILE (or .password).
Re-run: bash scripts/create-stable-signing-cert.sh
If the cert already exists in Keychain, delete it first, then recreate so a CI backup is written.
Or manually: Keychain Access → select cert → File → Export Items… → .p12"

P12_PW="$(tr -d '\n' < "$PW_FILE")"
B64="$(base64 < "$P12_FILE" | tr -d '\n')"

# 校验 p12 能解开且 CN 对得上（OpenSSL 3 读 -legacy 导出的 p12 需要同样加 -legacy）
SUBJECT="$(openssl pkcs12 -in "$P12_FILE" -passin pass:"$P12_PW" -nokeys -legacy 2>/dev/null \
  | openssl x509 -noout -subject 2>/dev/null \
  || openssl pkcs12 -in "$P12_FILE" -passin pass:"$P12_PW" -nokeys 2>/dev/null \
  | openssl x509 -noout -subject 2>/dev/null \
  || true)"
printf '%s\n' "$SUBJECT" | grep -qF "$CERT_NAME" || \
  fail "p12 backup does not contain CN=$CERT_NAME (got: ${SUBJECT:-<empty>})"


if [[ "$SET_GH" == "--set-github-secrets" ]]; then
  command -v gh >/dev/null || fail "gh CLI not found"
  printf '%s' "$B64" | gh secret set MACOS_CODESIGN_P12
  printf '%s' "$P12_PW" | gh secret set MACOS_CODESIGN_P12_PASSWORD
  printf '%s' "$CERT_NAME" | gh secret set MACOS_CODESIGN_IDENTITY
  printf 'GitHub secrets updated: MACOS_CODESIGN_P12 / MACOS_CODESIGN_P12_PASSWORD / MACOS_CODESIGN_IDENTITY\n'
  printf 'You can now delete: %s\n' "$BACKUP_DIR"
  exit 0
fi

printf '\n=== Add these GitHub Actions secrets ===\n\n'
printf 'MACOS_CODESIGN_IDENTITY=%s\n\n' "$CERT_NAME"
printf 'MACOS_CODESIGN_P12_PASSWORD=%s\n\n' "$P12_PW"
printf 'MACOS_CODESIGN_P12 (base64, one line):\n%s\n\n' "$B64"
printf 'Or run: bash scripts/export-signing-cert-for-ci.sh --set-github-secrets\n'
