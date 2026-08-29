#!/bin/sh

# Package a locally built macOS App for the unsigned MVP channel.
# This is deliberately not a substitute for Developer ID signing + notarization.
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"

app_path="${1:-$project_dir/src-tauri/target/release/bundle/macos/LabFlow.app}"
output_dir="${2:-$project_dir/src-tauri/target/release/bundle/macos}"
version=$(node -p "require('./package.json').version")
archive_name="LabFlow-$version-Apple-Silicon.zip"
dmg_name="LabFlow-$version-Apple-Silicon.dmg"
stage_dir=$(mktemp -d "${TMPDIR:-/tmp}/labflow-macos-package.XXXXXX")

cleanup() {
  rm -rf "$stage_dir"
}
trap cleanup EXIT INT TERM

if [ ! -d "$app_path" ]; then
  echo "LabFlow.app not found: $app_path" >&2
  exit 1
fi

mkdir -p "$output_dir"

# Tauri creates the app bundle but, without a Developer ID identity, does not
# seal the bundle resources. Sign the complete bundle ad-hoc so Gatekeeper sees
# a structurally valid App rather than reporting it as damaged.
codesign --force --deep --sign - "$app_path"
codesign --verify --deep --strict --verbose=2 "$app_path"

ditto "$app_path" "$stage_dir/LabFlow.app"
ditto -c -k --sequesterRsrc --keepParent "$stage_dir/LabFlow.app" "$output_dir/$archive_name"

mkdir -p "$stage_dir/dmg"
ditto "$app_path" "$stage_dir/dmg/LabFlow.app"
ln -s /Applications "$stage_dir/dmg/Applications"
hdiutil create -volname LabFlow -srcfolder "$stage_dir/dmg" -ov -format UDZO "$output_dir/$dmg_name" >/dev/null

echo "Created: $output_dir/$archive_name"
echo "Created: $output_dir/$dmg_name"
