use ext4::{Ext4Reader, Result};
use positioned_io2::ReadAt;
use std::env;
use std::fs::File;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <ext4_image_file>", args[0]);
        std::process::exit(1);
    }

    let image_path = &args[1];

    let reader = Ext4Reader::new(File::open(image_path)?)?;

    println!("\nRoot directory contents:");
    let root_path = unix_path::Path::new("/");
    traverse_directory(&reader, root_path, 0)?;

    Ok(())
}

fn traverse_directory<R: ReadAt>(
    reader: &Ext4Reader<R>,
    path: &unix_path::Path,
    indent: usize,
) -> Result<()> {
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
