use std::{
    cell::RefCell,
    io::{Read, Seek},
};

pub struct Slice<'a, R> {
    inner: RefCell<&'a mut R>,
    range: std::ops::Range<u64>,
    pos: u64,
}

impl<'a, R: Read + Seek> Slice<'a, R> {
    pub fn new(inner: &'a mut R, range: std::ops::Range<u64>) -> Self {
        inner.seek(std::io::SeekFrom::Start(range.start)).unwrap();
        Self {
            inner: RefCell::new(inner),
            range,
            pos: 0,
        }
    }
}

impl<'a, R: Read + Seek> Read for Slice<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.borrow_mut().read(buf)?;
        self.pos += read as u64;
        Ok(read)
    }
}

impl<'a, R: Read + Seek> Seek for Slice<'a, R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            std::io::SeekFrom::Start(offset) => offset,
            std::io::SeekFrom::End(offset) => {
                if offset >= 0 {
                    self.range.end + offset as u64
                } else {
                    self.range.end - (-offset) as u64
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

        self.inner
            .borrow_mut()
            .seek(std::io::SeekFrom::Start(self.range.start + new_pos))?;
        self.pos = new_pos;
        Ok(self.pos)
    }
}

impl<'a, R: Read + Seek> positioned_io2::ReadAt for Slice<'a, R> {
    fn read_at(&self, pos: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        if pos >= self.range.end - self.range.start {
            return Ok(0);
        }
        let read_len = std::cmp::min(buf.len() as u64, self.range.end - self.range.start - pos);
        self.inner
            .borrow_mut()
            .seek(std::io::SeekFrom::Start(self.range.start + pos))?;
        self.inner.borrow_mut().read(&mut buf[..read_len as usize])
    }
}
