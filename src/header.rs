use bytemuck::{Pod, Zeroable};
use uuid::Uuid;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VdiHeader {
    pub text: [u8; 0x40],
    pub signature: u32,
    pub version: u32,
    pub header_size: u32,
    pub image_type: u32,
    pub image_flags: u32,
    pub description: [u8; 0x100],
    pub block_offsets_offset: u32,
    pub data_offset: u32,
    pub cylinders: u32, // disk geometry, unused here
    pub heads: u32,     // disk geometry, unused here
    pub sectors: u32,   // disk geometry, unused here
    pub sector_size: u32,
    pub unused1: u32,
    pub disk_size: u64,
    pub block_size: u32,
    pub block_extra: u32, // unused here
    pub blocks_in_image: u32,
    pub blocks_allocated: u32,
    pub uuid_image: Uuid,
    pub uuid_last_snap: Uuid,
    pub uuid_link: Uuid,
    pub uuid_parent: Uuid,
}

impl VdiHeader {
    pub const VERSION: u32 = 0x00010001;
    pub const SIGNATURE: u32 = 0xBEDA107F;
}
