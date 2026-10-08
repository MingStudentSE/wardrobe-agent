#!/usr/bin/env bash
# Ensure a Tauri-produced .deb has a Debian-policy-valid Package field.
#
# Background: tauri-bundler derives the deb Package name from productName
# unchanged. With productName 电子衣橱, `dpkg -i` refuses the package
# ("invalid package name in 'Package' field: must start with an alphanumeric
# character", Debian policy 5.6.1 requires ^[a-z0-9][a-z0-9+.-]+$), while the
# CI smoke uses `dpkg --extract`, which never validates the control file — so
# the defect shipped unnoticed. Window titles and desktop-entry display names
# are unaffected by this fix: they come from app.windows[].title and the
# packaged .desktop file, not from the control field.
#
# Formal pipeline step (ticket #35): repack invalid packages under the ASCII
# identity `wardrobe-agent`, then ASSERT validity — both here and in the
# callers (scripts/vm/linux-build-and-package.sh, .github/workflows/
# media-bundles.yml). If the input is already valid, it is copied through.
#
# Usage: repack-deb-package-name.sh <in.deb> <out-dir> [package-name=wardrobe-agent]
# Output: <out-dir>/<package>_<version>_<arch>.deb (path echoed on stdout)
# NOTE: `dpkg-deb -f` cannot even READ the offending control (its parser
# rejects the CJK Package field outright, same as dpkg -i); `-R` raw-extract
# and `--ctrl-tarfile` do not parse, so all field reads go through -R.
set -euo pipefail

in="$1"
outdir="${2:?output directory required}"
name="${3:-wardrobe-agent}"

valid() { printf '%s' "$1" | grep -qE '^[a-z0-9][a-z0-9+.-]*$'; }

[ -f "$in" ] || { echo "repack-deb-package-name: input not found: $in" >&2; exit 1; }
valid "$name" || { echo "repack-deb-package-name: target package name invalid: $name" >&2; exit 1; }

read_field() { # $1 = deb, $2 = field name; via raw control stream (no parser)
  # control member is stored without "./" prefix by dpkg-deb; dump all and
  # take the first match from the leading control file.
  dpkg-deb --ctrl-tarfile "$1" | tar -xO | awk -v f="$2" \
    'index($0, f":")==1 { v=substr($0, length(f)+2); sub(/^ /, "", v); print v; exit }'
}

in_name="$(read_field "$in" Package)"
in_version="$(read_field "$in" Version)"
in_arch="$(read_field "$in" Architecture)"
[ -n "$in_name" ] && [ -n "$in_version" ] && [ -n "$in_arch" ] \
  || { echo "repack-deb-package-name: cannot read control fields of $in" >&2; exit 1; }

if valid "$in_name"; then
  echo "repack-deb-package-name: $in already valid (Package: $in_name); copying through" >&2
  out="$outdir/$(basename "$in")"
  mkdir -p "$outdir"
  cp "$in" "$out"
else
  echo "repack-deb-package-name: repacking $in (invalid Package: $in_name) -> $name" >&2
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  dpkg-deb -R "$in" "$work/pkg"
  sed -i "s/^Package: .*/Package: $name/" "$work/pkg/DEBIAN/control"
  out="$outdir/${name}_${in_version}_${in_arch}.deb"
  mkdir -p "$outdir"
  dpkg-deb -b "$work/pkg" "$out" >/dev/null
fi

out_name="$(dpkg-deb -f "$out" Package)"
valid "$out_name" || { echo "repack-deb-package-name: FAIL output Package invalid: $out_name" >&2; exit 1; }
[ "$out_name" = "$name" ] || { echo "repack-deb-package-name: FAIL output Package mismatch: $out_name != $name" >&2; exit 1; }
echo "$out"
