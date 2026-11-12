# tools/run.sh <map> <tank id> <WTV_DEMO view> <name>: start the app in demo mode, capture shots/<name>.png, close it.
# Overwrites the app's remembered map/tank (%APPDATA%/wtv-viewer.json). WAIT=seconds before the capture (default 25).
cd "$(dirname "$0")/.." && mkdir -p shots
printf '{"map":"%s","tank":"%s","shell":"0:0"}' "$1" "$2" > "$APPDATA/wtv-viewer.json"
WTV_DEMO="$3" ./target/release/wtv.exe > "shots/$4.log" 2>&1 &
for i in $(seq 1 40); do sleep 1; grep -q . "shots/$4.log" && break; done
sleep "${WAIT:-25}"
powershell -ExecutionPolicy Bypass -File tools/shot.ps1 "$(cygpath -w "$PWD/shots/$4.png")"
taskkill //IM wtv.exe //F > /dev/null 2>&1
tail -3 "shots/$4.log"
