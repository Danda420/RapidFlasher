use std::fs::File;
use std::io::Write;
use std::os::unix::io::FromRawFd;
use anyhow::Result;

pub struct RecoveryUI {
    pipe: File,
}

impl RecoveryUI {
    pub unsafe fn new(fd_num: i32) -> Result<Self> {
        let pipe = unsafe { File::from_raw_fd(fd_num) };
        Ok(RecoveryUI { pipe })
    }

    pub fn ui_print(&mut self, message: &str) -> Result<()> {
        writeln!(self.pipe, "ui_print {}", message)?;
        writeln!(self.pipe, "ui_print")?;
        self.pipe.flush()?;
        Ok(())
    }

    pub fn show_progress(&mut self, val_str: &str) -> Result<()> {
        let val: f32 = val_str.parse().unwrap_or(0.0);
        let adjusted = val + 10.0;
        writeln!(self.pipe, "progress 1 {:.0}", adjusted)?;
        self.pipe.flush()?;
        Ok(())
    }
}