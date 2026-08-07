# smix-usbmux

Speak Apple's usbmux protocol: list attached iOS devices, and open a TCP
tunnel to a port on one.

A physical iPhone does not share the host's loopback. A server running on
the device — like smix's XCUITest runner, which listens on the device's
`127.0.0.1:22087` — cannot be reached by connecting to that address on the
Mac. Something has to bridge the two.

That something is `usbmuxd`, and it is **Apple's own daemon, already on
the machine**: `/var/run/usbmuxd` exists wherever the developer tools do.
The third-party library usually reached for at this point turns out to be
a client wrapper around something already present.

So what is left to write is the protocol, and it is small: a 16-byte
header and a plist with four keys. That is this crate.

## What it does

```rust
use smix_usbmux::{list_devices, connect};

for d in list_devices()? {
    println!("{} over {}", d.serial, d.connection_type);
}

// The returned stream IS the tunnel — read and write it as if the port
// were local.
let mut sock = connect(device_id, 22087)?;
# Ok::<(), smix_usbmux::UsbmuxError>(())
```

## Three decisions worth knowing

**No third-party dependencies, including for plist.** The used subset is
dictionaries, arrays, strings and integers — four types. A general plist
library would be a dependency for a format whose relevant part fits on a
page, and writing one here would be a second thing to maintain. What
cannot be self-made is the daemon, and Apple ships it.

**Unsupported input is an error, never a skip.** A parser that ignores
tags it does not understand turns a protocol change into missing data
instead of into a message. Same for a truncated read: an empty device list
would make "nothing is plugged in" and "the answer did not arrive" the
same answer.

**A closed port is reported as the device refusing, not as a timeout.**
`connect` to a port nobody listens on comes back as
`UsbmuxError::Refused` naming the port. A caller that had to discover this
by waiting would have no way to tell it apart from a slow device.

## Two things the wire will teach you the hard way

- **The length field counts its own header.** Get it wrong and nothing
  fails at encode time — the daemon waits for bytes that never come. It
  surfaces as a hang, far from the mistake.
- **The port travels network-endian inside a plist integer.** 22087
  (0x5647) goes on the wire as 0x4756. Reversed, the daemon dials a port
  nobody is on, and the failure reads as the device refusing you.

## When this is the wrong crate

| You want | Use |
|---|---|
| App install / launch on a device | `xcrun devicectl` |
| To reach an Android device | `adb forward` — it already does this |
| A simulator | nothing; it shares the host's loopback |
