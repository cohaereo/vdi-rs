use crate::{
    structs::{
        DirEntry, DirectoryEntry, EXT4_EXTENTS_FL, EXT4_FT_DIR, EXT4_FT_REG_FILE, EXT4_ROOT_INO,
        EXT4_SUPER_MAGIC, GroupDescriptor, Inode, Superblock,
    },
    util::ReadAtExt,
};
use positioned_io2::ReadAt;
use std::io::{Read, Seek};
use thiserror::Error;
use unix_path::{Path, PathBuf};

pub mod structs;
mod util;

#[derive(Error, Debug)]
pub enum Ext4Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid superblock magic number")]
    InvalidSuperblock,
    #[error("Unsupported filesystem feature: {0}")]
    UnsupportedFeature(&'static str),
    #[error("Invalid inode number: {0}")]
    InvalidInode(u32),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Invalid directory entry")]
    InvalidDirectoryEntry,
}

pub type Result<T> = std::result::Result<T, Ext4Error>;

pub struct Metadata {
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
    pub mode: u16,
}

pub struct DirectoryIterator {
    entries: Vec<DirectoryEntry>,
    index: usize,
}

impl Iterator for DirectoryIterator {
    type Item = DirectoryEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.entries.len() {
            let entry = self.entries[self.index].clone();
            self.index += 1;
            Some(entry)
        } else {
            None
        }
    }
}

pub struct Ext4FileReader<'a, R: ReadAt> {
    reader: &'a Ext4Reader<R>,
    inode: Inode,
    position: u64,
    size: u64,
}

impl<'a, R: ReadAt> Ext4FileReader<'a, R> {
    pub fn size(&self) -> u64 {
        self.size
    }
}

impl<'a, R: ReadAt> Read for Ext4FileReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.size {
            return Ok(0);
        }

        let remaining = self.size - self.position;
        let to_read = std::cmp::min(buf.len() as u64, remaining) as usize;

        if to_read == 0 {
            return Ok(0);
        }

        let data = self
            .reader
            .read_file_data_range(&self.inode, self.position, to_read)
            .map_err(std::io::Error::other)?;

        let bytes_read = data.len();
        buf[..bytes_read].copy_from_slice(&data);
        self.position += bytes_read as u64;

        Ok(bytes_read)
    }
}

impl<'a, R: ReadAt> Seek for Ext4FileReader<'a, R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            std::io::SeekFrom::Start(offset) => offset,
            std::io::SeekFrom::End(offset) => {
                if offset >= 0 {
                    self.size + offset as u64
                } else {
                    self.size.checked_sub((-offset) as u64).ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek before start")
                    })?
                }
            }
            std::io::SeekFrom::Current(offset) => {
                if offset >= 0 {
                    self.position + offset as u64
                } else {
                    self.position.checked_sub((-offset) as u64).ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek before start")
                    })?
                }
            }
        };

        if new_pos > self.size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Seek beyond end of file",
            ));
        }

        self.position = new_pos;
        Ok(self.position)
    }
}

pub struct Ext4Reader<R: ReadAt> {
    reader: R,
    superblock: Superblock,
    group_descriptors: Vec<GroupDescriptor>,
    block_size: u64,
}

impl<R: ReadAt> Ext4Reader<R> {
    pub fn new(mut reader: R) -> Result<Self> {
        let superblock = Self::read_superblock(&mut reader)?;
        let block_size = if superblock.s_log_block_size < 32 {
            1024u64 << superblock.s_log_block_size
        } else {
            1024u64
        };

        let group_count = superblock
            .s_blocks_count_lo
            .div_ceil(superblock.s_blocks_per_group);
        let group_descriptors =
            Self::read_group_descriptors(&mut reader, group_count as usize, block_size)?;

        Ok(Ext4Reader {
            reader,
            superblock,
            group_descriptors,
            block_size,
        })
    }

    fn read_superblock(reader: &mut R) -> Result<Superblock> {
        let s = reader.read_pod_owned::<Superblock>(1024)?;
        if s.s_magic != EXT4_SUPER_MAGIC {
            return Err(Ext4Error::InvalidSuperblock);
        }

        Ok(s)
    }

    fn read_group_descriptors(
        reader: &mut R,
        group_count: usize,
        block_size: u64,
    ) -> Result<Vec<GroupDescriptor>> {
        let gdt_offset = if block_size == 1024 { 2048 } else { block_size };
        let descriptors = reader.read_pod_vec::<GroupDescriptor>(gdt_offset, group_count)?;

        Ok(descriptors)
    }

    fn read_inode(&self, inode_num: u32) -> Result<Inode> {
        if inode_num == 0 {
            return Err(Ext4Error::InvalidInode(inode_num));
        }

        let group = (inode_num - 1) / self.superblock.s_inodes_per_group;
        let index = (inode_num - 1) % self.superblock.s_inodes_per_group;

        if group as usize >= self.group_descriptors.len() {
            return Err(Ext4Error::InvalidInode(inode_num));
        }

        let group_desc = &self.group_descriptors[group as usize];
        let inode_table_block = group_desc.bg_inode_table_lo as u64;
        let inode_offset = inode_table_block * self.block_size
            + index as u64 * self.superblock.s_inode_size as u64;

        let inode = self.reader.read_pod_owned::<Inode>(inode_offset)?;

        Ok(inode)
    }

    fn read_directory_entries(&self, inode: &Inode) -> Result<Vec<DirEntry>> {
        let mut entries = Vec::new();
        let size = ((inode.i_size_high as u64) << 32) | inode.i_size_lo as u64;

        if inode.i_flags & EXT4_EXTENTS_FL != 0 {
            let blocks = self.read_extent_blocks(inode)?;
            for block_num in blocks {
                self.read_directory_block(block_num, size, &mut entries)?;
            }
        } else {
            for &block_num in &inode.i_block[0..12] {
                if block_num == 0 {
                    break;
                }

                self.read_directory_block(block_num, size, &mut entries)?;
            }
        }

        Ok(entries)
    }

    fn read_extent_blocks(&self, inode: &Inode) -> Result<Vec<u32>> {
        self.read_extent_blocks_recursive(&inode.i_block)
    }

    fn read_extent_blocks_recursive(&self, extent_data: &[u32]) -> Result<Vec<u32>> {
        let magic = extent_data[0] & 0xFFFF;
        let entries = (extent_data[0] >> 16) & 0xFFFF;
        let _max_entries = extent_data[1] & 0xFFFF;
        let depth = (extent_data[1] >> 16) & 0xFFFF;

        if magic != 0xF30A {
            return Err(Ext4Error::UnsupportedFeature("extents"));
        }

        let mut blocks = Vec::new();

        if depth == 0 {
            // Leaf extents - direct references to data blocks
            for i in 0..entries {
                let base_idx = (3 + i * 3) as usize;
                if base_idx + 2 < extent_data.len() {
                    let _logical_block = extent_data[base_idx];
                    let length = extent_data[base_idx + 1] & 0xFFFF;
                    let physical_block_hi = (extent_data[base_idx + 1] >> 16) & 0xFFFF;
                    let physical_block_lo = extent_data[base_idx + 2];
                    let physical_block =
                        ((physical_block_hi as u64) << 32) | physical_block_lo as u64;

                    for j in 0..length {
                        let block_num = physical_block + j as u64;
                        blocks.push(block_num as u32);
                    }
                }
            }
        } else {
            // Index extents - indirect references to other extent blocks
            for i in 0..entries {
                let base_idx = (3 + i * 3) as usize;
                if base_idx + 2 < extent_data.len() {
                    let _logical_block = extent_data[base_idx];
                    let physical_block_lo = extent_data[base_idx + 1];
                    let physical_block_hi = extent_data[base_idx + 2] & 0xFFFF;
                    let physical_block =
                        ((physical_block_hi as u64) << 32) | physical_block_lo as u64;

                    let block_offset = physical_block * self.block_size;
                    let mut block_data = vec![0u8; self.block_size as usize];
                    self.reader.read_exact_at(block_offset, &mut block_data)?;

                    let u32_data: Vec<u32> = block_data
                        .chunks_exact(4)
                        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        .collect();

                    let mut sub_blocks = self.read_extent_blocks_recursive(&u32_data)?;
                    blocks.append(&mut sub_blocks);
                }
            }
        }

        Ok(blocks)
    }

    fn read_directory_block(
        &self,
        block_num: u32,
        size: u64,
        entries: &mut Vec<DirEntry>,
    ) -> Result<()> {
        let block_offset = block_num as u64 * self.block_size;

        let mut block_data = vec![0u8; self.block_size as usize];
        self.reader.read_exact_at(block_offset, &mut block_data)?;

        let mut offset = 0;
        while offset < block_data.len() && offset < size as usize {
            if offset + 8 > block_data.len() {
                break;
            }

            let inode = u32::from_le_bytes([
                block_data[offset],
                block_data[offset + 1],
                block_data[offset + 2],
                block_data[offset + 3],
            ]);

            let rec_len = u16::from_le_bytes([block_data[offset + 4], block_data[offset + 5]]);
            if rec_len == 0 || rec_len as usize > block_data.len() - offset {
                return Err(Ext4Error::InvalidDirectoryEntry);
            }

            let name_len = block_data[offset + 6];
            let file_type = block_data[offset + 7];

            if name_len as usize > rec_len as usize - 8 {
                return Err(Ext4Error::InvalidDirectoryEntry);
                // offset += rec_len as usize;
                // continue;
            }

            let name_bytes = &block_data[offset + 8..offset + 8 + name_len as usize];
            let name = String::from_utf8_lossy(name_bytes).to_string();

            if inode != 0 && !name.is_empty() {
                entries.push(DirEntry {
                    inode,
                    rec_len,
                    name_len,
                    file_type,
                    name,
                });
            }

            offset += rec_len as usize;
        }

        Ok(())
    }

    pub fn read_dir<P: AsRef<Path>>(&self, path: P) -> Result<DirectoryIterator> {
        let path = path.as_ref();
        let inode_num = self.find_inode_by_path(path)?;
        let inode = self.read_inode(inode_num)?;

        if (inode.i_mode & 0xF000) != 0x4000 {
            return Err(Ext4Error::FileNotFound(format!(
                "{} is not a directory",
                path.display()
            )));
        }

        let entries = self.read_directory_entries(&inode)?;
        let mut dir_entries = Vec::new();

        for entry in entries {
            if entry.name == "." || entry.name == ".." {
                continue;
            }

            let entry_path = if path.as_unix_str() == "/" {
                PathBuf::from(format!("/{}", entry.name))
            } else {
                PathBuf::from(format!("{}/{}", path.display(), entry.name))
            };

            let is_dir = entry.file_type == EXT4_FT_DIR;
            let is_file = entry.file_type == EXT4_FT_REG_FILE;

            let size = if is_file {
                match self.read_inode(entry.inode) {
                    Ok(file_inode) => {
                        ((file_inode.i_size_high as u64) << 32) | file_inode.i_size_lo as u64
                    }
                    Err(_) => 0,
                }
            } else {
                0
            };

            dir_entries.push(DirectoryEntry {
                name: entry.name,
                path: entry_path,
                is_file,
                is_dir,
                size,
            });
        }

        dir_entries.sort_by_key(|entry| entry.name.clone());

        Ok(DirectoryIterator {
            entries: dir_entries,
            index: 0,
        })
    }

    pub fn open<P: AsRef<Path>>(&self, path: P) -> Result<Ext4FileReader<'_, R>> {
        let path = path.as_ref();
        let inode_num = self.find_inode_by_path(path)?;
        let inode = self.read_inode(inode_num)?;

        if (inode.i_mode & 0xF000) != 0x8000 {
            return Err(Ext4Error::FileNotFound(format!(
                "{} is not a regular file",
                path.display()
            )));
        }

        let size = ((inode.i_size_high as u64) << 32) | inode.i_size_lo as u64;

        Ok(Ext4FileReader {
            reader: self,
            inode,
            position: 0,
            size,
        })
    }

    pub fn exists<P: AsRef<Path>>(&self, path: P) -> bool {
        self.find_inode_by_path(path).is_ok()
    }

    pub fn metadata<P: AsRef<Path>>(&self, path: P) -> Option<Metadata> {
        let path = path.as_ref();
        let inode_num = self.find_inode_by_path(path).ok()?;
        let inode = self.read_inode(inode_num).ok()?;

        let is_file = (inode.i_mode & 0xF000) == 0x8000;
        let is_dir = (inode.i_mode & 0xF000) == 0x4000;
        let size = ((inode.i_size_high as u64) << 32) | inode.i_size_lo as u64;

        Some(Metadata {
            is_file,
            is_dir,
            size,
            mode: inode.i_mode,
        })
    }

    fn find_inode_by_path<P: AsRef<Path>>(&self, path: P) -> Result<u32> {
        let path = path.as_ref();
        let path_str = path
            .as_unix_str()
            .to_str()
            .ok_or_else(|| Ext4Error::FileNotFound("Invalid path encoding".to_string()))?;

        if path_str == "/" {
            return Ok(EXT4_ROOT_INO);
        }

        let components: Vec<&str> = path_str.split('/').filter(|s| !s.is_empty()).collect();
        let mut current_inode = EXT4_ROOT_INO;

        for component in components {
            let inode = self.read_inode(current_inode)?;

            if (inode.i_mode & 0xF000) != 0x4000 {
                return Err(Ext4Error::FileNotFound(format!(
                    "Path component is not a directory: {}",
                    component
                )));
            }

            let entries = self.read_directory_entries(&inode)?;
            let mut found = false;

            for entry in entries {
                if entry.name == component {
                    current_inode = entry.inode;
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(Ext4Error::FileNotFound(format!(
                    "Path not found: {}",
                    path.display()
                )));
            }
        }

        Ok(current_inode)
    }

    fn read_file_data_range(&self, inode: &Inode, start: u64, length: usize) -> Result<Vec<u8>> {
        let file_size = ((inode.i_size_high as u64) << 32) | inode.i_size_lo as u64;

        if start >= file_size {
            return Ok(Vec::new());
        }

        let actual_length = std::cmp::min(length as u64, file_size - start) as usize;
        let mut data = vec![0u8; actual_length];
        let mut bytes_read = 0;

        let start_block = start / self.block_size;
        let start_offset = start % self.block_size;
        let mut remaining = actual_length;

        if inode.i_flags & EXT4_EXTENTS_FL != 0 {
            let blocks = self.read_extent_blocks(inode)?;

            for (block_idx, &block_num) in blocks.iter().enumerate() {
                if block_idx < start_block as usize {
                    continue;
                }

                if remaining == 0 {
                    break;
                }

                let block_offset = block_num as u64 * self.block_size;

                let skip_bytes = if block_idx == start_block as usize {
                    start_offset
                } else {
                    0
                };
                let read_size =
                    std::cmp::min(self.block_size - skip_bytes, remaining as u64) as usize;

                let mut block_data = vec![0u8; read_size];
                self.reader
                    .read_exact_at(block_offset + skip_bytes, &mut block_data)?;

                data[bytes_read..bytes_read + read_size].copy_from_slice(&block_data);
                bytes_read += read_size;
                remaining -= read_size;
            }
        } else {
            for (block_idx, &block_num) in inode.i_block[0..12].iter().enumerate() {
                if block_num == 0 {
                    break;
                }

                if (block_idx as u64) < start_block {
                    continue;
                }

                if remaining == 0 {
                    break;
                }

                let block_offset = block_num as u64 * self.block_size;

                let skip_bytes = if block_idx as u64 == start_block {
                    start_offset
                } else {
                    0
                };
                let read_size =
                    std::cmp::min(self.block_size - skip_bytes, remaining as u64) as usize;

                let mut block_data = vec![0u8; read_size];
                self.reader
                    .read_exact_at(block_offset + skip_bytes, &mut block_data)?;

                data[bytes_read..bytes_read + read_size].copy_from_slice(&block_data);
                bytes_read += read_size;
                remaining -= read_size;
            }
        }

        data.truncate(bytes_read);
        Ok(data)
    }
}

unsafe impl<R: Send + ReadAt> Send for Ext4Reader<R> {}
unsafe impl<R: Sync + ReadAt> Sync for Ext4Reader<R> {}
