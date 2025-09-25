use positioned_io2::ReadAt;

pub trait ReaderExt {
    fn read_pod_at<T: bytemuck::Pod>(&mut self, offset: u64) -> anyhow::Result<T>;
}

impl<R: ReadAt> ReaderExt for R {
    fn read_pod_at<T: bytemuck::Pod>(&mut self, offset: u64) -> anyhow::Result<T> {
        let mut buf = vec![0u8; std::mem::size_of::<T>()];
        self.read_exact_at(offset, &mut buf)?;
        Ok(bytemuck::from_bytes::<T>(&buf).to_owned())
    }
}
