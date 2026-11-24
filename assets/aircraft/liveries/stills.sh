#!/bin/sh
# Renders the aircraft picker's picture of every paint scheme (aircraft::Livery::still):
#
#   cargo build --release && sh assets/aircraft/liveries/stills.sh [key ...]
#
# Each is shot with --still (the aircraft held in the air, gear down, engines
# stopped, cut out of its surroundings by its body-frame box, post.wgsl
# `still_alpha`) at three times the picture's size, then cropped to the
# aircraft and fitted to 680 x 392 (fit.py). Keys: c172 dr400 bonanza 747 g650.
set -e
cd "$(dirname "$0")/../../.."
OUT=assets/aircraft/liveries
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
shoot() { # key, camera yaw,pitch,distance, then one picture name per scheme in order
    key=$1; cam=$2; shift 2
    i=0
    for name in "$@"; do
        ./target/release/fablegame --headless --still --no-menu --offline --mute --aircraft "$key" --livery $i \
            --agl 1500 --speed 0 --clouds 0 --time 13 --no-taa --render-scale 1 --width 2040 --height 1176 \
            --hold 60 --shot "$TMP/$name.png@8@$cam" > "$TMP/log" 2>&1
        python3 "$OUT/fit.py" "$TMP/$name.png" "$OUT/$name.png"
        echo "$OUT/$name.png"
        i=$((i + 1))
    done
}
want() { [ $# -eq 0 ] || echo " $ARGS " | grep -q " $1 "; }
ARGS="$*"
if [ -z "$ARGS" ] || want c172; then shoot c172 130,0.45,9 c172_none c172_navy c172_royal c172_green c172_burgundy; fi
if [ -z "$ARGS" ] || want dr400; then shoot dr400 130,0.45,9 dr400_classic dr400_sunburst dr400_aeroclub dr400_green; fi
if [ -z "$ARGS" ] || want bonanza; then shoot bonanza 130,0.45,9 bonanza_tan bonanza_blue bonanza_maroon bonanza_green bonanza_white; fi
if [ -z "$ARGS" ] || want 747; then shoot 747 130,0.45,9 b747_classic b747_orivante b747_kestralis; fi
if [ -z "$ARGS" ] || want g650; then shoot g650 130,0.45,9 g650_private g650_orivante g650_kestralis; fi
