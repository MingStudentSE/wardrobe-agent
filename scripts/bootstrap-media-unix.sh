#!/usr/bin/env bash
set -euo pipefail
# Build only the HEVC decoder; no GPL x265 encoder. No global installation.
media_root="${1:?Provide an isolated media build directory}"
mkdir -p "$media_root"
media_root="$(cd "$media_root" && pwd)"
media_prefix="$media_root/install"
export PKG_CONFIG_PATH="$media_prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export CMAKE_PREFIX_PATH="$media_prefix${CMAKE_PREFIX_PATH:+:$CMAKE_PREFIX_PATH}"
for project in libde265 libheif; do
  case "$project" in
    libde265) ref=v1.1.3 ;;
    libheif) ref=v1.23.1 ;;
  esac
  if [ ! -d "$media_root/$project/.git" ]; then
    git clone --depth 1 --branch "$ref" "https://github.com/strukturag/$project.git" "$media_root/$project"
  fi
  git -C "$media_root/$project" describe --tags --exact-match | diff - <(echo "$ref")
done
cmake -S "$media_root/libde265" -B "$media_root/libde265-build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$media_prefix" -DCMAKE_INSTALL_LIBDIR=lib -DBUILD_SHARED_LIBS=ON -DENABLE_SDL=OFF -DENABLE_DEC265=OFF -DENABLE_ENCODER=OFF
cmake --build "$media_root/libde265-build" --parallel 4
cmake --install "$media_root/libde265-build"
media_options=(-DBUILD_SHARED_LIBS=ON -DBUILD_TESTING=OFF -DWITH_EXAMPLES=OFF -DWITH_GDK_PIXBUF=OFF -DENABLE_PLUGIN_LOADING=OFF -DWITH_LIBDE265=ON -DWITH_LIBDE265_PLUGIN=OFF -DWITH_LIBSHARPYUV=OFF)
for codec in X265 X264 KVAZAAR UVG266 VVDEC VVENC OpenH264_DECODER DAV1D AOM_DECODER AOM_ENCODER SvtEnc RAV1E JPEG_DECODER JPEG_ENCODER OpenJPEG_ENCODER OpenJPEG_DECODER FFMPEG_DECODER OPENJPH_ENCODER; do
  media_options+=("-DWITH_${codec}=OFF")
done
cmake -S "$media_root/libheif" -B "$media_root/libheif-build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$media_prefix" -DCMAKE_INSTALL_LIBDIR=lib "${media_options[@]}"
cmake --build "$media_root/libheif-build" --parallel 4
cmake --install "$media_root/libheif-build"
printf 'Media prefix: %s\nUse PKG_CONFIG_PATH=%s/lib/pkgconfig and runtime LD_LIBRARY_PATH / DYLD_LIBRARY_PATH=%s/lib\n' "$media_prefix" "$media_prefix" "$media_prefix"
