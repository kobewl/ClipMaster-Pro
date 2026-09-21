#!/usr/bin/env bash
# 在登录钥匙串里创建一张「Code Signing」自签名证书，给 macOS TCC 一个稳定身份。
#
# 背景（Java 类比）：
#   TCC 辅助功能授权 ≈ 按「代码身份」存的 ACL，不是按应用名字。
#   ad-hoc 签名的 Designated Requirement 绑的是 CDHash（≈ jar 的 SHA-256），
#   每次重新编译/发版二进制一变，ACL 就对不上 —— 设置里开关还亮着，但
#   AXIsProcessTrusted() 返回 false，只能删掉重新加。
#   用同一张证书签名后，DR 绑的是「bundle id + 证书指纹」，更新后仍然匹配。
#
# 幂等、不需要 sudo。产物仅适合个人/内测分发（未公证，Gatekeeper 仍会提示一次）。
# 正式对外分发请改用 Apple Developer ID，见 docs/release/RELEASE.md。

set -euo pipefail

readonly CERT_NAME="${CLIPMASTER_SIGNING_CERT_NAME:-ClipMaster Pro Code Signing}"
readonly KEYCHAIN="${CLIPMASTER_KEYCHAIN:-$HOME/Library/Keychains/login.keychain-db}"

if security find-identity -p codesigning "$KEYCHAIN" 2>/dev/null | grep -qF "$CERT_NAME"; then
  printf 'Code-signing identity already present: %s\n' "$CERT_NAME"
  security find-identity -p codesigning "$KEYCHAIN" | grep -F "$CERT_NAME" || true
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/openssl.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $CERT_NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" \
  -days 3650 -config "$TMP/openssl.cnf" 2>/dev/null

# OpenSSL 3 默认 PKCS#12 算法会被 macOS Security 框架拒绝；优先 -legacy。
# security import 不接受空密码的 p12。
P12_PW="$(openssl rand -hex 16)"
if ! openssl pkcs12 -export -legacy \
  -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -name "$CERT_NAME" -out "$TMP/cert.p12" -passout pass:"$P12_PW" 2>/dev/null; then
  openssl pkcs12 -export \
    -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
    -name "$CERT_NAME" -out "$TMP/cert.p12" -passout pass:"$P12_PW"
fi

security import "$TMP/cert.p12" -k "$KEYCHAIN" -P "$P12_PW" -A -T /usr/bin/codesign

# 尽量让 codesign 以后不再弹「允许使用私钥」。需要登录钥匙串密码时可通过
# KEYCHAIN_PASSWORD 传入；未设置则首次 codesign 时点「始终允许」即可。
if [[ -n "${KEYCHAIN_PASSWORD:-}" ]]; then
  security set-key-partition-list -S apple-tool:,apple:,codesign: \
    -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null 2>&1 || true
fi

# 备份 p12：CI 必须用「同一张」证书，否则下次发版 TCC 又断。
# 上传 GitHub Secret 后请删除本地备份。
BACKUP_DIR="${CLIPMASTER_CERT_BACKUP_DIR:-$HOME/.clipmaster-signing}"
mkdir -p "$BACKUP_DIR"
chmod 700 "$BACKUP_DIR"
cp "$TMP/cert.p12" "$BACKUP_DIR/ClipMasterPro-CodeSigning.p12"
printf '%s\n' "$P12_PW" > "$BACKUP_DIR/ClipMasterPro-CodeSigning.password"
chmod 600 "$BACKUP_DIR/ClipMasterPro-CodeSigning.p12" "$BACKUP_DIR/ClipMasterPro-CodeSigning.password"

printf 'Created self-signed code-signing identity: %s\n' "$CERT_NAME"
security find-identity -p codesigning "$KEYCHAIN" | grep -F "$CERT_NAME" || true
printf '\nCI backup (upload once, then delete):\n  %s\n  %s\n' \
  "$BACKUP_DIR/ClipMasterPro-CodeSigning.p12" \
  "$BACKUP_DIR/ClipMasterPro-CodeSigning.password"
printf '\nNext:\n'
printf '  1. bash scripts/sign-macos-app.sh "/Applications/ClipMaster Pro.app"\n'
printf '  2. 系统设置 → 辅助功能：删掉 ClipMaster Pro，再重新添加并打开\n'
printf '  3. 完全退出并重启应用（授权后进程要重启才生效）\n'
printf '  4. bash scripts/export-signing-cert-for-ci.sh --set-github-secrets\n'
