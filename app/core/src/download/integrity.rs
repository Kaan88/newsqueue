//! Resume metadata is evidence only after its bytes have been checked on disk.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Record {
    pub id: String,
    pub offset: u64,
    pub len: u64,
    pub crc: u32,
    #[serde(default)]
    pub file_crc: Option<u32>,
}

pub(super) fn verified_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".nqverified");
    name.into()
}

pub(super) fn read(path: &Path, metadata: &Path, segments: &[Segment]) -> HashMap<String, Record> {
    let mut records = HashMap::new();
    let Ok(mut file) = fs::File::open(path) else { return records; };
    let Ok(text) = fs::read_to_string(metadata) else { return records; };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let ids: HashSet<&str> = segments.iter().map(|s| s.id.as_str()).collect();
    let candidates: Vec<Record> = text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();
    // Deduplicate before hashing, so repeated lines cannot bypass whole-file CRC.
    let mut unique = HashMap::new();
    for record in candidates { unique.insert(record.id.clone(), record); }
    let mut candidates: Vec<Record> = unique.into_values().collect();
    candidates.sort_by_key(|r| r.offset);
    let mut bytes = Vec::new();
    let mut whole = crc32fast::Hasher::new();
    let mut end = 0;
    let mut contiguous = true;
    let mut advertised = None;
    for record in candidates {
        if !ids.contains(record.id.as_str()) || record.len > MAX_ARTICLE_BYTES as u64
            || record.offset.checked_add(record.len).is_none_or(|end| end > size) { continue; }
        bytes.resize(record.len as usize, 0);
        if file.seek(SeekFrom::Start(record.offset)).is_ok()
            && file.read_exact(&mut bytes).is_ok() && crc32(&bytes) == record.crc {
            contiguous &= record.offset == end;
            end = record.offset + record.len;
            whole.update(&bytes);
            if let Some(crc) = record.file_crc {
                if advertised.is_some_and(|old| old != crc) { return HashMap::new(); }
                advertised = Some(crc);
            }
            records.insert(record.id.clone(), record);
        }
    }
    if contiguous && end == size && records.len() == segments.len()
        && advertised.is_some_and(|crc| crc != whole.finalize()) { records.clear(); }

    records
}

pub(super) fn covers(records: &HashMap<String, Record>, count: usize, size: u64) -> bool {
    if records.len() != count || count == 0 { return false; }
    let mut ranges: Vec<_> = records.values().collect();
    ranges.sort_by_key(|r| r.offset);
    let mut end = 0;
    for r in ranges {
        if r.offset != end { return false; }
        let Some(next) = end.checked_add(r.len) else { return false; };
        end = next;
    }
    end == size
}

pub(super) fn encode(records: &HashMap<String, Record>) -> Vec<u8> {
    let mut out = Vec::new();
    for record in records.values() {
        serde_json::to_writer(&mut out, record).expect("serializable resume record");
        out.push(b'\n');
    }
    out
}

pub(super) fn append(task: &Task, offset: u64, data: &[u8], file_crc: Option<u32>) -> Result<(), String> {
    let record = Record { id: task.id.clone(), offset, len: data.len() as u64, crc: crc32(data), file_crc };
    let mut line = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
    line.push(b'\n');
    fs::OpenOptions::new().create(true).append(true).open(&task.progress)
        .and_then(|mut file| file.write_all(&line))
        .map_err(|e| format!("Cannot save resume metadata: {e}"))
}
