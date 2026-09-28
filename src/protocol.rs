//! IT5701 HID feature reports for the TRX50 AERO D chassis controller.
//!
//! 64-byte report, id 0xCC. Encoded from the observed report layout, not copied
//! from OpenRGB. Timing steps are the controller's own Gigabyte ranges.

pub const REPORT_LEN: usize = 64;
pub const REPORT_ID: u8 = 0xCC;
pub const VID: u16 = 0x048D;
pub const PID: u16 = 0x5702;
/// Chip id from the CC 60 info report on this board.
pub const CHIP_ID: u32 = 0x5701_0100;

pub const LED_C: u8 = 4;
pub const ARGB_1: u8 = 5;
pub const ARGB_2: u8 = 6;

// Wave (6) is intentionally absent: this board does not offer it and the
// packet blanks the strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Static = 1,
    Pulse = 2,
    Flash = 3,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "static" => Some(Self::Static),
            "pulse" | "breath" | "breathing" => Some(Self::Pulse),
            "flash" | "blink" | "blinking" => Some(Self::Flash),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Pulse => "pulse",
            Self::Flash => "flash",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn parse(s: &str) -> Option<Self> {
        let hex = s.trim().trim_start_matches('#').trim_start_matches("0x");
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self {
            r: u8::from_str_radix(&hex[0..2], 16).ok()?,
            g: u8::from_str_radix(&hex[2..4], 16).ok()?,
            b: u8::from_str_radix(&hex[4..6], 16).ok()?,
        })
    }

    pub fn hex(self) -> String {
        format!("{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

pub fn zones_from_name(name: &str) -> Option<Vec<u8>> {
    match name.to_ascii_lowercase().as_str() {
        "all" => Some(vec![LED_C, ARGB_1, ARGB_2]),
        "led" | "led_c" => Some(vec![LED_C]),
        "argb1" | "argb_v2_1" => Some(vec![ARGB_1]),
        "argb2" | "argb_v2_2" => Some(vec![ARGB_2]),
        _ => None,
    }
}

pub fn zone_name(led: u8) -> &'static str {
    match led {
        LED_C => "led",
        ARGB_1 => "argb1",
        ARGB_2 => "argb2",
        _ => "unknown",
    }
}

fn put_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// Brightness the effect packet will actually carry.
/// Pulse is limited to 100; the controller misbehaves above that.
pub fn effect_brightness(mode: Mode, brightness: u8) -> u8 {
    match mode {
        Mode::Pulse => brightness.min(100),
        _ => brightness,
    }
}

/// Hardware effect packet for one zone. `speed` is 0 (fastest) through 9 (slowest).
pub fn effect_report(led: u8, mode: Mode, brightness: u8, speed: u8, color: Color) -> [u8; REPORT_LEN] {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    buf[1] = 0x20 + led;
    put_u32(&mut buf, 2, 1u32 << led);
    buf[11] = mode as u8;
    buf[12] = effect_brightness(mode, brightness);
    // color0 is 0x00RRGGBB, stored little-endian: B, G, R, 0
    buf[14] = color.b;
    buf[15] = color.g;
    buf[16] = color.r;

    let speed = speed.min(9);
    match mode {
        Mode::Static => {}
        Mode::Pulse => {
            let period = if speed <= 6 {
                400 + u16::from(speed) * 100
            } else {
                1000 + u16::from(speed - 6) * 200
            };
            put_u16(&mut buf, 22, period);
            put_u16(&mut buf, 24, period);
            put_u16(&mut buf, 26, 200);
        }
        Mode::Flash => {
            put_u16(&mut buf, 22, 100);
            put_u16(&mut buf, 24, 100);
            put_u16(&mut buf, 26, u16::from(speed) * 200 + 700);
        }
    }
    buf
}

/// Commit the zones whose effect packets were just written.
pub fn apply_report(zones: &[u8]) -> [u8; REPORT_LEN] {
    let mut mask = 0u32;
    for &led in zones {
        mask |= 1u32 << led;
    }
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    buf[1] = 0x28;
    put_u32(&mut buf, 2, mask);
    buf
}

/// Cold-boot prefix, captured from official software traffic: clear the effect
/// registers, commit the clear with a full apply, switch beat off. Without
/// this a freshly booted controller stays in its POST effect and ignores the
/// per-zone packets.
pub fn reset_reports() -> Vec<[u8; REPORT_LEN]> {
    let mut out = Vec::with_capacity(10);
    for reg in 0x20u8..=0x27u8 {
        let mut buf = [0u8; REPORT_LEN];
        buf[0] = REPORT_ID;
        buf[1] = reg;
        out.push(buf);
    }
    let mut apply = [0u8; REPORT_LEN];
    apply[0] = REPORT_ID;
    apply[1] = 0x28;
    apply[2] = 0xFF;
    out.push(apply);
    let mut beat = [0u8; REPORT_LEN];
    beat[0] = REPORT_ID;
    beat[1] = 0x31;
    out.push(beat);
    out
}

/// Header-enable writes matching official software on this layout, paired
/// with the ARGB zone each one precedes. The baseline (0x1B: strip and gen2
/// bits disabled) goes before the first ARGB effect packet. Enabling ARGB_1
/// clears bit 0 (0x1A); enabling ARGB_2 then clears bit 1 (0x18, or 0x19
/// when argb1 is not used). LED_C is a single zone and needs no write.
pub fn header_enable_reports(zones: &[u8]) -> (Option<[u8; REPORT_LEN]>, Vec<(u8, [u8; REPORT_LEN])>) {
    let mut mask = 0x1Bu8;
    let mut paired = Vec::new();
    for &led in zones {
        if led == ARGB_1 {
            mask &= !0x01;
            paired.push((led, header_enable_report(mask)));
        } else if led == ARGB_2 {
            mask &= !0x02;
            paired.push((led, header_enable_report(mask)));
        }
    }
    if paired.is_empty() {
        (None, paired)
    } else {
        (Some(header_enable_report(0x1B)), paired)
    }
}

fn header_enable_report(mask: u8) -> [u8; REPORT_LEN] {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    buf[1] = 0x32;
    buf[2] = mask;
    buf
}

pub fn save_report() -> [u8; REPORT_LEN] {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    buf[1] = 0x5E;
    buf
}

pub fn info_select_report() -> [u8; REPORT_LEN] {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    buf[1] = 0x60;
    buf
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub product: String,
    pub fw: String,
    pub support_cmd_flag: u8,
    pub chip_id: u32,
}

pub fn parse_info(buf: &[u8]) -> Option<Info> {
    if buf.len() < 60 || buf[0] != REPORT_ID {
        return None;
    }
    let product = buf
        .get(12..40)?
        .split(|b| *b == 0)
        .next()
        .unwrap_or(&[])
        .iter()
        .map(|b| if b.is_ascii_graphic() || *b == b' ' { *b as char } else { '?' })
        .collect::<String>();
    let fw = format!("{}.{}.{}.{}", buf[4], buf[5], buf[6], buf[7]);
    let chip_id = u32::from_le_bytes(buf[56..60].try_into().ok()?);
    Some(Info {
        product,
        fw,
        support_cmd_flag: buf[11],
        chip_id,
    })
}

pub fn is_this_board(info: &Info) -> bool {
    info.chip_id == CHIP_ID && info.product.starts_with("IT5701")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_packet_layout() {
        let pkt = effect_report(LED_C, Mode::Static, 50, 4, Color { r: 0x28, g: 0, b: 0 });
        assert_eq!(pkt[0], 0xCC);
        assert_eq!(pkt[1], 0x24);
        assert_eq!(&pkt[2..6], &0x10u32.to_le_bytes());
        assert_eq!(pkt[11], 1);
        assert_eq!(pkt[12], 50);
        assert_eq!(&pkt[14..18], &[0x00, 0x00, 0x28, 0x00]);
    }

    #[test]
    fn pulse_clamps_brightness_and_sets_period() {
        let pkt = effect_report(ARGB_1, Mode::Pulse, 255, 4, Color { r: 1, g: 2, b: 3 });
        assert_eq!(pkt[12], 100);
        assert_eq!(u16::from_le_bytes([pkt[22], pkt[23]]), 800);
        assert_eq!(u16::from_le_bytes([pkt[26], pkt[27]]), 200);
    }

    #[test]
    fn flash_periods() {
        let flash = effect_report(ARGB_2, Mode::Flash, 10, 4, Color { r: 0, g: 0, b: 0 });
        assert_eq!(flash[11], 3);
        assert_eq!(u16::from_le_bytes([flash[26], flash[27]]), 1500);
    }

    #[test]
    fn reset_prefix_matches_capture() {
        let reset = reset_reports();
        assert_eq!(reset.len(), 10);
        for (i, pkt) in reset.iter().take(8).enumerate() {
            assert_eq!(pkt[0], 0xCC);
            assert_eq!(pkt[1], 0x20 + i as u8);
            assert!(pkt[2..].iter().all(|b| *b == 0));
        }
        assert_eq!(&reset[8][0..4], &[0xCC, 0x28, 0xFF, 0x00]);
        assert_eq!(&reset[9][0..3], &[0xCC, 0x31, 0x00]);
    }

    #[test]
    fn header_enable_sequence_matches_capture() {
        let (baseline, paired) = header_enable_reports(&[LED_C, ARGB_1, ARGB_2]);
        assert_eq!(baseline.unwrap()[2], 0x1B);
        let pairs: Vec<(u8, u8)> = paired.iter().map(|(led, p)| (*led, p[2])).collect();
        assert_eq!(pairs, vec![(ARGB_1, 0x1A), (ARGB_2, 0x18)]);
        let (baseline, paired) = header_enable_reports(&[LED_C]);
        assert!(baseline.is_none() && paired.is_empty());
        let (baseline, paired) = header_enable_reports(&[ARGB_2]);
        assert_eq!(baseline.unwrap()[2], 0x1B);
        assert_eq!(paired[0].1[2], 0x19);
    }

    #[test]
    fn apply_mask_is_the_three_chassis_zones() {
        let pkt = apply_report(&[LED_C, ARGB_1, ARGB_2]);
        assert_eq!(&pkt[0..6], &[0xCC, 0x28, 0x70, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn save_is_cc_5e() {
        let pkt = save_report();
        assert_eq!(pkt[0], 0xCC);
        assert_eq!(pkt[1], 0x5E);
        assert!(pkt[2..].iter().all(|b| *b == 0));
    }

    #[test]
    fn info_from_live_sample() {
        let raw = [
            0xCC, 0x01, 0x00, 0x01, 0x08, 0x00, 0x1B, 0x00, 0x00, 0x00, 0x00, 0x00, 0x49, 0x54, 0x35, 0x37,
            0x30, 0x31, 0x2D, 0x47, 0x49, 0x47, 0x41, 0x42, 0x59, 0x54, 0x45, 0x20, 0x56, 0x38, 0x2E, 0x30,
            0x2E, 0x32, 0x37, 0x2E, 0x30, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x02, 0x00, 0x01, 0x00,
            0x00, 0x01, 0x02, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00, 0x01, 0x01, 0x57, 0x00, 0x01, 0x02, 0x00,
        ];
        let info = parse_info(&raw).unwrap();
        assert_eq!(info.fw, "8.0.27.0");
        assert_eq!(info.support_cmd_flag, 0);
        assert_eq!(info.chip_id, CHIP_ID);
        assert!(is_this_board(&info));
        assert!(info.product.starts_with("IT5701-GIGABYTE"));
    }
}
