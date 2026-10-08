#!/bin/sh
# 打包 wardrobe-web 部署形态（ADR0029 D8）：release 二进制 + 前端 dist + 运行脚本，
# 产出可复现的版本化目录并切换 current 软链。一切产物落 1TB 备份盘。
#
# 用法：
#   pack-web-release.sh --worktree <worktree根> --out <打包根> \
#     [--bin <wardrobe-web二进制>] [--dist <前端dist目录>] [--cli <wardrobe-cli二进制>]
#
# 产出布局：
#   <out>/releases/v<版本>-<打包时间>/
#     bin/wardrobe-web        服务二进制
#     bin/wardrobe-cli        （可选）同库 CLI，便于同一数据目录运维
#     lib/                    macOS 原生媒体动态库（libheif/libde265；启动脚本据此设 DYLD_LIBRARY_PATH）
#     web-dist/               React 构建产物
#     scripts/                start-web.sh / stop-web.sh / upgrade-web.sh
#     VERSION                 版本与来源说明
#   <out>/current -> releases/v<...>   （升级与回滚都只动这个软链）
set -eu

usage_and_exit() { echo "$1" >&2; exit 2; }

WORKTREE=""
OUT=""
BIN=""
DIST=""
CLI=""

while [ $# -gt 0 ]; do
 case "$1" in
  --worktree) WORKTREE="$2"; shift 2 ;;
  --out) OUT="$2"; shift 2 ;;
  --bin) BIN="$2"; shift 2 ;;
  --dist) DIST="$2"; shift 2 ;;
  --cli) CLI="$2"; shift 2 ;;
  *) usage_and_exit "未知参数：$1" ;;
 esac
done

[ -n "$WORKTREE" ] || usage_and_exit "必须提供 --worktree"
[ -d "$WORKTREE" ] || usage_and_exit "worktree 不存在：$WORKTREE"
[ -n "$OUT" ] || usage_and_exit "必须提供 --out"
case "$OUT" in /Users/links/.codex/external-workspace-volume/*|/Users/links/.codex/wardrobe-execution/*) ;; *) usage_and_exit "打包根必须在 1TB 备份盘（docs/agents/build-storage.md）" ;; esac

# 默认：本 worktree target/debug 下的 wrapper 指向缓存 release 产物；dist 为 apps/desktop/dist
if [ -z "$BIN" ]; then
 BIN="$WORKTREE/target/debug/wardrobe-web"
fi
[ -x "$BIN" ] || usage_and_exit "二进制不可执行：$BIN"
if [ -z "$DIST" ]; then DIST="$WORKTREE/apps/desktop/dist"; fi
[ -f "$DIST/index.html" ] || usage_and_exit "dist 缺少 index.html（先在 apps/desktop 运行 npm run build）：$DIST"
if [ -z "$CLI" ] && [ -x "$WORKTREE/target/debug/wardrobe-cli" ]; then CLI="$WORKTREE/target/debug/wardrobe-cli"; fi

VERSION="$("$BIN" --version | sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' | head -1)"
[ -n "$VERSION" ] || VERSION="0.0.0"
STAMP="$(date +%Y%m%d-%H%M%S)"
RELEASE_DIR="$OUT/releases/v$VERSION-$STAMP"

mkdir -p "$RELEASE_DIR/bin" "$RELEASE_DIR/lib" "$RELEASE_DIR/scripts"
cp "$BIN" "$RELEASE_DIR/bin/wardrobe-web"
if [ -n "$CLI" ]; then cp "$CLI" "$RELEASE_DIR/bin/wardrobe-cli"; fi
cp -R "$DIST" "$RELEASE_DIR/web-dist"

# macOS 原生媒体库（HEIC 解码）。非 macOS 或已静态链接时允许缺省。
NATIVE_LIB="/Users/links/.codex/wardrobe-execution/native-media/install/lib"
if [ -d "$NATIVE_LIB" ]; then
 cp "$NATIVE_LIB"/libheif*.dylib "$NATIVE_LIB"/libde265*.dylib "$RELEASE_DIR/lib/" 2>/dev/null || true
fi

cp "$WORKTREE/scripts/web/start-web.sh" "$WORKTREE/scripts/web/stop-web.sh" "$WORKTREE/scripts/web/upgrade-web.sh" "$WORKTREE/scripts/web/pack-web-release.sh" "$WORKTREE/scripts/web/deploy-smoke.mjs" "$WORKTREE/scripts/web/rehearse-deploy.sh" "$RELEASE_DIR/scripts/" 2>/dev/null || \
cp "$WORKTREE/scripts/web/start-web.sh" "$WORKTREE/scripts/web/stop-web.sh" "$WORKTREE/scripts/web/upgrade-web.sh" "$RELEASE_DIR/scripts/"
chmod +x "$RELEASE_DIR/scripts/"*.sh 2>/dev/null || true

{
 echo "name: wardrobe-web"
 echo "version: $VERSION"
 echo "packed_at: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
 echo "source_commit: $(git -C "$WORKTREE" rev-parse HEAD 2>/dev/null || echo unknown)"
 echo "bin_sha256: $(shasum -a 256 "$RELEASE_DIR/bin/wardrobe-web" | awk '{print $1}')"
 echo "layout: bin/ web-dist/ lib/ scripts/；升级与回滚仅切换 <out>/current 软链"
} > "$RELEASE_DIR/VERSION"

ln -sfn "$RELEASE_DIR" "$OUT/current"
echo "已打包并切换 current：$RELEASE_DIR"
cat "$RELEASE_DIR/VERSION"
