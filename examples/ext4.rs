use ext4::Ext4Reader;
use positioned_io2::ReadAt;
use vdi::VdiDisk;

fn main() -> anyhow::Result<()> {
    let Some(path) = std::env::args().nth(1) else {
        anyhow::bail!("Usage: {} <image>", std::env::args().next().unwrap());
    };

    let file = std::fs::File::open(&path)?;
    let mut disk = VdiDisk::open(file)?;
    println!("VDI header: {:X?}", disk.header);

    let partitions = bootsector::list_partitions(&mut disk, &bootsector::Options::default())?;

    for part in partitions {
        let disk = disk.slice(part.first_byte..part.first_byte + part.len);
        let ext4 = match Ext4Reader::new(disk) {
            Ok(ext4) => ext4,
            Err(e) => {
                eprintln!("Failed to open partition {}: {}", part.id, e);
                continue;
            }
        };

        traverse_directory(&ext4, unix_path::Path::new("/"), 0)?;
    }

    Ok(())
}

fn traverse_directory<R: ReadAt>(
    reader: &Ext4Reader<R>,
    path: &unix_path::Path,
    indent: usize,
) -> anyhow::Result<()> {
    let indent_str = "  ".repeat(indent);
    let entries = reader.read_dir(path)?;

    for entry in entries {
        let full_path = path.join(&entry.name);
        let file_type = if entry.is_dir {
            "DIR"
        } else if entry.is_file {
            "FILE"
        } else {
            "OTHER"
        };

        println!(
            "{}{} {} ({} bytes)",
            indent_str, file_type, entry.name, entry.size
        );

        if entry.is_dir {
            traverse_directory(reader, &full_path, indent + 1)?;
        }
    }

    Ok(())
}
