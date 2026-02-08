use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::sync::mpsc;
use std::thread;
use anyhow::Result;

pub struct ThreadedWriter {
    sender: Option<mpsc::SyncSender<Vec<u8>>>,
    handle: Option<thread::JoinHandle<Result<()>>>,
}

impl ThreadedWriter {
    pub fn new(path: String, buf_size: usize) -> Self {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(5);
        
        let handle = thread::spawn(move || {
            let file = File::create(path)?;
            let mut writer = BufWriter::with_capacity(buf_size, file);
            
            for chunk in rx {
                writer.write_all(&chunk)?;
            }
            writer.flush()?;
            Ok(())
        });

        ThreadedWriter {
            sender: Some(tx),
            handle: Some(handle),
        }
    }

    pub fn finish(mut self) -> Result<()> {
        drop(self.sender.take());
        if let Some(h) = self.handle.take() {
            h.join().unwrap()?;
        }
        Ok(())
    }
}

impl Write for ThreadedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let chunk = buf.to_vec();
        if let Some(tx) = &self.sender {
            tx.send(chunk).map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}