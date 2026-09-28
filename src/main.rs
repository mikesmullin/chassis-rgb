mod protocol;

use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use clap::Parser;
use hidapi::HidApi;

use protocol::{
    apply_report, effect_report, enable_headers_report, info_select_report, is_this_board,
    parse_info, save_report, zone_name, zones_from_name, Color, Mode, PID, REPORT_ID, REPORT_LEN,
    VID,
};

#[derive(Parser, Debug)]
#[command(
    name = "chassis-rgb",
    about = "Set TRX50 AERO D rev 1.2 chassis LEDs (IT5701, 048d:5702). Does not touch the GPU.",
    after_help = "\
Speed is 0 (fastest) through 9 (slowest), default 4. Brightness is 0-255; pulse is capped at 100.
--save sends one CC 5E flash commit. Persistence through POST is not yet confirmed.
Do not pass --save on every color change."
)]
struct Cli {
    /// static, pulse, flash, or wave
    #[arg(long, default_value = "static")]
    mode: String,

    /// RRGGBB, with or without a leading '#'
    #[arg(long, default_value = "280000")]
    color: String,

    /// 0-255. The home agent's previous OpenRGB calls used 50.
    #[arg(long, default_value_t = 50)]
    brightness: u8,

    /// 0 fastest, 9 slowest. Ignored for static.
    #[arg(long, default_value_t = 4)]
    speed: u8,

    /// all, led, argb1, or argb2
    #[arg(long, default_value = "all")]
    zone: String,

    /// Write the current effect to controller flash (one CC 5E).
    #[arg(long)]
    save: bool,

    /// Read the controller info report and exit.
    #[arg(long)]
    info: bool,

    /// Print the reports and do not open the device.
    #[arg(long)]
    dry_run: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<String, String> {
    if cli.info {
        if cli.dry_run {
            let pkt = info_select_report();
            eprintln!("info-select {}", hex(&pkt));
            return Ok("dry-run info".to_string());
        }
        let dev = open_device()?;
        let info = read_info(&dev)?;
        require_board(&info)?;
        return Ok(format!(
            "info product={} fw={} support_cmd_flag={} chip_id={:#010x}",
            info.product.replace(' ', "_"),
            info.fw,
            info.support_cmd_flag,
            info.chip_id
        ));
    }

    let mode = Mode::parse(&cli.mode).ok_or_else(|| {
        format!("unknown mode '{}'. use static, pulse, flash, or wave", cli.mode)
    })?;
    let color = Color::parse(&cli.color)
        .ok_or_else(|| format!("color '{}' is not a 6-digit hex RRGGBB", cli.color))?;
    if cli.speed > 9 {
        return Err("speed must be 0-9 (0 is fastest)".to_string());
    }
    let zones = zones_from_name(&cli.zone)
        .ok_or_else(|| format!("unknown zone '{}'. use all, led, argb1, or argb2", cli.zone))?;

    let mut reports = Vec::new();
    if zones.iter().any(|z| *z == protocol::ARGB_1 || *z == protocol::ARGB_2) {
        reports.push(enable_headers_report());
    }
    for &led in &zones {
        reports.push(effect_report(led, mode, cli.brightness, cli.speed, color));
    }
    reports.push(apply_report(&zones));
    if cli.save {
        reports.push(save_report());
    }

    if cli.dry_run {
        for pkt in &reports {
            eprintln!("{}", hex(pkt));
        }
    } else {
        let dev = open_device()?;
        let info = read_info(&dev)?;
        require_board(&info)?;
        for pkt in &reports {
            dev.send_feature_report(pkt)
                .map_err(|e| format!("SET_FEATURE failed: {e}"))?;
            thread::sleep(Duration::from_millis(20));
        }
    }

    let zone_list = zones
        .iter()
        .map(|z| zone_name(*z))
        .collect::<Vec<_>>()
        .join(",");
    let applied = protocol::effect_brightness(mode, cli.brightness);
    Ok(format!(
        "ok mode={} color={} brightness={} speed={} zones={} saved={} dry_run={}",
        mode.name(),
        color.hex(),
        applied,
        cli.speed,
        zone_list,
        cli.save,
        cli.dry_run
    ))
}

fn open_device() -> Result<hidapi::HidDevice, String> {
    let api = HidApi::new().map_err(|e| format!("hidapi init failed: {e}"))?;
    let mut matches = Vec::new();
    for dev in api.device_list() {
        if dev.vendor_id() == VID && dev.product_id() == PID {
            matches.push((dev.path().to_owned(), dev.interface_number()));
        }
    }
    if matches.is_empty() {
        return Err(
            "no 048d:5702 hidraw. If revm has the controller, detach that USB hostdev first."
                .to_string(),
        );
    }
    // Prefer interface 0 if several nodes are listed.
    matches.sort_by_key(|(_, iface)| if *iface == 0 { 0 } else { 1 });
    api.open_path(&matches[0].0)
        .map_err(|e| format!("open 048d:5702 failed: {e}"))
}

fn read_info(dev: &hidapi::HidDevice) -> Result<protocol::Info, String> {
    let select = info_select_report();
    dev.send_feature_report(&select)
        .map_err(|e| format!("info select failed: {e}"))?;
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    let n = dev
        .get_feature_report(&mut buf)
        .map_err(|e| format!("info read failed: {e}"))?;
    if n < 60 {
        return Err(format!("info report too short ({n} bytes)"));
    }
    parse_info(&buf).ok_or_else(|| "info report was not a CC report".to_string())
}

fn require_board(info: &protocol::Info) -> Result<(), String> {
    if is_this_board(info) {
        return Ok(());
    }
    Err(format!(
        "refusing device product='{}' chip_id={:#010x}; this tool only drives the TRX50 AERO D IT5701",
        info.product, info.chip_id
    ))
}

fn hex(buf: &[u8]) -> String {
    buf.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}
