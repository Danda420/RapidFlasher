use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::process::Command;
use anyhow::{Context, Result};
use zip::ZipArchive;
use std::os::unix::fs::PermissionsExt;

use crate::recovery::RecoveryUI;
use crate::threaded_writer::ThreadedWriter;

const WRITE_BUFFER_SIZE: usize = 4 * 1024 * 1024;

pub fn verify_device(ui: &mut RecoveryUI, allowed_devices: &str) -> Result<()> {
    let output = Command::new("getprop")
        .arg("ro.product.device")
        .output()?;
    let current_device = String::from_utf8(output.stdout)?.trim().to_string();
    
    let allowed: Vec<&str> = allowed_devices.split(',').map(|s| s.trim()).collect();

    let mut valid = false;
    for &d in &allowed {
        if current_device == d || current_device.contains(d) {
            valid = true;
            break;
        }
    }

    if !valid {
         let output_build = Command::new("getprop").arg("ro.build.product").output()?;
         let current_product = String::from_utf8(output_build.stdout)?.trim().to_string();
         for &d in &allowed {
            if current_product == d || current_product.contains(d) {
                valid = true;
                break;
            }
        }
    }
    
    if !valid {
        ui.ui_print("This ROM is not compatible for your device! aborting...")?;
        std::process::exit(1);
    }

    Ok(())
}

pub fn exec_binary(ui: &mut RecoveryUI, binary: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(binary)
        .args(args)
        .status()
        .context(format!("failed to exec {}", binary))?;
        
    if !status.success() {
        ui.ui_print(&format!("cmd failed: {} {:?}", binary, args))?;
    }
    Ok(())
}

pub fn exec_capture(binary: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(binary)
        .args(args)
        .output()
        .context(format!("Failed to exec {}", binary))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stdout)
}

pub fn package_extract_file(
    archive: &mut ZipArchive<File>, 
    zip_path: &str, 
    dest_path: &str
) -> Result<()> {
    let mut entry = archive.by_name(zip_path).context("File not found in zip")?;
    
    if let Some(parent) = Path::new(dest_path).parent() {
        fs::create_dir_all(parent)?;
    }

    let mut writer = ThreadedWriter::new(dest_path.to_string(), WRITE_BUFFER_SIZE);
    io::copy(&mut entry, &mut writer)?;
    writer.finish()?;
    Ok(())
}

pub fn package_extract_targz(
    archive: &mut ZipArchive<File>,
    zip_path: &str,
    dest_dir: &str
) -> Result<()> {
    let entry = archive.by_name(zip_path)?;
    let buf_reader = io::BufReader::with_capacity(WRITE_BUFFER_SIZE, entry);
    let tar_stream = flate2::read::GzDecoder::new(buf_reader);
    let mut tar_archive = tar::Archive::new(tar_stream);
    fs::create_dir_all(dest_dir)?;
    tar_archive.unpack(dest_dir)?;
    Ok(())
}

pub fn package_flash_partition(
    ui: &mut RecoveryUI,
    archive: &mut ZipArchive<File>,
    args: &[String]
) -> Result<()> {
    if args.is_empty() { return Ok(()); }
    
    let method = &args[0];
    let zip_entry = args.get(1).context("Missing zip entry arg")?;
    match method.as_str() {
        "0" => {
            let dest_path = args.get(2).context("Missing destination arg")?;
            let mut source = archive.by_name(zip_entry)?;
            let mut writer = ThreadedWriter::new(dest_path.to_string(), WRITE_BUFFER_SIZE);
            zstd::stream::copy_decode(&mut source, &mut writer)?;
            writer.finish()?;
        },
        "1" => {
            let dest_path = args.get(2).context("Missing destination arg")?;
            let source = archive.by_name(zip_entry)?;
            let mut writer = ThreadedWriter::new(dest_path.to_string(), WRITE_BUFFER_SIZE);
            let mut decoder = flate2::read::GzDecoder::new(source);
            io::copy(&mut decoder, &mut writer)?;
            writer.finish()?;
        },
        "2" => {
            crate::sparse::flash_sparse(ui, archive, args)?;
        },
        _ => {
            ui.ui_print(&format!("Unknown flash method: {}", method))?;
        }
    }
    Ok(())
}

pub fn disable_vbmeta(ui: &mut RecoveryUI, archive: &mut ZipArchive<File>) -> Result<()> {
    let mut bin_path = "/system/bin/avbctl".to_string();

    if !Path::new(&bin_path).exists() {
        if Path::new("/sbin/avbctl").exists() {
             bin_path = "/sbin/avbctl".to_string();
        } else {
            ui.ui_print("avbctl binary not found. Extracting from zip...")?;
            
            let tmp_path = "/tmp/avbctl";
            package_extract_file(archive, "META-INF/bin/avbctl", tmp_path)?;
            let mut perms = fs::metadata(tmp_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(tmp_path, perms)?;

            bin_path = tmp_path.to_string();
        }
    }

    ui.ui_print("Checking avb vbmeta status")?;

    let verity = exec_capture("avbctl", &["get-verity"])?;
    let verification = exec_capture("avbctl", &["get-verification"])?;

    ui.ui_print(&format!("- {}", verity))?;
    ui.ui_print(&format!("- {}", verification))?;

    std::thread::sleep(std::time::Duration::from_secs(1));
    ui.ui_print(" ")?;

    if verity.to_lowercase().contains("disabled") {
        ui.ui_print("avb vbmeta already disabled, no need to continue")?;
    } else {
        ui.ui_print("Disabling avb vbmeta")?;

        let dis_verity = exec_capture(&bin_path, &["--force", "disable-verity"])?;
        let dis_verification = exec_capture(&bin_path, &["--force", "disable-verification"])?;

        ui.ui_print(&format!("- {}", dis_verity))?;
        ui.ui_print(&format!("- {}", dis_verification))?;

        std::thread::sleep(std::time::Duration::from_secs(1));
        ui.ui_print(" ")?;
    }

    Ok(())
}

pub fn get_active_slot_suffix() -> Result<String> {
    let output = Command::new("getprop")
        .arg("ro.boot.slot_suffix")
        .output()?;
    let mut suffix = String::from_utf8(output.stdout)?.trim().to_string();

    if suffix.is_empty() {
         let output2 = Command::new("getprop").arg("ro.boot.slot").output()?;
         let s = String::from_utf8(output2.stdout)?.trim().to_string();
         if !s.is_empty() {
             suffix = format!("_{}", s);
         }
    }
    
    Ok(suffix)
}

pub fn set_slot(ui: &mut RecoveryUI, slot_mode: &str) -> Result<()> {
    let status = Command::new("bootctl")
        .arg("set-active-boot-slot")
        .arg(slot_mode)
        .status();
    
    match status {
        Ok(s) => if !s.success() { ui.ui_print("Warning: bootctl returned error.")?; },
        Err(_) => { ui.ui_print("Warning: bootctl binary not found.")?; }
    }
    Ok(())
}