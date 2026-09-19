#!/bin/bash
# Local developer build; production enables Sparkle metadata but still needs signing.
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || { echo 'Initial app build supports Apple silicon macOS only.' >&2; exit 1; }
mode=${1:-development}
[[ "$mode" == development || "$mode" == production ]] || { echo 'Usage: build-macos-app.sh [development|production]' >&2; exit 1; }
export MACOSX_DEPLOYMENT_TARGET=13.0
version=$(python3 scripts/macos_metadata.py)
output="$repo_root/dist/$mode"
app="$output/RWS.app"
[[ ! -e "$output" ]] || { echo "Output exists: $output. Move it aside before building." >&2; exit 1; }
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/bin" "$app/Contents/Frameworks"
metadata_args=(--output "$app/Contents/Info.plist")
if [[ "$mode" == production ]]; then
  : "${RWS_SPARKLE_PUBLIC_KEY:?Set the production Sparkle public key}"
  metadata_args+=(--production --public-key "$RWS_SPARKLE_PUBLIC_KEY")
fi
python3 scripts/macos_metadata.py "${metadata_args[@]}"
cargo build --locked --release --target aarch64-apple-darwin
swift build --package-path macos --configuration release --product RWSApp
swift_bin=$(swift build --package-path macos --configuration release --show-bin-path)
install -m 755 "$swift_bin/RWSApp" "$app/Contents/MacOS/RWSApp"
install -m 755 target/aarch64-apple-darwin/release/rws "$app/Contents/Resources/bin/rws"
# SwiftPM resolves the pinned binary target; copy its native macOS framework.
framework=$(find macos/.build/artifacts -path '*/macos-arm64_x86_64/Sparkle.framework' -type d -print -quit)
[[ -n "$framework" ]] || { echo 'Sparkle macOS framework not found.' >&2; exit 1; }
ditto "$framework" "$app/Contents/Frameworks/Sparkle.framework"
sparkle_license=$(find macos/.build/checkouts -maxdepth 2 -iname LICENSE -path "*[Ss]parkle*" -print -quit)
[[ -n "$sparkle_license" ]] || { echo "Sparkle license not found." >&2; exit 1; }
cp "$sparkle_license" "$app/Contents/Resources/Sparkle-LICENSE.txt"
# SPM executable lookup uses @rpath; installed bundles resolve their own Frameworks.
if ! otool -l "$app/Contents/MacOS/RWSApp" | grep -q '@executable_path/../Frameworks'; then
  install_name_tool -add_rpath '@executable_path/../Frameworks' "$app/Contents/MacOS/RWSApp"
fi
if [[ "$mode" == development ]]; then
  codesign --force --deep --sign - "$app"
  ditto -c -k --sequesterRsrc --keepParent "$app" "$output/RWS-$version-arm64-development.zip"
fi
printf 'Built %s\n' "$app"
