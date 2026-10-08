#!/bin/sh
# ticket-39-final 部署演练（本机实际执行，全部产物在 1TB 备份盘）：
#   打包 v1 → 部署启动 → 浏览器引导+录入（真实 Chrome）→ CLI 同库读回 → 停止
#   → 打包 v2 → 升级重启 → 浏览器重登驗證数据完好 → 回滚 v1 → 再次验证 → 升级回 v2 → 停止
# v1 用 debug 产物代表"旧版本"，v2 用 release 产物代表"新版本"（二进制确有变化）。
set -o pipefail
set -e
WT=/Users/links/.codex/wardrobe-execution/worktrees/ticket-39-final
CACHE=/Users/links/.codex/wardrobe-execution/cache/ticket39-final
RES=/Users/links/.codex/external-workspace-volume/codex/wardrobe-execution/results/continue-20261007/ticket39-final
DEPLOY="$RES/deploy"
PACK="$DEPLOY/pack"
DATA="$DEPLOY/data"
PORT=5392
PHOTO="$WT/design/studio/assets/jacket.png"

step(){ echo; echo "===== [$1] ====="; }

step "0 环境与端口预检"
. /Users/links/.codex/wardrobe-execution/native-env.sh
mkdir -p "$DEPLOY"
if curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then echo "端口 $PORT 已被占用，中止"; exit 1; fi
echo "端口 $PORT 空闲"

step "1 打包 v1（debug 二进制代表旧版本）"
sh "$WT/scripts/web/pack-web-release.sh" --worktree "$WT" --out "$PACK" \
  --bin "$CACHE/debug/wardrobe-web" --dist "$WT/apps/desktop/dist" \
  --cli "$CACHE/release/wardrobe-cli" || exit 1

step "2 部署启动（127.0.0.1:${PORT}，数据目录 ${DATA}）"
sh "$WT/scripts/web/start-web.sh" --pack "$PACK" --port $PORT --data-dir "$DATA" || exit 1
curl -fsS "http://127.0.0.1:$PORT/health"; echo
echo "引导令牌文件权限：$(ls -l "$DATA/web-bootstrap-token" | awk '{print $1}')"

step "3 浏览器跑通核心流程（真实 Chrome：引导→令牌→录入→重载恢复）"
mkdir -p "$DEPLOY/bin"
ln -sfn "$WT/apps/desktop/node_modules" "$DEPLOY/bin/node_modules"
cp "$WT/scripts/web/deploy-smoke.mjs" "$DEPLOY/bin/deploy-smoke.mjs"
cd "$DEPLOY/bin" && node deploy-smoke.mjs bootstrap --url "http://127.0.0.1:$PORT" --data-dir "$DATA" --photo "$PHOTO" --name "部署演练夹克" || exit 1

step "4 CLI 同库读回（Web 会话写入 = CLI 读取一致）"
# 直连 CLI 同样需要 bundle 内原生媒体库（libheif/libde265）
export DYLD_LIBRARY_PATH="$PACK/current/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
cli_trusted(){ env WARDROBE_DATA_DIR="$DATA" "$PACK/current/bin/wardrobe-cli" --trusted-local "$1" "$2"; }
SPACE=$(cli_trusted space.default '{}')
MEMBER="$(printf '%s' "$SPACE" | sed -n 's/.*"selected_member_id": *"\([^"]*\)".*/\1/p')"
[ -n "$MEMBER" ] || MEMBER="$(printf '%s' "$SPACE" | sed -n 's/.*"member_id": *"\([^"]*\)".*/\1/p')"
[ -n "$MEMBER" ] || { echo "无法确定默认成员"; exit 1; }
ITEMS=$(cli_trusted clothing.list "{\"member_id\":\"$MEMBER\",\"query\":\"部署演练夹克\"}")
echo "$ITEMS" | head -c 400; echo
echo "$ITEMS" | grep -q '部署演练夹克' || { echo "CLI 读回失败"; exit 1; }
echo "CLI 读回一致"

step "5 停止服务（用后即停；会话随进程消失）"
sh "$WT/scripts/web/stop-web.sh" --data-dir "$DATA" || exit 1
if curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then echo "停止后健康检查仍通过，异常"; exit 1; fi
echo "停止确认：/health 不可达"

step "6 打包 v2（release 二进制代表新版本）并升级重启"
sh "$WT/scripts/web/pack-web-release.sh" --worktree "$WT" --out "$PACK" \
  --bin "$CACHE/release/wardrobe-web" --dist "$WT/apps/desktop/dist" \
  --cli "$CACHE/release/wardrobe-cli" || exit 1
sh "$WT/scripts/web/upgrade-web.sh" --pack "$PACK" --data-dir "$DATA" --port $PORT || exit 1
echo "current → $(readlink "$PACK/current")"
export DYLD_LIBRARY_PATH="$PACK/current/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
cd "$DEPLOY/bin" && node deploy-smoke.mjs resume --url "http://127.0.0.1:$PORT" --data-dir "$DATA" --name "部署演练夹克" || exit 1
cli_trusted clothing.list "{\"member_id\":\"$MEMBER\",\"query\":\"部署演练夹克\"}" | grep -q '部署演练夹克' || { echo "升级后数据丢失"; exit 1; }
echo "升级后数据完好"

step "7 已并入第 6 步后的浏览器重登与 CLI 读回"

step "8 回滚到 v1（切换软链 → 重启）"
sh "$WT/scripts/web/upgrade-web.sh" --pack "$PACK" --data-dir "$DATA" --port $PORT --rollback || exit 1
echo "current → $(readlink "$PACK/current")"
cd "$DEPLOY/bin" && node deploy-smoke.mjs resume --url "http://127.0.0.1:$PORT" --data-dir "$DATA" --name "部署演练夹克" || exit 1

step "9 重新升级到 v2 并停止服务（演练结束，不留常驻进程）"
sh "$WT/scripts/web/upgrade-web.sh" --pack "$PACK" --data-dir "$DATA" --port $PORT || exit 1
sh "$WT/scripts/web/stop-web.sh" --data-dir "$DATA" || exit 1
echo
echo "===== 部署演练全部通过 ====="
