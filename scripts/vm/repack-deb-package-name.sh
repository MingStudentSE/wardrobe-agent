#!/usr/bin/env bash
# Repack a Tauri-produced .deb whose Package field is not Debian-policy valid
# (Debian policy 5.6.1: package names must match ^[a-z0-9][a-z0-9+.-]+$).
#
# Background: tauri-bundler derives the deb Package name from productName
# unchanged. With productName 电子衣橱, `dpkg -i` refuses the package
# ("invalid package name in 'Package' field"), while CI's `dpkg --extract`
# smoke never validates the control file, so the defect ships unnoticed.
#
# This is a verification-path workaround for ticket #35 multiplatform infra:
# it proves the package installs and runs once the name is policy-valid.
# The real fix (ASCII bundle name or an upstream name override) is tracked in
# docs/verification/ticket-35/multiplatform-infra.md and must be decided by
# the release workstream (it renames installer artifacts on every platform).
#
# Usage: repack-deb-package-name.sh <in.deb> <out.deb> <valid-package-name>
set -euo pipefail
in="$1"; out="$2"; name="$3"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
dpkg-deb -R "$in" "$work/pkg"
sed -i "s/^Package: .*/Package: $name/" "$work/pkg/DEBIAN/control"
dpkg-deb -b "$work/pkg" "$out" >/dev/null
echo "repacked: $out (Package: $name)"
