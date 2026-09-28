# Session notes

Hardware checked on this machine: Gigabyte TRX50 AERO D rev 1.2, chassis controller USB HID `048d:5702`, product `IT5701-GIGABYTE V8.0.27.0`, chip id `0x57010100`, info-report `support_cmd_flag` 0. GPU is separate: ASUS ROG ASTRAL GeForce RTX 5090 OC WHITE, ENE SMBus, `/dev/i2c-5` address `0x67`. This CLI does not drive the GPU.

The controller was passed through to the libvirt session domain `revm`. Detach that hostdev before opening hidraw. `lsusb` still listing `048d:5702` does not mean the host can open it.

## What was seen

| Command | Result |
|---|---|
| static `0000FF` brightness 80, all zones | Strip turned blue. The small onboard LED is the same strip, not a separate visible zone. |
| static `280000` brightness 50 | Solid dim red. |
| pulse `00FF00` brightness 80 speed 4, then speed 2 | Strip breathed green. |
| flash `00FFFF` brightness 80 speed 4 | Strip blinked cyan. |
| wave `FF8800` | Strip turned off. Wave was removed from the CLI. Do not send wave to this board. |
| static `280000` brightness 50 `--save` | One `CC 5E` after the effect. Strip stayed red. POST persistence was not tested. |

OpenRGB's motherboard mode list for this board is Static, Breathing, Flashing, Color Cycle, Double Flash, and Random. Wave is absent. Color Cycle and Double Flash were not tried.

## Reports

64-byte HID feature reports, report id `CC`.

- Effect packet per zone: header `0x20 + led`, `zone0 = 1 << led`, effect type, brightness, color as little-endian `0x00RRGGBB` (bytes B, G, R, 0).
- Zone ids used here: `led` = 4, `argb1` = 5 (ARGB_V2_1 and ARGB_V2_3 share that header), `argb2` = 6.
- Apply: `CC 28` plus the OR of those zone bits. All three zones is mask `0x70`.
- Enable digital-header effects before an effect packet: `CC 32 00`.
- Save: `CC 5E 00...`, once, only with `--save`. This is GCC `MCU_8297::SaveSetting()`. It is not gated by `support_cmd_flag`. `CC 47` is a different keep command and this firmware reports that bit clear, so this tool does not send it.
- Info select: `CC 60`, then GET_FEATURE.

Pulse brightness is capped at 100. Speed on this chassis controller is 0 (fastest) through 9 (slowest).

## GPU, if matching it from the agent

OpenRGB still prints `Connection attempt failed` because its SDK client fails to connect to itself. Judge the GPU command by exit code, not that line.

Target the GPU by name or index and do not let OpenRGB open the Gigabyte USB detector while this CLI holds the chassis. Opening the motherboard controller switches it to Direct and freezes a running hardware effect.

GPU Breathing is hardware, but its speed register only accepts 0 through 4 (0 fastest). OpenRGB's `--speed` percentage math can write an invalid value such as `0x28`, which selects Breathing and then does nothing. A direct register write of speed `0` made magenta breathing visible. Static color through `openrgb --device 0 --mode static` did work.

## Cold-boot failure and fix

After a reboot, `chassis-rgb` reported success but the strip stayed in rainbow. OpenRGB's static command worked on the same cold controller. A usbmon capture of OpenRGB showed the difference: it always sends a reset prefix first (zero registers `0x20`-`0x27`, `CC 28 FF 00`, `CC 31 00`, `CC 32 1B`), then interleaves header-enables (`1A` before ARGB_1, `18` before ARGB_2) with the effect packets, then a masked `CC 28 70` apply. The tool now sends that same sequence. Verified warm; a real reboot is still needed to confirm the cold path. USB re-enumeration does not reset the controller, so it cannot substitute for the reboot test.

## Not done

- No POST or cold-boot check after `--save`. Persistence is unconfirmed until the dim red is visible during firmware POST before the OS starts, with no OpenRGB reapply.
- No second `--save`. Do not commit on every color change.
- Home agent `pc_light_color` still shells out to OpenRGB for chassis and GPU together. It was not changed.
- No BIOS/SMI path. That path is LED power-state on older boards, not this IT5701 color save.
