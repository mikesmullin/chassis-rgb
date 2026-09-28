//! ASUS ENE SMBus GPU backend (RTX 5090 ASTRAL OC WHITE on this machine).
//!
//! Register layout encoded from the observed ENE protocol, not copied from
//! OpenRGB. Like OpenRGB, unknown device version strings are refused.

use i2cdev::core::I2CDevice;
use i2cdev::linux::LinuxI2CDevice;

pub const SLAVE_ADDR: u16 = 0x67;

/// Seen live on this card (ASUS reuses this controller across ASTRAL
/// cards). Accepted only with V2 color registers.
pub const VERSION: &str = "AUMA0-E6K5-1113";

const REG_DEVICE_NAME: u16 = 0x1000;
const REG_CONFIG_TABLE: u16 = 0x1C00;
const REG_COLORS_EFFECT_V2: u16 = 0x8160;
const REG_DIRECT: u16 = 0x8020;
const REG_MODE: u16 = 0x8021;
const REG_SPEED: u16 = 0x8022;
const REG_DIRECTION: u16 = 0x8023;
const REG_APPLY: u16 = 0x80A0;

const APPLY_VAL: u8 = 0x01;
const SAVE_VAL: u8 = 0xAA;

pub const MODE_STATIC: u8 = 1;
pub const MODE_BREATHING: u8 = 2;
pub const MODE_FLASHING: u8 = 3;

fn swap(reg: u16) -> u16 {
    ((reg << 8) & 0xFF00) | ((reg >> 8) & 0x00FF)
}

pub struct Gpu {
    dev: LinuxI2CDevice,
    pub bus: String,
    pub version: String,
    pub led_count: u8,
}

impl Gpu {
    pub fn open() -> Result<Self, String> {
        let mut tried = 0;
        for bus in 0..32u8 {
            let path = format!("/dev/i2c-{bus}");
            if std::fs::metadata(&path).is_err() {
                continue;
            }
            tried += 1;
            let dev = match LinuxI2CDevice::new(&path, SLAVE_ADDR) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let mut gpu = Gpu {
                dev,
                bus: path,
                version: String::new(),
                led_count: 0,
            };
            match gpu.read_version() {
                Ok(v) if v == VERSION => {
                    gpu.version = v;
                    gpu.led_count = gpu.read_led_count()?;
                    if gpu.led_count == 0 || gpu.led_count > 90 {
                        return Err(format!(
                            "implausible GPU LED count {} on {}",
                            gpu.led_count, gpu.bus
                        ));
                    }
                    return Ok(gpu);
                }
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
        if tried == 0 {
            return Err("no /dev/i2c-* nodes".to_string());
        }
        Err(format!(
            "no ENE GPU at 0x{SLAVE_ADDR:02X} with version {VERSION}. Run --info to see what was found."
        ))
    }

    fn reg_write(&mut self, reg: u16, val: u8) -> Result<(), String> {
        self.dev
            .smbus_write_word_data(0x00, swap(reg))
            .map_err(|e| format!("select reg {reg:#06X}: {e}"))?;
        self.dev
            .smbus_write_byte_data(0x01, val)
            .map_err(|e| format!("write reg {reg:#06X}: {e}"))
    }

    fn reg_read(&mut self, reg: u16) -> Result<u8, String> {
        self.dev
            .smbus_write_word_data(0x00, swap(reg))
            .map_err(|e| format!("select reg {reg:#06X}: {e}"))?;
        self.dev
            .smbus_read_byte_data(0x81)
            .map_err(|e| format!("read reg {reg:#06X}: {e}"))
    }

    fn reg_write_block(&mut self, reg: u16, data: &[u8]) -> Result<(), String> {
        self.dev
            .smbus_write_word_data(0x00, swap(reg))
            .map_err(|e| format!("select reg {reg:#06X}: {e}"))?;
        if self.dev.smbus_write_block_data(0x03, data).is_err() {
            for b in data {
                self.dev
                    .smbus_write_byte_data(0x01, *b)
                    .map_err(|e| format!("write reg {reg:#06X}: {e}"))?;
            }
        }
        Ok(())
    }

    fn read_version(&mut self) -> Result<String, String> {
        let mut bytes = Vec::with_capacity(16);
        for i in 0..16 {
            bytes.push(self.reg_read(REG_DEVICE_NAME + i)?);
        }
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(16);
        String::from_utf8(bytes[..end].to_vec())
            .map_err(|_| "GPU version string is not UTF-8".to_string())
    }

    fn read_led_count(&mut self) -> Result<u8, String> {
        // Same offset official software uses for this GPU generation.
        self.reg_read(REG_CONFIG_TABLE + 3)
    }

    pub fn read_mode_regs(&mut self) -> Result<(u8, u8, u8, u8), String> {
        Ok((
            self.reg_read(REG_DIRECT)?,
            self.reg_read(REG_MODE)?,
            self.reg_read(REG_SPEED)?,
            self.reg_read(REG_DIRECTION)?,
        ))
    }

    /// brightness 0-255 scales the color, matching the chassis tool.
    /// speed 0 (fastest) through 9 maps onto the ENE 0-4 range.
    pub fn apply(
        &mut self,
        mode: u8,
        r: u8,
        g: u8,
        b: u8,
        brightness: u8,
        speed: u8,
        save: bool,
    ) -> Result<(), String> {
        let scale = |v: u8| ((u16::from(v) * u16::from(brightness)) / 255) as u8;
        let ene_speed = (u16::from(speed.min(9)) * 4 + 4) / 9;
        // Effect color order on the wire is R, B, G.
        let triple = [scale(r), scale(b), scale(g)];
        for led in 0..self.led_count {
            let reg = REG_COLORS_EFFECT_V2 + u16::from(led) * 3;
            self.reg_write_block(reg, &triple)?;
        }
        self.reg_write(REG_APPLY, APPLY_VAL)?;
        self.reg_write(REG_MODE, mode)?;
        self.reg_write(REG_SPEED, ene_speed as u8)?;
        self.reg_write(REG_DIRECTION, 0)?;
        self.reg_write(REG_APPLY, APPLY_VAL)?;
        self.reg_write(REG_DIRECT, 0)?;
        self.reg_write(REG_APPLY, APPLY_VAL)?;
        if save {
            self.reg_write(REG_APPLY, SAVE_VAL)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_map_covers_ene_range() {
        let map = |s: u8| (u16::from(s.min(9)) * 4 + 4) / 9;
        assert_eq!(map(0), 0);
        assert_eq!(map(4), 2);
        assert_eq!(map(9), 4);
    }

    #[test]
    fn brightness_scales_color() {
        let scale = |v: u8, b: u8| ((u16::from(v) * u16::from(b)) / 255) as u8;
        assert_eq!(scale(0x28, 50), 7);
        assert_eq!(scale(255, 255), 255);
        assert_eq!(scale(255, 0), 0);
    }
}
