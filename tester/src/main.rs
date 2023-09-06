use ectool::{Access, AccessHid, Ec, Error, SecurityState};
use hidapi::HidApi;
use proc_mounts::MountIter;
use std::{
    fs,
    path::{Path, PathBuf},
    process, str, thread, time,
};
use sysfs_class::{Block, SysClass};
use termion::{color, style};

const EXPECTED_BOARD: &str = "system76/thelio_io_2";
const EXPECTED_VERSION: &str = "0.21.0-65-g0c3e4c";
const EXPECTED_PWM: u8 = 127;
const MINIMUM_RPM: u16 = 300;
const MODULE: &str = "system76_thelio_io";

fn block_module() -> Result<(), String> {
    println!("Blocking module {}", MODULE);
    fs::write(
        "/etc/modprobe.d/thelio-io-tester.conf",
        format!("blacklist {}", MODULE),
    )
    .map_err(|err| format!("failed to block module {}: {:?}", MODULE, err))?;

    if Path::new("/sys/module").join(MODULE).exists() {
        println!("Removing module {}", MODULE);
        let status = process::Command::new("modprobe")
            .arg("--remove")
            .arg(MODULE)
            .status()
            .map_err(|err| format!("failed to run modprobe: {:?}", err))?;
        if !status.success() {
            return Err(format!("failed to remove module {}: {:?}", MODULE, status));
        }
    }

    Ok(())
}

fn allow_module() -> Result<(), String> {
    println!("Allowing module {}", MODULE);
    fs::remove_file("/etc/modprobe.d/thelio-io-tester.conf")
        .map_err(|err| format!("failed to allow module {}: {:?}", MODULE, err))
}

fn find_mount_by_dev(dev: &Path) -> Result<Option<PathBuf>, Error> {
    for info_res in MountIter::new()? {
        let info = info_res?;
        if info.source == dev {
            return Ok(Some(info.dest));
        }
    }
    Ok(None)
}

fn reset_bootloader() -> Result<(), String> {
    let mut ecs = Vec::new();
    let api = HidApi::new().map_err(|err| format!("failed to access HID API: {:?}", err))?;
    for info in api.device_list() {
        #[allow(clippy::single_match)]
        match (info.vendor_id(), info.product_id(), info.interface_number()) {
            // System76 thelio_io_2
            (0x3384, 0x000B, 1) => {
                let device = info
                    .open_device(&api)
                    .map_err(|err| format!("failed to open EC: {:?}", err))?;
                let access = AccessHid::new(device, 10, 100)
                    .map_err(|err| format!("failed to access EC: {:?}", err))?;
                ecs.push(unsafe {
                    Ec::new(access).map_err(|err| format!("failed to probe EC: {:?}", err))?
                });
            }
            _ => continue,
        }
    }

    let attempts = 60;
    for attempt in 1..=attempts {
        let mut unlocked = 0;
        for ec in &mut ecs {
            let security_state = unsafe {
                ec.security_get()
                    .map_err(|err| format!("failed to get EC security state: {:?}", err))?
            };

            match security_state {
                SecurityState::Lock | SecurityState::PrepareLock => {
                    unsafe {
                        ec.security_set(SecurityState::PrepareUnlock)
                            .map_err(|err| format!("failed to prepare to unlock EC: {:?}", err))?
                    };
                }
                SecurityState::Unlock => {
                    unlocked += 1;
                }
                SecurityState::PrepareUnlock => (),
            }
        }

        if unlocked == ecs.len() {
            break;
        }

        println!(
            "{}PRESS POWER BUTTON TO UNLOCK{} ({}/{})",
            style::Bold,
            style::Reset,
            attempt,
            attempts
        );
        thread::sleep(time::Duration::new(1, 0));
    }

    for ec in &mut ecs {
        println!("Resetting EC");
        unsafe {
            ec.reset()
                .map_err(|err| format!("failed to reset EC: {:?}", err))?
        };
    }

    Ok(())
}

fn flash_firmware() -> Result<(), String> {
    let mut bootloaders = Vec::new();
    let attempts = 30;
    for attempt in 1..=attempts {
        for block in
            Block::all().map_err(|err| format!("failed to discover block devices: {:?}", err))?
        {
            let parent = match block.parent_device() {
                Some(some) => some,
                None => continue,
            };

            let vendor = match parent.device_vendor() {
                Ok(ok) => ok.trim().to_string(),
                Err(_) => continue,
            };

            let model = match parent.device_model() {
                Ok(ok) => ok.trim().to_string(),
                Err(_) => continue,
            };

            let dev = match (vendor.as_str(), model.as_str()) {
                ("RPI", "RP2") => Path::new("/dev").join(block.id()),
                _ => continue,
            };

            bootloaders.push(dev);
        }

        if bootloaders.is_empty() {
            println!(
                "Waiting for RP2040 to reset to bootloader ({}/{})",
                attempt, attempts
            );
            thread::sleep(time::Duration::new(1, 0));
        } else {
            break;
        }
    }

    if bootloaders.len() != 1 {
        return Err(format!(
            "found {} bootloaders, expected 1",
            bootloaders.len()
        ));
    }

    for dev in bootloaders {
        println!("Found RP2040 bootloader at {}", dev.display());

        thread::sleep(time::Duration::new(1, 0)); //hack to ensure the device is mounted
        let mut mount_opt = find_mount_by_dev(&dev).map_err(|err| {
            format!(
                "failed to find mount point for {}: {:?}",
                dev.display(),
                err
            )
        })?;
        if mount_opt.is_none() {
            let status = process::Command::new("udisksctl")
                .arg("mount")
                .arg("--block-device")
                .arg(&dev)
                .status()
                .map_err(|err| format!("failed to run udisksctl: {:?}", err))?;

            if !status.success() {
                return Err(format!("failed to mount {}: {:?}", dev.display(), status));
            }

            mount_opt = find_mount_by_dev(&dev).map_err(|err| {
                format!(
                    "failed to find mount point for {}: {:?}",
                    dev.display(),
                    err
                )
            })?;
        }

        let mount = match mount_opt {
            Some(some) => some,
            //TODO: error?
            None => continue,
        };

        println!("Writing firmware to {}", mount.display());
        fs::write(
            mount.join("firmware.uf2"),
            include_bytes!("../res/firmware.uf2"),
        )
        .map_err(|err| format!("failed to write firmware: {:?}", err))?;
    }

    Ok(())
}

fn tester() -> Result<(), String> {
    reset_bootloader()?;

    flash_firmware()?;

    let mut ecs = Vec::new();
    let attempts = 30;
    for attempt in 1..=attempts {
        let api = HidApi::new().map_err(|err| format!("failed to access HID API: {:?}", err))?;
        for info in api.device_list() {
            #[allow(clippy::single_match)]
            match (info.vendor_id(), info.product_id(), info.interface_number()) {
                // System76 thelio_io_2
                (0x3384, 0x000B, 1) => {
                    let device = info
                        .open_device(&api)
                        .map_err(|err| format!("failed to open EC: {:?}", err))?;
                    let access = AccessHid::new(device, 10, 100)
                        .map_err(|err| format!("failed to access EC: {:?}", err))?;
                    ecs.push(unsafe {
                        Ec::new(access).map_err(|err| format!("failed to probe EC: {:?}", err))?
                    });
                }
                _ => continue,
            }
        }

        if ecs.is_empty() {
            println!(
                "Waiting for RP2040 to reset to runtime ({}/{})",
                attempt, attempts
            );
            thread::sleep(time::Duration::new(1, 0))
        } else {
            break;
        }
    }

    if ecs.len() != 1 {
        return Err(format!("found {} ECs, expected 1", ecs.len()));
    }

    for mut ec in ecs {
        let data_size = unsafe { ec.access().data_size() };

        let board = {
            let mut data = vec![0; data_size];
            let size = unsafe {
                ec.board(&mut data)
                    .map_err(|err| format!("failed to read board: {:?}", err))?
            };
            data.truncate(size);
            String::from_utf8(data).map_err(|err| format!("failed to parse board: {:?}", err))?
        };

        if board != EXPECTED_BOARD {
            return Err(format!(
                "found board {:?}, expected {:?}",
                board, EXPECTED_BOARD
            ));
        }

        let version = {
            let mut data = vec![0; data_size];
            let size = unsafe {
                ec.version(&mut data)
                    .map_err(|err| format!("failed to read version: {:?}", err))?
            };
            data.truncate(size);
            String::from_utf8(data).map_err(|err| format!("failed to parse version: {:?}", err))?
        };

        if version != EXPECTED_VERSION {
            return Err(format!(
                "found version {:?}, expected {:?}",
                version, EXPECTED_VERSION
            ));
        }

        println!(
            "EC has expected firmware with board {:?} and version {:?}",
            board, version
        );

        for fan in 0..4 {
            println!("Testing fan {} with PWM {}", fan, EXPECTED_PWM);

            unsafe {
                ec.fan_set(fan, EXPECTED_PWM)
                    .map_err(|err| format!("failed to set fan {} PWM: {:?}", fan, err))?;
            };

            thread::sleep(time::Duration::new(1, 0));

            let pwm = unsafe {
                ec.fan_get(fan)
                    .map_err(|err| format!("failed to read fan {} PWM: {:?}", fan, err))?
            };
            if pwm != EXPECTED_PWM {
                return Err(format!(
                    "fan {} had PWM {}, expected {}",
                    fan, pwm, EXPECTED_PWM
                ));
            }

            let rpm = unsafe {
                ec.fan_tach(fan)
                    .map_err(|err| format!("failed to read fan {} RPM: {:?}", fan, err))?
            };

            println!("Fan {} running at {} RPM", fan, rpm);
            if rpm < MINIMUM_RPM {
                return Err(format!(
                    "fan {} had RPM {}, expected at least {}",
                    fan, rpm, MINIMUM_RPM
                ));
            }
        }
    }

    Ok(())
}

fn tester_with_blocked_module() -> Result<(), String> {
    block_module()?;

    let res = tester();

    allow_module()?;

    res
}

fn main() {
    match tester_with_blocked_module() {
        Ok(()) => {
            eprintln!(
                "{}{}PASS{}{}",
                style::Bold,
                color::Fg(color::Green),
                color::Fg(color::Reset),
                style::Reset,
            );
            process::exit(0);
        }
        Err(err) => {
            eprintln!(
                "{}{}FAIL: {}{}{}",
                style::Bold,
                color::Fg(color::Red),
                err,
                color::Fg(color::Reset),
                style::Reset,
            );
            process::exit(1);
        }
    }
}
