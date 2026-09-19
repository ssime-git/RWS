#!/bin/bash
# Build an experimental SSHFS locally. Never replaces system SSHFS.
set -euo pipefail
[[ $(uname -s) == Darwin ]] || { echo 'This build requires macOS.' >&2; exit 1; }
repo_root=$(cd -- "$(dirname -- "$0")/.." && pwd)
source_commit=9e35c39ba83f54a49a9df4bf0a629f26c60cc38c
if [[ -z ${DEVELOPER_DIR:-} && -d /Library/Developer/CommandLineTools ]]; then
  export DEVELOPER_DIR=/Library/Developer/CommandLineTools
fi
glib_prefix=${RWS_GLIB_PREFIX:-$(brew --prefix glib)}
[[ -f "$glib_prefix/include/glib-2.0/glib.h" ]] || { echo 'Install GLib or set RWS_GLIB_PREFIX.' >&2; exit 1; }
[[ -f /usr/local/include/fuse3/fuse.h ]] || { echo 'Install macFUSE development headers first.' >&2; exit 1; }
mkdir -p "$repo_root/.rws-local/sshfs"
build_dir=$(mktemp -d "$repo_root/.rws-local/sshfs/build.XXXXXX")
mkdir "$build_dir/compat"
base_url="https://raw.githubusercontent.com/libfuse/sshfs/$source_commit"
while read -r expected relative; do
  curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    --connect-timeout 15 --max-time 120 "$base_url/$relative" -o "$build_dir/$relative"
  actual=$(shasum -a 256 "$build_dir/$relative")
  [[ ${actual%% *} == "$expected" ]] || { echo "SHA-256 mismatch: $relative" >&2; exit 1; }
done <<'CHECKSUMS'
3314b2005c2554bde8d76fd835a52a66c2fe47a26caae5679819076b5911b437 sshfs.c
2b03ed08c41fe50f5c969ef87362023fce1bb73f5cc9d518dad2f50cac55823d cache.c
ce2d863daf29a8fef50cc9d53324918a9c033afd9622c8a0ad74a22b8155c749 cache.h
5253bc84a4ad1425da396d9c330a9791dd678f86bde707edd2eab40230d2f16d compat/darwin_compat.c
508f49074392153a24f1b7c00ab226aa16610eb977824b6f35bc692ff931b0f6 compat/darwin_compat.h
8177f97513213526df2cf6184d8ff986c675afb514d4e68a404010521b880643 COPYING
CHECKSUMS
/usr/bin/python3 "$repo_root/scripts/patch-sshfs.py" "$build_dir/sshfs.c"
cat > "$build_dir/config.h" <<'CONFIG'
#define PACKAGE_VERSION "3.7.5-rws-fskit3"
#define IDMAP_DEFAULT "user"
CONFIG
xcrun clang -DFUSE_DARWIN_ENABLE_EXTENSIONS=0 -DFUSE_USE_VERSION=31 \
  -D_REENTRANT -DHAVE_CONFIG_H -I/usr/local/include/fuse3 \
  -I"$glib_prefix/include/glib-2.0" -I"$glib_prefix/lib/glib-2.0/include" \
  -I"$build_dir" -I"$build_dir/compat" \
  "$build_dir/sshfs.c" "$build_dir/cache.c" "$build_dir/compat/darwin_compat.c" \
  -L/usr/local/lib -lfuse3 -L"$glib_prefix/lib" -lglib-2.0 -lgthread-2.0 \
  -framework Foundation -o "$build_dir/sshfs"
# Keep exact modified sources and GPL notice beside the experimental executable.
printf '%s\n' "Upstream: $source_commit" "Patches: disable unsupported extended rename; opt-in GLib NFC/NFD filename conversion; fresh directory enumeration handles" > "$build_dir/BUILD-INFO.txt"
printf 'Built experimental SSHFS (system executable unchanged):\n%s\n' "$build_dir/sshfs"
printf 'Use in this shell:\nexport RWS_SSHFS=%q\n' "$build_dir/sshfs"
