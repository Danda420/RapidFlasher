use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use anyhow::{bail, Context, Result};
use zip::ZipArchive;
use std::collections::HashMap;
use crate::func::get_active_slot_suffix;

mod recovery;
mod func;
mod sparse;
mod dynamic_partitions;
mod threaded_writer;

use recovery::RecoveryUI;
use func::{verify_device, package_extract_file, package_extract_targz, package_flash_partition, set_slot};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        bail!("Usage: update-binary <api> <fd> <zipfile>");
    }

    let pipe_fd: i32 = args[2].parse().context("Invalid FD")?;
    let zip_path = &args[3];

    let mut ui = unsafe { RecoveryUI::new(pipe_fd)? };

    let file = File::open(zip_path).context("Failed to open zip")?;
    let mut archive = ZipArchive::new(file)?;

    let mut vars: HashMap<String, String> = HashMap::new();
    let slot_suffix = get_active_slot_suffix().unwrap_or_default();
    vars.insert("SLOT".to_string(), slot_suffix);

    let script_path = Path::new("/tmp/updater-script");
    {
        let mut script_entry = archive.by_name("META-INF/com/google/android/updater-script")
            .context("Could not find updater-script in ZIP")?;
        let mut out = File::create(script_path)?;
        io::copy(&mut script_entry, &mut out)?;
    }
    
    let script_file = File::open(script_path)?;
    let reader = BufReader::new(script_file);

    for line_result in reader.lines() {
        let line = line_result?;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts = match shell_words::split(trimmed) {
            Ok(p) => p,
            Err(_) => {
                ui.ui_print(&format!("Syntax error in line: {}", trimmed))?;
                continue;
            }
        };

        if parts.is_empty() { continue; }

        let cmd = &parts[0];
        let raw_args = &parts[1..];

        let args: Vec<String> = raw_args.iter().map(|arg| {
            let mut new_arg = arg.to_string();
            for (key, val) in &vars {
                new_arg = new_arg.replace(&format!("${{{}}}", key), val);
                new_arg = new_arg.replace(&format!("${}", key), val);
            }
            new_arg
        }).collect();

        match cmd.as_str() {
            "set" => {
                 if args.len() >= 2 {
                     vars.insert(args[0].clone(), args[1].clone());
                 }
            },
            "ui_print" => {
                let msg = args.get(0).cloned().unwrap_or_default();
                ui.ui_print(&msg)?;
            },
            "show_progress" => {
                let fraction = args.get(0).cloned().unwrap_or_else(|| "0.0".to_string());
                let seconds = args.get(1).cloned().unwrap_or_else(|| "0".to_string());
                ui.show_progress(&fraction, &seconds)?;
            },
            "verify_device" => {
                let devices = args.get(0).context("verify_device missing args")?;
                verify_device(&mut ui, devices)?;
            },
            "package_extract_file" => {
                if args.len() < 2 { continue; }
                package_extract_file(&mut archive, &args[0], &args[1])?;
            },
            "package_extract_targz" => {
                if args.len() < 2 { continue; }
                package_extract_targz(&mut archive, &args[0], &args[1])?;
            },
            "package_flash_partition" => {
                package_flash_partition(&mut ui, &mut archive, &args)?;
            },
            "update_dynamic_partitions" => {
                if args.is_empty() { continue; }
                let op_list_file = &args[0];
                
                match dynamic_partitions::update_dynamic_partitions(&mut ui, &mut archive, op_list_file) {
                    Ok(_) => {},
                    Err(e) => {
                        ui.ui_print(&format!("Error updating partitions: {:?}", e))?;
                        return Err(e);
                    }
                }
            },
            "disable_vbmeta" => {
                crate::func::disable_vbmeta(&mut ui, &mut archive)?;
            },
            "set_slot" => {
                let slot = args.get(0).cloned().unwrap_or_else(|| "0".to_string());
                set_slot(&mut ui, &slot)?;
            },
            "run_program" => {
                crate::func::run_program(&mut ui, &args)?;
            },
            _ => { }
        }
    }

    let _ = fs::remove_file(script_path);
    let _ = fs::remove_dir_all("/tmp/lptools"); 
    Ok(())
}