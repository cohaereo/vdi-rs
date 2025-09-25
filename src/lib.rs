use positioned_io2::ReadAt;
use std::{
    io::{Read, Seek, Write},
    sync::Mutex,
};
use util::ReaderExt;

use crate::header::VdiHeader;

pub mod header;
pub mod slice;
mod util;

#[derive(Debug)]
pub struct VdiDisk<R: Read + Seek> {
    pub header: header::VdiHeader,
    pub block_size: usize,
    /// Absolute file offsets of each block relative to the start of the vdi file
    pub block_offsets: Vec<Option<u64>>,

    reader: Mutex<R>,
    position: u64,
}

impl<R: Read + Seek> VdiDisk<R> {
    pub fn open(mut reader: R) -> anyhow::Result<Self> {
        let header = reader.read_pod::<header::VdiHeader>()?;
        anyhow::ensure!(
            header.version == VdiHeader::VERSION,
            "Unsupported VDI version"
        );
        anyhow::ensure!(
            header.signature == VdiHeader::SIGNATURE,
            "Invalid VDI signature"
        );
        anyhow::ensure!(
            header.image_type == 1,
            "Only dynamic VDI images are supported"
        );

        let mut block_offsets = vec![None; header.blocks_in_image as usize];
        reader.seek(std::io::SeekFrom::Start(header.block_offsets_offset as u64))?;
        for offset in &mut block_offsets {
            let loc = reader.read_pod::<u32>()?;
            if loc == u32::MAX {
                *offset = None;
            } else {
                *offset = Some(header.data_offset as u64 + loc as u64 * header.block_size as u64);
            }
        }

        Ok(Self {
            header,
            block_size: header.block_size as usize,
            block_offsets,
            reader: Mutex::new(reader),
            position: 0,
        })
    }

    pub fn slice(&mut self, range: std::ops::Range<u64>) -> slice::Slice<'_, Self> {
        slice::Slice::new(self, range)
    }

    pub fn slice_owned(
        self,
        range: std::ops::Range<u64>,
    ) -> std::io::Result<slice::OwnedSlice<Self>> {
        slice::OwnedSlice::new(self, range)
    }
}

impl<R: Read + Seek> positioned_io2::ReadAt for VdiDisk<R> {
    fn read_at(&self, mut pos: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut total_read = 0;
        while total_read < buf.len() {
            let block_index = (pos / self.block_size as u64) as usize;
            let block_offset = (pos % self.block_size as u64) as usize;
            if block_index >= self.block_offsets.len() {
                break; // EOF
            }

            let to_read = std::cmp::min(buf.len() - total_read, self.block_size - block_offset);

            if let Some(file_offset) = self.block_offsets[block_index] {
                let mut reader = self.reader.lock().unwrap();
                reader.seek(std::io::SeekFrom::Start(file_offset + block_offset as u64))?;
                let n = reader.read(&mut buf[total_read..total_read + to_read])?;
                if n == 0 {
                    break; // EOF
                }
                total_read += n;
                pos += n as u64;
            } else {
                // Unallocated block
                buf[total_read..total_read + to_read].fill(0);
                total_read += to_read;
                pos += to_read as u64;
            }
        }
        Ok(total_read)
    }
}

impl<R: Read + Seek> Read for VdiDisk<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.read_at(self.position, buf)?;
        self.position += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> Write for VdiDisk<R> {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "VdiDisk does not support write operations",
        ))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<R: Read + Seek> std::io::Seek for VdiDisk<R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            std::io::SeekFrom::Start(offset) => offset,
            std::io::SeekFrom::End(offset) => {
                let end = self.header.disk_size;
                if offset >= 0 {
                    end.checked_add(offset as u64).ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Tried to seek beyond end of the disk",
                        )
                    })?
                } else {
                    end.checked_sub((-offset) as u64).ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Tried to seek beyond end of the disk",
                        )
                    })?
                }
            }
            std::io::SeekFrom::Current(offset) => {
                if offset >= 0 {
                    self.position.checked_add(offset as u64).ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Tried to seek beyond end of the disk",
                        )
                    })?
                } else {
                    self.position.checked_sub((-offset) as u64).ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Tried to seek beyond end of the disk",
                        )
                    })?
                }
            }
        };

        if new_pos > self.header.disk_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Tried to seek beyond end of the disk",
            ));
        }

        self.position = new_pos;
        Ok(self.position)
    }
}
