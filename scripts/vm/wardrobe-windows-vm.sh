#!/usr/bin/env bash
# Windows 11 ARM64 build VM lifecycle (ticket #35). QEMU/HVF headless + VNC + TPM 2.0 (swtpm).
#
#   scripts/vm/wardrobe-windows-vm.sh start      # VM with installer ISOs (autounattend)
#   scripts/vm/wardrobe-windows-vm.sh start-os   # boot installed OS only (no ISOs)
#   scripts/vm/wardrobe-windows-vm.sh stop | screen [file.ppm] | qmp-cmd '<json>' | type '<text>'
#
# Everything (disk, ISOs, TPM state, sockets, logs) lives on the backup disk.
# VNC: vnc://127.0.0.1:5901 ; SSH after first-logon: port 2224 (hostfwd).
# Note: QEMU 11.1.2 aarch64 tpm-tis-device cannot register MMIO under HVF
# (HV_BAD_ARGUMENT) and tpm-crb-device does not exist on this build, so the
# vTPM is opt-in via TPM=1; Windows setup uses the LabConfig bypasses from
# Autounattend.xml (attestation features unavailable — fine for a build VM).
set -euo pipefail

VMROOT="${WARDROBE_VM_ROOT:-/Users/links/.codex/external-workspace-volume/codex/vm}"
ISO="$VMROOT/iso"; DISKS="$VMROOT/disks"; SEEDS="$VMROOT/seeds"; LOGS="$VMROOT/logs"
TPMDIR="$VMROOT/tpm/win11-arm64"
FW=/opt/homebrew/share/qemu/edk2-aarch64-code.fd

base_args=(
  -machine virt,highmem=on -accel hvf -cpu host -smp 4 -m 8192
  -bios "$FW"
  -drive file="$DISKS/win11-arm64.qcow2",if=none,id=ssd,format=qcow2
  -device nvme,drive=ssd,serial=WARDROBE01,bootindex=1
  -device qemu-xhci,id=xhci
  -device usb-kbd -device usb-tablet
  -device ramfb
  -device virtio-net-pci,netdev=net0
  -netdev user,id=net0,hostfwd=tcp:127.0.0.1:2224-:22
  -vnc 127.0.0.1:1
  -qmp unix:"$LOGS/win11-qmp.sock",server,nowait
  -serial "file:$LOGS/win11-serial.log"
  -pidfile "$LOGS/win11.pid" -daemonize
)

tpm_args=()
if [ "${TPM:-0}" = "1" ]; then
  tpm_args=(-chardev socket,id=chrtpm,path="$TPMDIR/swtpm.sock"
            -tpmdev emulator,id=tpm0,chardev=chrtpm
            -device tpm-tis-device,tpmdev=tpm0)
fi

cmd="${1:-}"; shift || true
case "$cmd" in
  start)
    mkdir -p "$TPMDIR"
    if [ "${TPM:-0}" = "1" ] && [ ! -S "$TPMDIR/swtpm.sock" ]; then
      swtpm socket --tpm2 --tpmstate dir="$TPMDIR" \
        --ctrl type=unixio,path="$TPMDIR/swtpm.sock" \
        --log file="$LOGS/win11-swtpm.log",level=20 --daemon
    fi
    qemu-system-aarch64 "${base_args[@]}" ${tpm_args[@]+${tpm_args[@]}} \
      -drive if=none,id=cd0,format=raw,media=cdrom,readonly=on,file="$ISO/Win11_ARM64_en-us_26300.iso" \
      -device usb-storage,bus=xhci.0,drive=cd0,bootindex=0 \
      -drive if=none,id=cd1,format=raw,media=cdrom,readonly=on,file="$ISO/virtio-win.iso" \
      -device usb-storage,bus=xhci.0,drive=cd1 \
      -drive if=none,id=cd2,format=raw,media=cdrom,readonly=on,file="$SEEDS/wardrobe-setup.iso" \
      -device usb-storage,bus=xhci.0,drive=cd2
    echo "win11 VM launched (pid $(cat "$LOGS/win11.pid")); VNC 127.0.0.1:5901, SSH :2224 after setup"
    ;;
  start-os)
    qemu-system-aarch64 "${base_args[@]}" ${tpm_args[@]+${tpm_args[@]}}
    echo "win11 VM launched from disk (pid $(cat "$LOGS/win11.pid"))"
    ;;
  stop)
    [ -f "$LOGS/win11.pid" ] && kill "$(cat "$LOGS/win11.pid")" 2>/dev/null || true
    rm -f "$LOGS/win11.pid"
    pkill -f "swtpm socket.*win11-arm64" 2>/dev/null || true
    echo stopped ;;
  screen)
    OUT="${1:-$LOGS/win11-screen.ppm}"
    OUT_JSON="${OUT//\\/\\\\}"; OUT_JSON="${OUT_JSON//\"/\\\"}"
    { printf '{"execute":"qmp_capabilities"}\n'; sleep 0.3
      printf '{"execute":"screendump","arguments":{"filename":"%s"}}\n' "$OUT_JSON"; sleep 2; } \
      | nc -U "$LOGS/win11-qmp.sock" >/dev/null 2>&1 || true
    [ -f "$OUT" ] && sips -s format png "$OUT" --out "${OUT%.ppm}.png" >/dev/null 2>&1 && echo "${OUT%.ppm}.png" ;;
  qmp-cmd)
    { printf '{"execute":"qmp_capabilities"}\n'; sleep 0.3; printf '%s\n' "$1"; sleep 1; } \
      | nc -U "$LOGS/win11-qmp.sock" ;;
  type)
    # Type a string + Enter via QMP input-send-event, few keys per call:
    # large event bursts outrun the guest USB polling and lose key-ups
    # (stuck-key auto-repeat), so chunking is mandatory.
    TYPEFILE="$LOGS/win11-type.json"
    VMROOT="$VMROOT" python3 - "$1" "$TYPEFILE" <<'PYEOF'
import json, os, subprocess, sys, time
target, outfile = sys.argv[1], sys.argv[2]
combo = {':': ['shift', 'semicolon'], '_': ['shift', 'minus']}
simple = {' ': 'spc', '.': 'dot', '-': 'minus', '=': 'equal', '\\': 'backslash',
          '/': 'slash', ',': 'comma', ';': 'semicolon'}
def key(q, down):
    return {"type": "key", "data": {"down": down, "key": {"type": "qcode", "data": q}}}
def events_for(ch):
    keys = combo.get(ch, [simple.get(ch, ch)])
    evs = [key(q, True) for q in keys]
    evs += [key(q, False) for q in reversed(keys)]
    return evs
chunks, cur = [], []
for ch in target:
    evs = events_for(ch)
    if len(cur) + len(evs) > 6:
        chunks.append(cur); cur = []
    cur.extend(evs)
if cur:
    chunks.append(cur)
chunks.append([key('ret', True), key('ret', False)])
sock = os.environ['VMROOT'] + '/logs/win11-qmp.sock'
ok = True
for i, events in enumerate(chunks):
    payload = json.dumps({"execute": "input-send-event", "arguments": {"events": events}}) + "\n"
    proc = subprocess.run(
        ["nc", "-U", sock],
        input='{"execute":"qmp_capabilities"}\n' + payload,
        capture_output=True, text=True, timeout=15)
    if '"error"' in proc.stdout:
        print(f"chunk {i+1}: ERROR {proc.stdout.strip()[-140:]}")
        ok = False
    time.sleep(0.9)
print("OK" if ok else "FAILED")
PYEOF
    echo "typed: $1" ;;
  key)
    # Single key with minimal latency (boot-manager "press any key" windows are ~2s).
    Q="$1"
    { printf '{"execute":"qmp_capabilities"}\n{"execute":"input-send-event","arguments":{"events":[{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"%s"}}},{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"%s"}}}]}}\n' "$Q" "$Q"; sleep 1; } \
      | nc -U "$LOGS/win11-qmp.sock" >/dev/null
    echo "key: $Q" ;;
  *) echo "usage: $0 start|start-os|stop|screen|qmp-cmd|type '<text>'" >&2; exit 1 ;;
esac
