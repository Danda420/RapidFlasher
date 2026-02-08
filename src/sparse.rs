use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write, ErrorKind};
use byteorder::{LittleEndian, ReadBytesExt}; 
use anyhow::{Context, Result, bail};
use zip::ZipArchive;

const SPARSE_HEADER_MAGIC: u32 = 0xed26ff3a;
const CHUNK_TYPE_RAW: u16 = 0xCAC1;
const CHUNK_TYPE_FILL: u16 = 0xCAC2;
const CHUNK_TYPE_DONT_CARE: u16 = 0xCAC3;
const CHUNK_TYPE_CRC32: u16 = 0xCAC4;

pub fn flash_sparse(
    ui: &mut crate::recovery::RecoveryUI,
    archive: &mut ZipArchive<File>,
    args: &[String]
) -> Result<()> {
    let raw_name = &args[1];

    let (zip_base, start, end, partition_path, is_range) = if raw_name.ends_with(".*") {
        let base = &raw_name[..raw_name.len() - 2];
        let partition = args.get(2).context("Missing partition arg")?;

        ui.ui_print(&format!("Auto-detecting '{}' chunks...", base))?;

        let mut min_idx = usize::MAX;
        let mut max_idx = 0;
        let mut found_any = false;

        let num_files = archive.len();
        for i in 0..num_files {
            let file = archive.by_index(i)?;
            let name = file.name();
            if name.starts_with(base) {
                if let Some(suffix) = name.strip_prefix(base) {
                    if suffix.starts_with('.') {
                        if let Ok(idx) = suffix[1..].parse::<usize>() {
                            if idx < min_idx { min_idx = idx; }
                            if idx > max_idx { max_idx = idx; }
                            found_any = true;
                        }
                    }
                }
            }
        }

        if !found_any { bail!("No chunks found for {}.*", base); }
        ui.ui_print(&format!("  Found chunks {} to {}", min_idx, max_idx))?;
        (base.to_string(), min_idx, max_idx, partition.to_string(), true)

    } else {
        let arg2 = args.get(2).context("Missing arg2")?;
        if arg2.chars().all(|c| c.is_numeric()) {
            let start = arg2.parse()?;
            let end = args.get(3).context("Missing end")?.parse()?;
            let partition = args.get(4).context("Missing partition")?;
            (raw_name.to_string(), start, end, partition.to_string(), true)
        } else {
            (raw_name.to_string(), 0, 0, arg2.to_string(), false)
        }
    };

    let device_file = OpenOptions::new()
        .read(true) 
        .write(true)
        .open(&partition_path)
        .context(format!("Failed to open partition {}", partition_path))?;
        
    let mut writer = BufWriter::with_capacity(16 * 1024 * 1024, device_file);

    if is_range {
        for i in start..=end {
            let entry_name = format!("{}.{}", zip_base, i);
            ui.ui_print(&format!("  - Processing {}...", entry_name))?;

            writer.flush()?;
            writer.seek(SeekFrom::Start(0))?;

            let mut entry = archive.by_name(&entry_name).context("Chunk not found")?;
            write_sparse(&mut entry, &mut writer)?;
        }

    } else {
        let mut entry = archive.by_name(&zip_base)?;
        write_sparse(&mut entry, &mut writer)?;
    }
    
    writer.flush()?;
    
    Ok(())
}

fn write_sparse<R: Read, W: Write + Seek>(reader: &mut R, writer: &mut W) -> Result<()> {
    let magic = reader.read_u32::<LittleEndian>()?;
    if magic != SPARSE_HEADER_MAGIC { bail!("Invalid sparse magic: {:x}", magic); }

    let _major = reader.read_u16::<LittleEndian>()?;
    let _minor = reader.read_u16::<LittleEndian>()?;
    let file_hdr_sz = reader.read_u16::<LittleEndian>()?;
    let chunk_hdr_sz = reader.read_u16::<LittleEndian>()?;
    let blk_sz = reader.read_u32::<LittleEndian>()?;
    let _total_blks = reader.read_u32::<LittleEndian>()?;
    let total_chunks = reader.read_u32::<LittleEndian>()?;
    let _crc = reader.read_u32::<LittleEndian>()?;

    let bytes_read_so_far = 28; 
    if file_hdr_sz > bytes_read_so_far {
        io::copy(&mut reader.take((file_hdr_sz - bytes_read_so_far) as u64), &mut io::sink())?;
    }

    let zero_buf = vec![0u8; 1024 * 1024];

    for _ in 0..total_chunks {
        let chunk_type = reader.read_u16::<LittleEndian>()?;
        let _reserved = reader.read_u16::<LittleEndian>()?;
        let chunk_sz = reader.read_u32::<LittleEndian>()?; 
        let total_sz = reader.read_u32::<LittleEndian>()?; 
        let data_sz = (total_sz as u64) - (chunk_hdr_sz as u64);
        let output_sz = (chunk_sz as u64) * (blk_sz as u64);

        match chunk_type {
            CHUNK_TYPE_RAW => { io::copy(&mut reader.take(output_sz), writer)?; },
            CHUNK_TYPE_FILL => {
                let fill_val = reader.read_u32::<LittleEndian>()?;
                if fill_val == 0 {
                    seek_or_write(writer, output_sz, &zero_buf)?;
                } else {
                    let mut fill_block = vec![0u8; blk_sz as usize];
                    let fb = fill_val.to_le_bytes();
                    for i in 0..(blk_sz as usize / 4) {
                        fill_block[i*4..i*4+4].copy_from_slice(&fb);
                    }
                    for _ in 0..chunk_sz { writer.write_all(&fill_block)?; }
                }
            },
            CHUNK_TYPE_DONT_CARE => { seek_or_write(writer, output_sz, &zero_buf)?; },
            CHUNK_TYPE_CRC32 => { io::copy(&mut reader.take(data_sz), &mut io::sink())?; },
            _ => { io::copy(&mut reader.take(data_sz), &mut io::sink())?; }
        }
    }
    Ok(())
}

fn seek_or_write<W: Write + Seek>(writer: &mut W, mut bytes: u64, buf: &[u8]) -> Result<()> {
    if writer.flush().is_ok() {
        if writer.seek(SeekFrom::Current(bytes as i64)).is_ok() { return Ok(()); }
    }
    while bytes > 0 {
        let to_write = std::cmp::min(bytes, buf.len() as u64) as usize;
        if let Err(e) = writer.write_all(&buf[..to_write]) {
             if e.kind() == ErrorKind::WriteZero || e.raw_os_error() == Some(28) { return Ok(()); }
             return Err(e.into());
        }
        if let Err(e) = writer.flush() {
            if e.kind() == ErrorKind::WriteZero || e.raw_os_error() == Some(28) { return Ok(()); }
            return Err(e.into());
        }
        bytes -= to_write as u64;
    }
    Ok(())
}