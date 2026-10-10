#!/bin/sh
# 停止 wardrobe-web：只按本服务的数据目录 pid 文件停止，不使用全局 pkill。
# 用法：stop-web.sh --data-dir <目录>（或 WARDROBE_WEB_DATA）
set -eu

DATA_DIR="${WARDROBE_WEB_DATA:-}"
while [ $# -gt 0 ]; do
 case "$1" in
  --data-dir) DATA_DIR="$2"; shift 2 ;;
  *) echo "未知参数：$1" >&2; exit 2 ;;
 esac
done
[ -n "$DATA_DIR" ] || { echo "必须提供 --data-dir（或 WARDROBE_WEB_DATA）" >&2; exit 1; }

PID_FILE="$DATA_DIR/wardrobe-web.pid"
if [ ! -f "$PID_FILE" ]; then
 echo "没有运行记录（$PID_FILE 不存在），无需停止"
 exit 0
fi
PID="$(cat "$PID_FILE")"
if kill -0 "$PID" 2>/dev/null; then
 kill "$PID"
 i=0
 while kill -0 "$PID" 2>/dev/null && [ $i -lt 20 ]; do i=$((i+1)); sleep 0.5; done
 if kill -0 "$PID" 2>/dev/null; then
  echo "进程未在 10 秒内退出，发送 TERM 失败；不做强制杀进程，请人工检查 pid $PID" >&2
  exit 1
 fi
 echo "wardrobe-web 已停止（pid ${PID}）"
else
 echo "记录中的进程已不在运行（pid ${PID}）"
fi
rm -f "$PID_FILE"
