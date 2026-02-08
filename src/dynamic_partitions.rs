use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::os::unix::fs::PermissionsExt;

use crate::recovery::RecoveryUI;
use crate::func::{package_extract_file, exec_binary, get_active_slot_suffix}; 

#[derive(Debug, Default)]
struct SuperInfo {
    metadata_size: u64,
    metadata_slots: u32,
    size: u64,
    first_sector: u64,
    is_virtual_ab: bool,
}

#[derive(Clone)]
struct OpList {
    groups: HashMap<String, String>,
    partitions: HashMap<String, String>,
    sizes: HashMap<String, String>,
    remove_all_groups: bool,
    auto_detect_active_slot: bool,
}

pub fn update_dynamic_partitions(
    ui: &mut RecoveryUI,
    archive: &mut zip::ZipArchive<File>,
    op_list_file: &str
) -> Result<()> {
    extract_tools(archive)?;

    let op_list_path = "/tmp/op_list";
    package_extract_file(archive, op_list_file, op_list_path)?;
    let default_ops = parse_op_list(op_list_path)?;

    let ops = if default_ops.auto_detect_active_slot {
        ui.ui_print("Auto-detecting active slot...")?;
        process_auto_op_list(ui, default_ops)?
    } else {
        default_ops
    };

    if !ops.remove_all_groups && ops.groups.is_empty() {
        incremental_update(ui, &ops)?;
    } else {
        let info = parse_lpdump()?;
        full_flash(ui, &info, &ops)?;
        let tool_path = "/tmp/lptools/lptools";
        for (part, _size) in &ops.sizes {
            let _ = exec_binary(ui, tool_path, &["map", part]);
        }
    }

    Ok(())
}

fn incremental_update(ui: &mut RecoveryUI, ops: &OpList) -> Result<()> {
    let tool = "/tmp/lptools/lptools";
    
    for (part, size) in &ops.sizes {
        let _ = exec_binary(ui, tool, &["unmap", part]);
        let _ = exec_binary(ui, tool, &["remove", part]);
        let _ = exec_binary(ui, tool, &["create", part, size]);
        let _ = Command::new(tool)
            .args(&["map", part])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    
    Ok(())
}

fn full_flash(ui: &mut RecoveryUI, info: &SuperInfo, ops: &OpList) -> Result<()> {
    let lptools_bin = "/tmp/lptools/lptools";
    let lpmake_bin = "/tmp/lptools/lpmake";

    if ops.remove_all_groups {
        let _ = exec_binary(ui, lptools_bin, &["clear-cow"]);

        if let Ok(entries) = fs::read_dir("/dev/block/mapper") {
            for entry in entries {
                if let Ok(entry) = entry {
                    if let Ok(ft) = entry.file_type() {
                        if !ft.is_dir() {
                            let path = entry.path();
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                 if name == "control" { continue; }
                                 if name == "userdata" { continue; }
                                 if name == "metadata" { continue; }
                                 let _ = exec_binary(ui, lptools_bin, &["unmap", name]);
                                 let _ = exec_binary(ui, lptools_bin, &["remove", name]);
                            }
                        }
                    }
                }
            }
        }
    }

    let placeholder = "/tmp/placeholder.img";
    if !Path::new(placeholder).exists() {
        Command::new("dd")
            .args(&["if=/dev/zero", &format!("of={}", placeholder), "bs=1M", "count=1"])
            .status()?;
    }

    let mut args = Vec::new();
    args.push(format!("--metadata-size={}", info.metadata_size));
    args.push("--super-name=super".to_string());
    args.push(format!("--metadata-slots={}", info.metadata_slots));
    args.push(format!("--device=super:{}:{}", info.size, info.first_sector));
    
    if info.is_virtual_ab {
        args.push("--virtual-ab".to_string());
    }

    let resolve_size = |s: &str| -> String {
        if s == "auto" { info.size.to_string() } else { s.to_string() }
    };

    if info.metadata_slots == 1 || info.metadata_slots == 2 {
        for (g_name, g_size) in &ops.groups {
            let size = resolve_size(g_size);
            args.push(format!("--group={}:{}", g_name, size));
        }
        for (p_name, _p_group) in &ops.partitions {
            if let Some(size) = ops.sizes.get(p_name) {
                 let group = ops.partitions.get(p_name).unwrap();
                 args.push(format!("--partition={}:none:{}:{}", p_name, size, group));
                 args.push(format!("--image={}={}", p_name, placeholder));
            }
        }

    } else if info.metadata_slots == 3 {
        let mut slot = "_a";
        let mut empty_slot = "_b";
        
        for key in ops.sizes.keys() {
            if key.contains("_b") {
                slot = "_b";
                empty_slot = "_a";
                break;
            }
        }

        let mut group_tbl_active = String::new();
        let mut group_tbl_inactive = String::new();

        for g_name in ops.groups.keys() {
            if g_name.contains(slot) {
                group_tbl_active = g_name.clone();
            } else {
                group_tbl_inactive = g_name.clone();
            }
        }

        if let Some(g_size) = ops.groups.get(&group_tbl_active) {
            let size = resolve_size(g_size);
            args.push(format!("--group={}:{}", group_tbl_active, size));
        }
        if !group_tbl_inactive.is_empty() {
             if let Some(g_size) = ops.groups.get(&group_tbl_inactive) {
                let size = resolve_size(g_size);
                args.push(format!("--group={}:{}", group_tbl_inactive, size));
            }
        }

        for (p_name, group) in &ops.partitions {
            if p_name.contains(slot) {
                 if let Some(size) = ops.sizes.get(p_name) {
                     args.push(format!("--partition={}:none:{}:{}", p_name, size, group));
                     args.push(format!("--image={}={}", p_name, placeholder));
                 }
            }
        }

        if !group_tbl_inactive.is_empty() {
            for (p_name, group) in &ops.partitions {
                 if p_name.contains(empty_slot) {
                      args.push(format!("--partition={}:none:0:{}", p_name, group));
                 }
            }
        }

    } else {
        bail!("Meta slot count {} is not supported!", info.metadata_slots);
    }

    args.push("--output=/dev/block/by-name/super".to_string());
    
    let mut cmd = Command::new(lpmake_bin);
    for arg in args {
        cmd.arg(arg);
    }
    
    let status = cmd.status().context("Failed to run lpmake")?;
    if !status.success() {
        bail!("lpmake exec failed");
    }

    Ok(())
}

fn extract_tools(archive: &mut zip::ZipArchive<File>) -> Result<()> {
    let tools = vec!["lpdump", "lpmake", "lptools"];
    let base_out = "/tmp/lptools";
    fs::create_dir_all(base_out)?;

    for tool in tools {
        let zip_path = format!("META-INF/bin/lptools/{}", tool);
        let out_path = format!("{}/{}", base_out, tool);
        if !Path::new(&out_path).exists() {
            package_extract_file(archive, &zip_path, &out_path)?;
            let mut perms = fs::metadata(&out_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&out_path, perms)?;
        }
    }
    Ok(())
}

fn parse_lpdump() -> Result<SuperInfo> {
    let output = Command::new("/tmp/lptools/lpdump").output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let mut info = SuperInfo::default();
    
    let re_meta_size = Regex::new(r"Metadata max size:\s+(\d+)")?;
    let re_meta_slot = Regex::new(r"Metadata slot count:\s+(\d+)")?;
    let re_first_sec = Regex::new(r"First sector:\s+(\d+)")?;
    let re_size = Regex::new(r"Size:\s+(\d+)")?;
    let re_flags = Regex::new(r"Header flags:\s+(\w+)")?;

    if let Some(caps) = re_meta_size.captures(&stdout) { info.metadata_size = caps[1].parse()?; }
    if let Some(caps) = re_meta_slot.captures(&stdout) { info.metadata_slots = caps[1].parse()?; }
    if let Some(caps) = re_first_sec.captures(&stdout) { 
        let sector: u64 = caps[1].parse()?;
        info.first_sector = sector * 512; 
    }
    if let Some(caps) = re_size.captures(&stdout) { info.size = caps[1].parse()?; }
    if let Some(caps) = re_flags.captures(&stdout) {
        if &caps[1] == "virtual_ab_device" { info.is_virtual_ab = true; }
    }
    Ok(info)
}

fn parse_op_list(path: &str) -> Result<OpList> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut ops = OpList {
        groups: HashMap::new(),
        partitions: HashMap::new(),
        sizes: HashMap::new(),
        remove_all_groups: false,
        auto_detect_active_slot: false,
    };

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.split('#').next().unwrap_or("").trim(); 
        if trimmed.is_empty() { continue; }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        match parts[0] {
            "auto_detect_active_slot" => { ops.auto_detect_active_slot = true; },
            "add_group" => { ops.groups.insert(parts[1].to_string(), parts[2].to_string()); },
            "add" => { ops.partitions.insert(parts[1].to_string(), parts[2].to_string()); },
            "resize" => { ops.sizes.insert(parts[1].to_string(), parts[2].to_string()); },
            "remove_all_groups" => { ops.remove_all_groups = true; },
            _ => {}
        }
    }
    Ok(ops)
}

fn process_auto_op_list(ui: &mut RecoveryUI, raw: OpList) -> Result<OpList> {
    let suffix = get_active_slot_suffix()?;
    
    if suffix.is_empty() {
        ui.ui_print(" Device is A-only! the fuck are you doing using this arg...")?;
        return Ok(raw);
    }

    ui.ui_print(&format!("Active slot: {}", suffix))?;

    let active = suffix.as_str();

    let mut new_ops = raw.clone();
    
    new_ops.partitions.clear();
    
    for (part_base, group_base) in &raw.partitions {
        let p_active = format!("{}{}", part_base, active);
        let g_active = format!("{}{}", group_base, active);
        new_ops.partitions.insert(p_active, g_active);
    }

    new_ops.sizes.clear();

    for (part_base, size) in &raw.sizes {
        let p_active = format!("{}{}", part_base, active);
        new_ops.sizes.insert(p_active, size.clone());
    }

    Ok(new_ops)
}