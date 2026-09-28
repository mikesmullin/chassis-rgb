# chassis-rgb

CLI for the Gigabyte TRX50 AERO D rev 1.2 chassis LEDs. It talks only to the onboard IT5701 (`048d:5702`, chip id `0x57010100`). It does not control the GPU and does not start an SDK server.

```
chassis-rgb --mode static --color 280000 --brightness 50
chassis-rgb --mode pulse --color 00FF00 --speed 4
chassis-rgb --mode flash --color 00FFFF --speed 4
chassis-rgb --info
chassis-rgb --mode static --color 280000 --brightness 50 --save
```

Success is one stdout line and exit 0. Failures go to stderr and exit 1.

## Modes that work on this board

`static`, `pulse`, and `flash` were confirmed on the strip.

`wave` is accepted by the CLI but this board does not support it. Sending it turned the strip off. Do not use it here. OpenRGB advertises Color Cycle and Double Flash as well; those have not been tried.

Zones: `led`, `argb1` (ARGB_V2_1 and ARGB_V2_3 share one header), `argb2`, or `all`. On this case the visible strip is that onboard lighting. There is not a separate small LED to judge.

Brightness is 0-255. Pulse is capped at 100. Speed is 0 (fastest) through 9 (slowest).

## Save

`--save` sends one `CC 5E` after the effect, the same report GCC's `SaveSetting()` sends. It is off by default. Do not pass it on every color change.

One save of static `280000` was sent. The strip stayed red. Survival across reboot is not confirmed. Check the color during BIOS/POST before the OS starts, then after a cold boot, with no software reapply.

## Device access

The tool refuses a `048d:5702` whose info report is not this IT5701. Another process must not hold the interface. A Windows VM hostdev for `048d:5702` has to be detached first. `lsusb` still listing the device does not mean the host can open it.

## GPU

Matching the GPU is a separate OpenRGB command. See `NOTES.md` for the ENE speed-register quirk: Breathing only accepts speed 0-4, and OpenRGB's percentage speed can write an invalid value that looks like a solid color.

## Build

```
cargo test
cargo build --release
```

The binary is `target/release/chassis-rgb`.
