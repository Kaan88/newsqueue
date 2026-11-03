//! File-level join/split used by the queue and Explorer actions.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Join files in the given order into `output`.
pub fn join_files(inputs: &[PathBuf], output: &Path) -> Result<u64, String> {
    if inputs.len() < 2 {
        return Err("Select at least two files to join".into());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut out = File::create(output).map_err(|error| error.to_string())?;
    let mut total = 0u64;
    for input in inputs {
        let mut file = File::open(input)
            .map_err(|error| format!("Cannot open {}: {error}", input.display()))?;
        total += std::io::copy(&mut file, &mut out)
            .map_err(|error| format!("Cannot write {}: {error}", output.display()))?;
    }
    out.flush().map_err(|error| error.to_string())?;
    Ok(total)
}

/// Split `input` into numbered parts of `part_size` bytes.
pub fn split_file(
    input: &Path,
    part_size: u64,
    output_dir: &Path,
    base_name: &str,
) -> Result<Vec<PathBuf>, String> {
    if part_size == 0 {
        return Err("Part size must be greater than zero".into());
    }
    fs::create_dir_all(output_dir).map_err(|error| error.to_string())?;
    let mut base = base_name.to_string();
    let mut suffix = 1;
    while output_dir.join(format!("{base}.001")).exists()
        || output_dir.join(format!("{base}.001")) == input
    {
        base = format!("{base_name}-{suffix}");
        suffix += 1;
    }
    let mut source = File::open(input).map_err(|error| error.to_string())?;
    let mut parts = Vec::new();
    let mut number = 1u32;
    let path = output_dir.join(format!("{base}.{number:03}"));
    let mut part = File::create(&path).map_err(|error| error.to_string())?;
    parts.push(path);
    let mut written = 0u64;
    let mut buffer = vec![0u8; (part_size.min(1024 * 1024)).max(1) as usize];
    loop {
        let read = source.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        let mut offset = 0;
        while offset < read {
            if written >= part_size {
                part.flush().map_err(|error| error.to_string())?;
                number += 1;
                let path = output_dir.join(format!("{base}.{number:03}"));
                part = File::create(&path).map_err(|error| error.to_string())?;
                parts.push(path);
                written = 0;
            }
            let count = (part_size - written).min((read - offset) as u64) as usize;
            part.write_all(&buffer[offset..offset + count])
                .map_err(|error| error.to_string())?;
            offset += count;
            written += count as u64;
        }
    }
    part.flush().map_err(|error| error.to_string())?;
    Ok(parts)
}

/// Find `.001`, `.002`, ... split parts in `folder` and join the first set.
pub fn join_split_parts(folder: &Path) -> Result<Option<PathBuf>, String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Ok(None);
    };
    let mut groups: std::collections::HashMap<String, Vec<(u32, PathBuf)>> =
        std::collections::HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            continue;
        };
        if extension.len() != 3 || !extension.chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        let Ok(number) = extension.parse::<u32>() else {
            continue;
        };
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        groups
            .entry(stem.to_string())
            .or_default()
            .push((number, path));
    }
    let Some((stem, mut parts)) = groups
        .into_iter()
        .filter(|(_, parts)| parts.len() > 1)
        .max_by_key(|(_, parts)| parts.len())
    else {
        return Ok(None);
    };
    parts.sort_by_key(|(number, _)| *number);
    let output = folder.join(stem);
    let inputs: Vec<PathBuf> = parts.into_iter().map(|(_, path)| path).collect();
    join_files(&inputs, &output)?;
    Ok(Some(output))
}

/// Sort paths the way split parts are numbered.
pub fn sort_split_paths(paths: &mut [PathBuf]) {
    paths.sort_by(|left, right| natural_key(left).cmp(&natural_key(right)));
}

fn natural_key(path: &Path) -> (String, u64) {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some((stem, extension)) = name.rsplit_once('.') {
        if extension.chars().all(|character| character.is_ascii_digit()) {
            if let Ok(number) = extension.parse::<u64>() {
                return (stem.to_string(), number);
            }
        }
    }
    (name, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn joins_and_splits_files_round_trip() {
        let root = temp_root("files");
        let first = root.join("a.bin");
        let second = root.join("b.bin");
        fs::write(&first, b"hello ").unwrap();
        fs::write(&second, b"world").unwrap();
        let joined = root.join("joined.bin");
        assert_eq!(join_files(&[first, second], &joined).unwrap(), 11);
        assert_eq!(fs::read(&joined).unwrap(), b"hello world");

        let parts = split_file(&joined, 4, &root.join("parts"), "joined").unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(fs::read(&parts[0]).unwrap(), b"hell");
        assert_eq!(fs::read(&parts[1]).unwrap(), b"o wo");
        assert_eq!(fs::read(&parts[2]).unwrap(), b"rld");

        let recombined = root.join("recombined.bin");
        join_files(&parts, &recombined).unwrap();
        assert_eq!(fs::read(&recombined).unwrap(), b"hello world");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn joins_numbered_split_parts_in_a_folder() {
        let root = temp_root("split-parts");
        fs::write(root.join("release.001"), b"first").unwrap();
        fs::write(root.join("release.002"), b"second").unwrap();
        fs::write(root.join("release.003"), b"third").unwrap();
        let output = join_split_parts(&root).unwrap().unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"firstsecondthird");
        assert_eq!(output.file_name().unwrap(), "release");
        let _ = fs::remove_dir_all(root);
    }
}
