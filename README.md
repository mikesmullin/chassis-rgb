# chassis-rgb

CLI for this machine's PC lights: the Gigabyte TRX50 AERO D rev 1.2 chassis LEDs (onboard IT5701, `048d:5702`, chip id `0x57010100`) and the ASUS ROG ASTRAL RTX 5090 OC WHITE GPU (ENE SMBus, `0x67`). It does not start an SDK server.

```
chassis-rgb --mode static --color 280000 --brightness 50
chassis-rgb --mode pulse --color 00FF00 --speed 4
chassis-rgb --mode flash --color 00FFFF --speed 4
chassis-rgb --device gpu --mode static --color 280000
chassis-rgb --info
chassis-rgb --mode static --color 280000 --brightness 50 --save
```

`--device` is `all` (default), `chassis`, or `gpu`.

Success is one stdout line and exit 0. Failures go to stderr and exit 1.

## Modes that work on this board

`static`, `pulse`, and `flash` were confirmed on the strip.

`wave` is rejected: this board does not offer it and the packet blanked the strip during testing. OpenRGB advertises Color Cycle and Double Flash as well; those have not been tried.

Zones: `led`, `argb1` (ARGB_V2_1 and ARGB_V2_3 share one header), `argb2`, or `all`. On this case the visible strip is that onboard lighting. There is not a separate small LED to judge.

Brightness is 0-255. Pulse is capped at 100. Speed is 0 (fastest) through 9 (slowest).

## Save

`--save` sends one `CC 5E` after the effect, the same report GCC's `SaveSetting()` sends. It is off by default. Do not pass it on every color change.

One save of static `280000` was sent. The strip stayed red. Survival across reboot is not confirmed. Check the color during BIOS/POST before the OS starts, then after a cold boot, with no software reapply.

## Cold boot

After a reboot the controller sits in its POST effect and used to ignore this tool's packets. Every apply now starts with the same reset prefix official software sends (clear effect registers `0x20`-`0x27`, full apply, beat off, header-enable baseline), captured over USB. The fix is verified on a warm controller; confirming it from a true cold boot still needs a reboot.

## Device access

The tool refuses a `048d:5702` whose info report is not this IT5701. Another process must not hold the interface. A Windows VM hostdev for `048d:5702` has to be detached first. `lsusb` still listing the device does not mean the host can open it.

## GPU

The GPU backend speaks the ENE SMBus protocol directly: effect colors, mode, speed, direction, apply, and the `0xAA` flash save. Chassis pulse/flash map onto the ENE breathing/flashing modes. The ENE speed register only accepts 0-4, so the 0-9 speed is scaled down (4 lands on normal). Brightness scales the color values; the chip has no brightness register.

Only the exact version string seen on this card (`AUMA0-E6K5-1113`) is driven, with the V2 color registers and the config-table LED count. Anything else is refused. See `NOTES.md` for how that quirk was found: OpenRGB's percentage speed once wrote `0x28`, which selected Breathing and then did nothing.

## Build

```
cargo test
cargo build --release
```

The binary is `target/release/chassis-rgb`.
