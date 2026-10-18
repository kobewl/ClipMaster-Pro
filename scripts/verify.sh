#!/usr/bin/env bash
# ClipMaster Pro 本地 / 发版统一检查（fail-fast，任何一步失败立即退出）。
#
# 依次运行：
#   1. python3 scripts/check-secrets.py              受追踪文件中的凭据扫描
#   2. （仅 --release）scripts/check-release-version.py 校验 tag 与各处版本号一致，
#      scripts/extract-changelog.py 校验 CHANGELOG.md 有本版本的一节
#   3. cargo fmt --check                             （src-tauri）
#   4. cargo clippy --all-targets -- -D warnings     （src-tauri）
#   5. cargo test                                    （src-tauri）
#   6. npx vitest run --passWithNoTests              （测试文件只在本地，发版 runner 上没有）
#   7. npm run build                                 类型检查 + 前端构建
#
# 用法：
#   bash scripts/verify.sh                      日常检查
#   bash scripts/verify.sh --release vX.Y.Z     发版前检查（额外跑第 2 步）
#
# release.yml 在构建前调用 `bash scripts/verify.sh --release "$TAG"`，
# 本地与发版的检查是同一份脚本，不会各自漂移。
#
# 不会自动安装为 git hook。需要时在仓库根目录执行：
#   ln -s ../../scripts/verify.sh .git/hooks/pre-push
# 以 pre-push 身份运行时，推送的每个 vX.Y.Z tag 都会自动按 --release 检查。
# 临时跳过：git push --no-verify
set -euo pipefail

ROOT="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/.." && pwd)"
cd "$ROOT"

usage() {
  echo "用法: $(basename "$0") [--release vX.Y.Z]" >&2
}

release_tags=()

if [[ "$(basename "$0")" == "pre-push" ]]; then
  # git 以 `pre-push <remote> <url>` 调用，并在 stdin 给出
  # `<local_ref> <local_sha> <remote_ref> <remote_sha>`，每行一个 ref。
  zero_sha="0000000000000000000000000000000000000000"
  while read -r local_ref local_sha _remote_ref _remote_sha; do
    [[ -z "${local_ref:-}" ]] && continue
    [[ "${local_sha}" == "${zero_sha}" ]] && continue
    if [[ "${local_ref}" =~ ^refs/tags/(v[0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
      release_tags+=("${BASH_REMATCH[1]}")
    fi
  done
else
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --release)
        if [[ $# -lt 2 || -z "$2" ]]; then
          usage
          exit 2
        fi
        release_tags+=("$2")
        shift 2
        ;;
      -h | --help)
        usage
        exit 0
        ;;
      *)
        usage
        exit 2
        ;;
    esac
  done
fi

step() {
  echo
  echo "==> $*"
}

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

step "凭据扫描"
python3 scripts/check-secrets.py

for tag in "${release_tags[@]+"${release_tags[@]}"}"; do
  step "版本号一致性 ${tag}"
  python3 scripts/check-release-version.py "$tag"
  step "CHANGELOG 章节 ${tag}"
  python3 scripts/extract-changelog.py "$tag" --output "$tmp"
done

step "cargo fmt --check"
(cd src-tauri && cargo fmt --check)

step "cargo clippy --all-targets -- -D warnings"
(cd src-tauri && cargo clippy --all-targets -- -D warnings)

step "cargo test"
(cd src-tauri && cargo test)

step "vitest"
npx vitest run --passWithNoTests

step "npm run build"
npm run build

echo
echo "✓ verify passed"
