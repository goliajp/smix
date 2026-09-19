# Sourced by the devicectl e2e scripts. Two questions about a device, both
# answered by `devicectl list devices` and both needed before driving it.
#
# Booting and shutting down are NOT here. `teardown-restores-scan` reads
# each script for its own shutdown and its own record of who booted the
# device; a shutdown moved behind a helper is one that scan stops seeing.

# simulator_state <UDID> → Booted | Shutdown | … | absent
simulator_state() {
  xcrun simctl list devices -j | python3 -c '
import json, sys
u = sys.argv[1]
print(next((d["state"] for v in json.load(sys.stdin)["devices"].values() for d in v if d["udid"] == u), "absent"))' "$1"
}

# phone_tunnel_state <UDID> → connected | disconnected | … | absent
phone_tunnel_state() {
  xcrun devicectl list devices --json-output - 2>/dev/null | python3 -c '
import json, sys
u = sys.argv[1]
for d in json.load(sys.stdin)["result"]["devices"]:
    if d.get("hardwareProperties", {}).get("udid") == u:
        print(d.get("connectionProperties", {}).get("tunnelState", "unknown")); break
else:
    print("absent")' "$1"
}

# device_offers <UDID> <featureIdentifier> → yes | no | unlisted
# What the device itself lists, not what its kind is assumed to do.
device_offers() {
  xcrun devicectl list devices --json-output - 2>/dev/null | python3 -c '
import json, sys
u, feature = sys.argv[1], sys.argv[2]
for d in json.load(sys.stdin)["result"]["devices"]:
    if d.get("properties", {}).get("hardware", {}).get("udid") == u:
        caps = [c.get("featureIdentifier") for c in d.get("capabilities", [])]
        print("yes" if feature in caps else "no"); break
else:
    print("unlisted")' "$1" "$2"
}
