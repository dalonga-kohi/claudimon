#!/usr/bin/env bash
# Regenerate the README screenshots in docs/screenshots/.
# Needs: cargo, python3, and the `kbd` console fonts (standard on Arch).
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --quiet
bin=target/release/claudimon
out=docs/screenshots
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$out"

# name | scene | cols | rows | env
shots=(
  "sanctuary|garden|120|34|CLAUDIMON_HOUR=17.5"
  "encounter|encounter:Haikitten@meadow|100|30|CLAUDIMON_HOUR=12"
  "battle|battle|100|30|CLAUDIMON_HOUR=12"
  "town-night|town|120|34|CLAUDIMON_HOUR=21.5"
  "caves|caves|100|30|CLAUDIMON_ZOOM=in"
  "snow|meadow|120|34|CLAUDIMON_HOUR=11 CLAUDIMON_WEATHER=snow"
  "lighthouse|lighthouse|120|34|CLAUDIMON_HOUR=22"
  "mythos|encounter:Mythos@isle|100|30|CLAUDIMON_HOUR=12"
  "claude|claude|100|30|CLAUDIMON_HOUR=12"
  "catalog|catalog|100|30|"
  "team|team|100|30|"
)

for s in "${shots[@]}"; do
  IFS='|' read -r name scene cols rows envs <<<"$s"
  # The snapshot never touches a real save; point it somewhere harmless anyway.
  env CLAUDIMON_SAVE="$tmp/none.json" $envs "$bin" --snapshot "$scene" "$cols" "$rows" "$tmp/$name.ppm"
  python3 tools/render_cells.py "$tmp/$name.ppm.cells" "$out/$name.png"
  echo "  $out/$name.png"
done
