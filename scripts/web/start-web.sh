#!/bin/sh
# 启动 wardrobe-web（ADR0029）：只绑定 127.0.0.1；数据目录、端口、界面目录全部参数化。
# 用法：start-web.sh [--pack <打包根>] [--port <端口>] [--data-dir <目录>] [--dist <目录>] [--bin <二进制>]
#   --pack     打包根（含 current 软链），默认 $WARDROBE_WEB_PACK
#   --port     监听端口，默认 17838（$WARDROBE_WEB_PORT 可覆盖）
#   --data-dir 数据目录，默认 $WARDROBE_WEB_DATA；未提供则拒绝启动（避免误用系统盘默认目录）
# （注释里的变量引用仅作文档；运行路径均已使用 ASCII 边界或 ${VAR} 形式）
#   --dist     界面目录，默认 <current>/web-dist
#   --bin      直接指定二进制（优先级最高；升级演练/本地调试用）
# 启动成功后打印访问地址；首启引导令牌位于 <data-dir>/web-bootstrap-token（0600，仅限本机操作员）。
set -eu

PACK="${WARDROBE_WEB_PACK:-}"
PORT="${WARDROBE_WEB_PORT:-17838}"
DATA_DIR="${WARDROBE_WEB_DATA:-}"
DIST=""
BIN=""

while [ $# -gt 0 ]; do
 case "$1" in
  --pack) PACK="$2"; shift 2 ;;
  --port) PORT="$2"; shift 2 ;;
  --data-dir) DATA_DIR="$2"; shift 2 ;;
  --dist) DIST="$2"; shift 2 ;;
  --bin) BIN="$2"; shift 2 ;;
  *) echo "未知参数：$1" >&2; exit 2 ;;
 esac
done

CURRENT=""
if [ -n "$PACK" ]; then
 CURRENT="$(readlink -f "$PACK/current" 2>/dev/null || echo "$PACK/current")"
 [ -d "$CURRENT" ] || { echo "打包根缺少 current：$PACK/current" >&2; exit 1; }
fi
if [ -z "$BIN" ]; then
 [ -n "$CURRENT" ] || { echo "必须提供 --pack 或 --bin" >&2; exit 1; }
 BIN="$CURRENT/bin/wardrobe-web"
fi
[ -x "$BIN" ] || { echo "二进制不可执行：$BIN" >&2; exit 1; }
[ -n "$DATA_DIR" ] || { echo "必须显式提供 --data-dir（或 WARDROBE_WEB_DATA）：本服务不允许静默落到系统盘默认目录" >&2; exit 1; }
if [ -z "$DIST" ]; then
 if [ -n "$CURRENT" ] && [ -d "$CURRENT/web-dist" ]; then DIST="$CURRENT/web-dist";
 elif [ -n "$CURRENT" ]; then DIST="$CURRENT/web-dist"; fi
fi

PID_FILE="$DATA_DIR/wardrobe-web.pid"
LOG_FILE="$DATA_DIR/wardrobe-web.log"
mkdir -p "$DATA_DIR"

if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
 echo "wardrobe-web 已在运行（pid $(cat "$PID_FILE")）；如需重启请先 stop-web.sh" >&2
 exit 1
fi

# macOS 原生媒体库：优先用 bundle 内 lib/，其次环境既有路径（构建机本地运行）
DYLD="/Users/links/.codex/wardrobe-execution/native-media/install/lib"
if [ -n "$CURRENT" ] && [ -d "$CURRENT/lib" ]; then DYLD="$CURRENT/lib"; fi

nohup env DYLD_LIBRARY_PATH="$DYLD${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" \
 "$BIN" --port "$PORT" --data-dir "$DATA_DIR" ${DIST:+--web-dist "$DIST"} \
 >> "$LOG_FILE" 2>&1 &
echo $! > "$PID_FILE"

# 等待健康检查（最多 15 秒）
i=0
while [ $i -lt 30 ]; do
 if curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then
  echo "wardrobe-web 已启动：http://127.0.0.1:$PORT/  (pid $(cat "$PID_FILE")，数据目录 $DATA_DIR)"
  if [ -f "$DATA_DIR/web-bootstrap-token" ]; then
   echo "首次使用：引导令牌在 $DATA_DIR/web-bootstrap-token（0600；在网页首启页粘贴一次，用后作废）"
  fi
  exit 0
 fi
 if ! kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
  echo "wardrobe-web 启动失败，日志尾部：" >&2
  tail -5 "$LOG_FILE" >&2
  rm -f "$PID_FILE"
  exit 1
 fi
 i=$((i+1)); sleep 0.5
done
echo "健康检查超时，日志尾部：" >&2; tail -5 "$LOG_FILE" >&2
exit 1
