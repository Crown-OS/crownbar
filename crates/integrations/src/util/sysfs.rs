//! Reading the small text files under `/sys` and `/proc` into the stack, for
//! pollers that would otherwise allocate a string per number per tick.

use std::{fs::File, io::Read, path::Path};

/// Wider than any single value sysfs prints.
const NUMBER_LEN: usize = 32;

/// The opening of the file at `path`, as much of it as fits in `buffer`.
pub fn read_head<'buffer>(path: &Path, buffer: &'buffer mut [u8]) -> Option<&'buffer str> {
    let mut file = File::open(path).ok()?;
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]).ok()? {
            0 => break,
            read => filled += read,
        }
    }
    std::str::from_utf8(&buffer[..filled]).ok()
}

/// A file holding one unsigned number, as sysfs attributes do.
pub fn read_number(path: &Path) -> Option<u64> {
    let mut buffer = [0; NUMBER_LEN];
    read_head(path, &mut buffer)?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_number_with_its_newline() {
        let path = std::env::temp_dir().join(format!("crownbar-sysfs-{}", std::process::id()));
        std::fs::write(&path, "4200000\n").expect("temp dir is writable");
        assert_eq!(read_number(&path), Some(4_200_000));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn stops_at_the_buffer() {
        let mut buffer = [0; 8];
        let head = read_head(Path::new("/proc/self/status"), &mut buffer);
        assert_eq!(head.map(str::len), Some(8));
    }

    #[test]
    fn missing_file_is_none() {
        assert_eq!(read_number(Path::new("/nonexistent/crownbar")), None);
    }
}
