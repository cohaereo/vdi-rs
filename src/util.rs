pub trait ReaderExt {
    fn read_pod<T: bytemuck::Pod>(&mut self) -> anyhow::Result<T>;
}

impl<R: std::io::Read> ReaderExt for R {
    fn read_pod<T: bytemuck::Pod>(&mut self) -> anyhow::Result<T> {
        let mut buf = vec![0u8; std::mem::size_of::<T>()];
        self.read_exact(&mut buf)?;
        Ok(bytemuck::from_bytes::<T>(&buf).to_owned())
    }
}
