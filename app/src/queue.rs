//! The download queue as the ListView shows it: collections with their files
//! indented below when expanded, flattened into rows. Pure functions, so the
//! tree, the cell texts and the change detection are unit-tested.

use newsqueue_core::model::{format_size_short, ItemStatus, QUEUE_COLUMN_NAMES};
use newsqueue_core::snapshot::QueueRow;

/// Logical column indices (`QUEUE_COLUMN_NAMES`).
pub const COL_SUBJECT: usize = 0;
pub const COL_STATUS: usize = 6;
pub const COLUMN_COUNT: usize = QUEUE_COLUMN_NAMES.len();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// Index into `Snapshot::queue`.
    Coll(usize),
    /// Collection index, file index.
    File(usize, usize),
}

impl Row {
    #[cfg(test)]
    pub fn collection(self) -> usize {
        match self {
            Row::Coll(c) | Row::File(c, _) => c,
        }
    }
}

/// Rows for the queue: collections whose subject (or one of whose file
/// names) contains `filter`, with their files when expanded.
pub fn flatten(queue: &[QueueRow], filter: &str) -> Vec<Row> {
    let f = filter.trim().to_lowercase();
    let mut rows = Vec::new();
    for (i, c) in queue.iter().enumerate() {
        let file_hit = |name: &str| !f.is_empty() && name.to_lowercase().contains(&f);
        let coll_hit = f.is_empty() || c.subject.to_lowercase().contains(&f);
        if !coll_hit && !c.file_rows.iter().any(|x| file_hit(&x.name)) {
            continue;
        }
        rows.push(Row::Coll(i));
        if c.expanded {
            for (j, x) in c.file_rows.iter().enumerate() {
                if coll_hit || file_hit(&x.name) {
                    rows.push(Row::File(i, j));
                }
            }
        }
    }
    rows
}

pub fn status_text(c: &QueueRow) -> String {
    let st = c.status.unwrap_or(ItemStatus::Queued);
    match st {
        ItemStatus::Downloading | ItemStatus::Paused | ItemStatus::Queued if c.progress > 0.0 && c.progress < 1.0 => {
            format!("{} {:.0}%", st.label(), (c.progress * 100.0).floor())
        }
        _ => st.label().to_string(),
    }
}

fn age(days: f64) -> String {
    if days <= 0.0 {
        String::new()
    } else {
        format!("{days:.1} days")
    }
}

fn yes_no(v: bool) -> String {
    if v { "Yes" } else { "No" }.into()
}

/// Text of logical column `col` for `row`.
pub fn cell(queue: &[QueueRow], row: Row, col: usize) -> String {
    match row {
        Row::Coll(i) => {
            let Some(c) = queue.get(i) else { return String::new() };
            match col {
                0 => c.subject.clone(),
                1 => format_size_short(c.total_bytes),
                2 => c.parts.to_string(),
                3 => age(c.age_days),
                4 => c.group.clone(),
                5 => c.poster.clone(),
                6 => status_text(c),
                7 => c.folder.clone(),
                8 => yes_no(c.unrar),
                9 => c.execute.clone(),
                10 => yes_no(c.delete_archives),
                11 => c.posted_on.clone(),
                _ => String::new(),
            }
        }
        Row::File(i, j) => {
            let Some(f) = queue.get(i).and_then(|c| c.file_rows.get(j)) else { return String::new() };
            match col {
                0 => f.name.clone(),
                1 => format_size_short(f.bytes),
                2 => f.parts.to_string(),
                6 => file_status(f),
                _ => String::new(),
            }
        }
    }
}

pub fn file_status(f: &newsqueue_core::snapshot::FileRow) -> String {
    if f.done || f.progress >= 1.0 {
        "Completed".into()
    } else if f.held {
        "Held (PAR2)".into()
    } else if f.progress > 0.0 {
        format!("{:.0}%", (f.progress * 100.0).floor())
    } else {
        String::new()
    }
}

/// How the progress bar of a row is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bar {
    Normal,
    Paused,
    Error,
    Done,
}

pub fn bar(queue: &[QueueRow], row: Row) -> (f32, Bar) {
    match row {
        Row::Coll(i) => {
            let Some(c) = queue.get(i) else { return (0.0, Bar::Normal) };
            let kind = match c.status.unwrap_or(ItemStatus::Queued) {
                ItemStatus::Completed => Bar::Done,
                ItemStatus::Paused => Bar::Paused,
                ItemStatus::Error | ItemStatus::Incomplete => Bar::Error,
                _ => Bar::Normal,
            };
            (c.progress.clamp(0.0, 1.0), kind)
        }
        Row::File(i, j) => {
            let Some(f) = queue.get(i).and_then(|c| c.file_rows.get(j)) else { return (0.0, Bar::Normal) };
            let kind = if f.done || f.progress >= 1.0 {
                Bar::Done
            } else if f.held {
                Bar::Paused
            } else {
                Bar::Normal
            };
            (if f.done { 1.0 } else { f.progress.clamp(0.0, 1.0) }, kind)
        }
    }
}

/// Muted text for finished collections and their files.
pub fn finished(queue: &[QueueRow], row: Row) -> bool {
    bar(queue, row).1 == Bar::Done
}

/// A row's visible content, to find the rows a redraw needs.
pub fn signature(queue: &[QueueRow], row: Row, cols: &[usize]) -> String {
    let mut s = String::new();
    for &c in cols {
        s.push_str(&cell(queue, row, c));
        s.push('\u{1}');
    }
    let (p, b) = bar(queue, row);
    s.push_str(&format!("{:.3}{b:?}", p));
    if let Row::Coll(i) = row {
        if let Some(c) = queue.get(i) {
            s.push(if c.expanded { '-' } else { '+' });
        }
    }
    s
}

/// Contiguous index ranges whose rows differ between two renderings.
pub fn changed_ranges(old: &[String], new: &[String]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for i in 0..new.len().max(old.len()) {
        let differs = old.get(i) != new.get(i);
        match (differs, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, i - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, new.len().max(old.len()) - 1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use newsqueue_core::snapshot::FileRow;

    fn q() -> Vec<QueueRow> {
        let file = |name: &str, progress: f32, done: bool| FileRow { name: name.into(), bytes: 1000, parts: 2, progress, held: false, done };
        vec![
            QueueRow {
                id: 1,
                subject: "Alpha.Release".into(),
                status: Some(ItemStatus::Downloading),
                progress: 0.426,
                expanded: true,
                parts: 4,
                file_rows: vec![file("alpha.part01.rar", 1.0, true), file("alpha.part02.rar", 0.5, false)],
                ..Default::default()
            },
            QueueRow {
                id: 2,
                subject: "Beta.Archive".into(),
                status: Some(ItemStatus::Completed),
                progress: 1.0,
                expanded: false,
                file_rows: vec![file("beta.zip", 1.0, true)],
                ..Default::default()
            },
        ]
    }

    #[test]
    fn expanded_collections_show_their_files_below() {
        let rows = flatten(&q(), "");
        assert_eq!(rows, vec![Row::Coll(0), Row::File(0, 0), Row::File(0, 1), Row::Coll(1)]);
        assert_eq!(rows[2].collection(), 0);
    }

    #[test]
    fn the_filter_matches_subjects_and_file_names() {
        assert_eq!(flatten(&q(), "beta"), vec![Row::Coll(1)]);
        // A file hit keeps its collection and shows only the matching files.
        assert_eq!(flatten(&q(), "part02"), vec![Row::Coll(0), Row::File(0, 1)]);
        assert!(flatten(&q(), "nothing").is_empty());
    }

    #[test]
    fn cells_and_bars_follow_the_row_state() {
        let q = q();
        assert_eq!(cell(&q, Row::Coll(0), COL_STATUS), "Downloading 42%");
        assert_eq!(cell(&q, Row::File(0, 0), COL_STATUS), "Completed");
        assert_eq!(cell(&q, Row::File(0, 1), COL_STATUS), "50%");
        assert_eq!(cell(&q, Row::File(0, 1), 4), "");
        assert_eq!(bar(&q, Row::Coll(1)).1, Bar::Done);
        assert!(finished(&q, Row::Coll(1)));
        assert!(!finished(&q, Row::Coll(0)));
    }

    #[test]
    fn only_changed_rows_are_redrawn() {
        let a = vec!["x".to_string(), "y".into(), "z".into(), "w".into()];
        let b = vec!["x".to_string(), "Y".into(), "Z".into(), "w".into(), "v".into()];
        assert_eq!(changed_ranges(&a, &b), vec![(1, 2), (4, 4)]);
        assert!(changed_ranges(&a, &a).is_empty());
        let mut q2 = q();
        let cols: Vec<usize> = (0..COLUMN_COUNT).collect();
        let before = signature(&q2, Row::Coll(0), &cols);
        q2[0].progress = 0.5;
        assert_ne!(before, signature(&q2, Row::Coll(0), &cols));
    }
}
