# Virtual Disk Image parser
This crate provides support for reading VirtualBox Virtual Disk Images (VDI).

Opened VDI files can be read using the std Read/Seek traits (writing is not supported (yet)).
Additionally, `VdiDisk` implements `ReadAt` from [positioned-io2](https://crates.io/crates/positioned-io2)

## Example
```rs
let file = File::open(&path)?;
let mut disk = VdiDisk::open(file)?;
println!("VDI header: {:X?}", disk.header);

let partitions = bootsector::list_partitions(&mut disk, &bootsector::Options::default())?;
```