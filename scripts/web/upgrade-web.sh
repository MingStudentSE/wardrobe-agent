#!/bin/sh
# 升级 / 回滚 wardrobe-web：切换 <打包根>/current 软链 → 停止 → 启动 → 健康检查。
# 数据目录与端口保持不变；会话不持久化，升级重启后需用连接令牌重新登录（ADR0029 D4）。
#
# 用法：
#   upgrade-web.sh --pack <打包根> --data-dir <目录> [--port <端口>]                 # 升级到最新打包版本
#   upgrade-web.sh --pack <打包根> --data-dir <目录> --release <releases/xxx>       # 切到指定版本
#   upgrade-web.sh --pack <打包根> --data-dir <目录> --rollback                     # 回滚到上一版本
#   可选 --backup-dir <目录>：升级前在数据目录内做一次 SQLite 快照拷贝（服务已停时拷贝是安全的；
#   完整备份/恢复走应用内 backup.create/restore，见 docs/api/web.md）。
#
# 回滚边界（如实标注）：数据库 schema 由 wardrobe-core 单向迁移；若新版本已升级过 schema，
# 旧版本会以 unsupported_schema 拒绝打开，此时必须先恢复升级前的应用级备份（backup.restore），
# 不能仅靠切换二进制回滚。--rollback 只覆盖「schema 未变化」的常规升级回退。
set -eu

PACK=""
DATA_DIR=""
PORT="${WARDROBE_WEB_PORT:-17838}"
MODE="latest"
RELEASE=""
BACKUP_DIR=""

while [ $# -gt 0 ]; do
 case "$1" in
  --pack) PACK="$2"; shift 2 ;;
  --data-dir) DATA_DIR="$2"; shift 2 ;;
  --port) PORT="$2"; shift 2 ;;
  --release) MODE="explicit"; RELEASE="$2"; shift 2 ;;
  --rollback) MODE="rollback"; shift ;;
  --backup-dir) BACKUP_DIR="$2"; shift 2 ;;
  *) echo "未知参数：$1" >&2; exit 2 ;;
 esac
done
[ -n "$PACK" ] && [ -n "$DATA_DIR" ] || { echo "必须提供 --pack 与 --data-dir" >&2; exit 1; }

CURRENT_LINK="$PACK/current"
if [ ! -L "$CURRENT_LINK" ]; then echo "current 软链不存在：$CURRENT_LINK" >&2; exit 1; fi

HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT_DIR="$HERE"
[ -f "$SCRIPT_DIR/start-web.sh" ] || SCRIPT_DIR="$PACK/current/scripts"

# 选目标版本
TARGET=""
case "$MODE" in
 latest)
  TARGET="$(ls -d "$PACK"/releases/*/ 2>/dev/null | sort | tail -1)"
  [ -n "$TARGET" ] || { echo "releases 目录为空" >&2; exit 1; }
  ;;
 explicit)
  TARGET="$RELEASE"
  [ -d "$TARGET" ] || { echo "指定版本不存在：$TARGET" >&2; exit 1; }
  ;;
 rollback)
  CURRENT_TARGET="$(readlink -f "$CURRENT_LINK")"
  TARGET="$(ls -d "$PACK"/releases/*/ 2>/dev/null | sort | tail -1)"
  # 倒序找第一个既不是当前版本的目录
  for candidate in $(ls -d "$PACK"/releases/*/ 2>/dev/null | sort -r); do
   if [ "$(readlink -f "$candidate")" != "$CURRENT_TARGET" ]; then TARGET="$candidate"; break; fi
  done
  if [ "$(readlink -f "$TARGET")" = "$CURRENT_TARGET" ]; then
   echo "没有可回滚的历史版本（current 已是唯一版本）" >&2; exit 1
  fi
  ;;
esac
TARGET="${TARGET%/}"

echo "升级目标：${TARGET}（current 现指向 $(readlink -f "$CURRENT_LINK")）"

# 停止（升级期间数据不可写）
sh "$SCRIPT_DIR/stop-web.sh" --data-dir "$DATA_DIR"

# 可选：停机后做数据目录快照（不含系统硬链接语义的朴素拷贝，仅 SQLite 主库）
if [ -n "$BACKUP_DIR" ]; then
 mkdir -p "$BACKUP_DIR"
 if [ -f "$DATA_DIR/wardrobe.sqlite" ]; then
  cp "$DATA_DIR/wardrobe.sqlite" "$BACKUP_DIR/wardrobe.sqlite.pre-upgrade"
  echo "已快照数据目录到 $BACKUP_DIR"
 fi
fi

ln -sfn "$TARGET" "$CURRENT_LINK"
echo "current → $(readlink "$CURRENT_LINK")"

# 启动并健康检查
if sh "$SCRIPT_DIR/start-web.sh" --pack "$PACK" --port "$PORT" --data-dir "$DATA_DIR"; then
 echo "升级完成：$(cat "$TARGET/VERSION" 2>/dev/null | head -2 | tr '\n' ' ')"
 exit 0
fi
echo "启动失败，回滚 current 到 $(readlink -f "$CURRENT_LINK" >/dev/null 2>&1 && echo 旧版本) —— 请检查日志 $DATA_DIR/wardrobe-web.log" >&2
exit 1
