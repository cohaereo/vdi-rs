use std::io::{Read, Seek};

use positioned_io2::ReadAt;

use crate::VdiDisk;

pub struct Slice<'a> {
    inner: &'a VdiDisk,
    range: std::ops::Range<u64>,
    pos: u64,
}

impl<'a> Slice<'a> {
    pub fn new(inner: &'a VdiDisk, range: std::ops::Range<u64>) -> Self {
        Self {
            inner,
            range,
            pos: 0,
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        (self.range.end - self.range.start) as usize
    }
}

impl<'a> Read for Slice<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read_at(self.range.start + self.pos, buf)?;
        self.pos += read as u64;
        Ok(read)
    }
}

impl<'a> Seek for Slice<'a> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            std::io::SeekFrom::Start(offset) => offset,
            std::io::SeekFrom::End(offset) => {
                if offset >= 0 {
                    self.len() as u64 + offset as u64
                } else {
                    self.len() as u64 - (-offset) as u64
                }
            }
            std::io::SeekFrom::Current(offset) => {
                if offset >= 0 {
                    self.pos + offset as u64
                } else {
                    self.pos - (-offset) as u64
                }
            }
        };

        if new_pos > self.range.end {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek out of bounds",
            ));
        }

        self.pos = new_pos;
        Ok(self.pos)
    }
}

impl<'a> positioned_io2::ReadAt for Slice<'a> {
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        if pos >= self.len() as u64 {
            return Ok(0);
        }
        let read_len = std::cmp::min(buf.len() as u64, self.len() as u64 - pos);
        self.inner
            .read_at(self.range.start + pos, &mut buf[..read_len as usize])
    }
}

pub struct OwnedSlice {
    inner: VdiDisk,
    range: std::ops::Range<u64>,
    pos: u64,
}

impl OwnedSlice {
    pub fn new(inner: VdiDisk, range: std::ops::Range<u64>) -> std::io::Result<Self> {
        Ok(Self {
            inner,
            range,
            pos: 0,
        })
    }

    pub fn into_inner(self) -> VdiDisk {
        self.inner
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        (self.range.end - self.range.start) as usize
    }
}

impl Read for OwnedSlice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read_at(self.range.start + self.pos, buf)?;
        self.pos += read as u64;
        Ok(read)
    }
}

impl Seek for OwnedSlice {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            std::io::SeekFrom::Start(offset) => offset,
            std::io::SeekFrom::End(offset) => {
                if offset >= 0 {
                    self.len() as u64 + offset as u64
                } else {
                    self.len() as u64 - (-offset) as u64
                }
            }
            std::io::SeekFrom::Current(offset) => {
                if offset >= 0 {
                    self.pos + offset as u64
                } else {
                    self.pos - (-offset) as u64
                }
            }
        };

        if new_pos > self.range.end {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek out of bounds",
            ));
        }

        self.pos = new_pos;
        Ok(self.pos)
    }
}

impl positioned_io2::ReadAt for OwnedSlice {
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        if pos >= self.len() as u64 {
            return Ok(0);
        }
        let read_len = std::cmp::min(buf.len() as u64, self.len() as u64 - pos);
        self.inner
            .read_at(self.range.start + pos, &mut buf[..read_len as usize])
    }
}
