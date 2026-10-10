#!/usr/bin/env bash
# Local Ubuntu build VM lifecycle for wardrobe-agent release infrastructure (ticket #35).
#
# All VM disks, ISOs and logs live on the 1TB backup disk
# (/Users/links/.codex/external-workspace-volume, ~70GiB free at 2026-10-08);
# nothing is written to the system disk. No UTM data directory is involved:
# QEMU (already installed via Homebrew) runs the VMs headless, so there is no
# ~/Library/Containers/com.utmapp.UTM to migrate.
#
# Usage:
#   scripts/vm/wardrobe-build-vm.sh create-arm64   # first boot + cloud-init provision
#   scripts/vm/wardrobe-build-vm.sh launch-arm64   # start again after shutdown
#   scripts/vm/wardrobe-build-vm.sh ssh-arm64 [cmd...]
#   scripts/vm/wardrobe-build-vm.sh create-amd64   # x86_64 TCG feasibility VM
#   scripts/vm/wardrobe-build-vm.sh launch-amd64 | ssh-amd64 | stop-arm64 | stop-amd64
#
# arm64 runs with Hypervisor.framework acceleration (near-native);
# amd64 falls back to TCG full emulation on Apple Silicon (slow; used for
# feasibility measurement, see docs/verification/ticket-35/multiplatform-infra.md).
set -euo pipefail

VMROOT="${WARDROBE_VM_ROOT:-/Users/links/.codex/external-workspace-volume/codex/vm}"
ISO="$VMROOT/iso"
DISKS="$VMROOT/disks"
SEEDS="$VMROOT/seeds"
LOGS="$VMROOT/logs"
QEMU_FW=/opt/homebrew/share/qemu
IMAGE_BASE_URL="https://cloud-images.ubuntu.com/releases/24.04/release"

 die() { echo "wardrobe-build-vm: $*" >&2; exit 1; }

ensure_dirs() { mkdir -p "$ISO" "$DISKS" "$SEEDS" "$LOGS"; }

fetch_image() { # $1 = filename
  local f="$1"
  if [ ! -f "$ISO/$f" ]; then
    curl -fL --retry 3 -o "$ISO/$f" "$IMAGE_BASE_URL/$f"
  fi
  (cd "$ISO" && curl -fsSL "$IMAGE_BASE_URL/SHA256SUMS" -o SHA256SUMS && awk -v f="$f" '$2=="*"f || $2==f' SHA256SUMS | shasum -a 256 -c -)
}

make_seed_iso() { # $1 = seed name stem (user-data-$1 + meta-data-$1 must exist)
  local stem="$1" dir="$SEEDS/cidata-$1"
  mkdir -p "$dir"
  cp "$SEEDS/user-data-$stem" "$dir/user-data"
  cp "$SEEDS/meta-data-$stem" "$dir/meta-data"
  hdiutil makehybrid -iso -joliet -default-volume-name cidata -o "$SEEDS/seed-$stem.iso" "$dir" >/dev/null
}

launch_vm() { # $1=arch key (arm64|amd64; disk/seed/pid names) $2=extra qemu args...
  local arch="$1"; shift
  local qemu_arch="$arch"
  [ "$qemu_arch" = amd64 ] && qemu_arch=x86_64
  [ "$qemu_arch" = arm64 ] && qemu_arch=aarch64
  local disk="$DISKS/wardrobe-build-$arch.qcow2"
  local pidfile="$LOGS/$arch.pid"
  [ -f "$disk" ] || die "disk missing: $disk (run create-$arch first)"
  if [ -f "$pidfile" ] && ps -p "$(cat "$pidfile")" >/dev/null 2>&1; then
    echo "VM $arch already running (pid $(cat "$pidfile"))"; return 0
  fi
  qemu-system-"$qemu_arch" "$@" \
    -drive file="$disk",if=virtio,format=qcow2 \
    -drive file="$SEEDS/seed-$arch.iso",if=virtio,format=raw,media=cdrom,readonly=on \
    -device virtio-net-pci,netdev=net0 \
    -netdev user,id=net0,hostfwd=tcp:127.0.0.1:$([ "$arch" = arm64 ] && echo 2222 || echo 2223)-:22 \
    -display none \
    -serial "file:$LOGS/$arch-serial.log" \
    -pidfile "$pidfile" -daemonize
  echo "VM $arch launched (pid $(cat "$pidfile"))"
}

arm64_qemu_args=(-machine virt,highmem=on -accel hvf -cpu host -smp 4 -m 6144 -bios "$QEMU_FW/edk2-aarch64-code.fd")
amd64_qemu_args=(-machine q35 -accel tcg -smp 4 -m 4096) # default SeaBIOS; ubuntu cloud img boots BIOS+UEFI

cmd="${1:-}"; shift || true
case "$cmd" in
  create-arm64)
    ensure_dirs
    fetch_image ubuntu-24.04-server-cloudimg-arm64.img
    [ -f "$SEEDS/seed-arm64.iso" ] || make_seed_iso arm64
    cp "$ISO/ubuntu-24.04-server-cloudimg-arm64.img" "$DISKS/wardrobe-build-arm64.qcow2"
    qemu-img resize "$DISKS/wardrobe-build-arm64.qcow2" 40G
    launch_vm arm64 "${arm64_qemu_args[@]}"
    ;;
  launch-arm64) launch_vm arm64 "${arm64_qemu_args[@]}" ;;
  create-amd64)
    ensure_dirs
    fetch_image ubuntu-24.04-server-cloudimg-amd64.img
    [ -f "$SEEDS/seed-amd64.iso" ] || make_seed_iso amd64
    cp "$ISO/ubuntu-24.04-server-cloudimg-amd64.img" "$DISKS/wardrobe-build-amd64.qcow2"
    qemu-img resize "$DISKS/wardrobe-build-amd64.qcow2" 30G
    launch_vm amd64 "${amd64_qemu_args[@]}"
    ;;
  launch-amd64) launch_vm amd64 "${amd64_qemu_args[@]}" ;;
  ssh-arm64) exec ssh -i "$SEEDS/id_ed25519_wardrobe_build" -p 2222 \
      -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null ubuntu@127.0.0.1 "$@" ;;
  ssh-amd64) exec ssh -i "$SEEDS/id_ed25519_wardrobe_build" -p 2223 \
      -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null ubuntu@127.0.0.1 "$@" ;;
  stop-arm64|stop-amd64)
    local_arch=${cmd#stop-}
    [ -f "$LOGS/$local_arch.pid" ] && kill "$(cat "$LOGS/$local_arch.pid")" && rm -f "$LOGS/$local_arch.pid" && echo "VM $local_arch stopped" ;;
  *) die "unknown command: $cmd (see header for usage)" ;;
esac
