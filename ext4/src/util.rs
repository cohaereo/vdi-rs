use positioned_io2::ReadAt;

pub trait ReadAtExt {
    fn read_pod_owned<T: bytemuck::Pod>(&self, offset: u64) -> std::io::Result<T>;
    fn read_pod_vec<T: bytemuck::Pod>(&self, offset: u64, count: usize) -> std::io::Result<Vec<T>>;
}

impl<R: ReadAt> ReadAtExt for R {
    fn read_pod_owned<T: bytemuck::Pod>(&self, offset: u64) -> std::io::Result<T> {
        let mut buf = vec![0u8; std::mem::size_of::<T>()];
        self.read_exact_at(offset, &mut buf)?;
        Ok(bytemuck::try_from_bytes::<T>(&buf)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bytemuck error"))?
            .to_owned())
    }

    fn read_pod_vec<T: bytemuck::Pod>(&self, offset: u64, count: usize) -> std::io::Result<Vec<T>> {
        let mut buf = vec![0u8; std::mem::size_of::<T>() * count];
        self.read_exact_at(offset, &mut buf)?;
        Ok(bytemuck::try_cast_slice::<u8, T>(&buf)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bytemuck error"))?
            .to_owned())
    }
}
