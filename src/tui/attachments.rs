//! Files attached to the message being read.

/// `bytes` as a short size, such as `2 KB` or `1.2 MB`.
pub fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    match bytes {
        b if b < KB => format!("{b} B"),
        b if b < MB => format!("{} KB", b.div_ceil(KB)),
        b if b < GB => format!("{:.1} MB", b as f64 / MB as f64),
        b => format!("{:.1} GB", b as f64 / GB as f64),
    }
}
