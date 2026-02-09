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

    pub fn show_progress(&mut self, fraction_str: &str, seconds_str: &str) -> Result<()> {
        let fraction: f32 = fraction_str.parse().unwrap_or(0.0);
        let seconds: i32 = seconds_str.parse().unwrap_or(0);
    
        writeln!(self.pipe, "progress {} {}", fraction, seconds)?;
        self.pipe.flush()?;
        
        Ok(())
    }
}