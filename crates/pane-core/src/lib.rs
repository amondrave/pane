//! Pane core: memory-mapped file loading and line indexing.
//!
//! Key design (see PRD): the original file is mapped read-only (`memmap2`) and
//! the OS manages paging. Only the line-offset index lives on the heap, never
//! the content. This is what lets Pane open multi-GB files without loading them
//! entirely into memory.

use std::borrow::Cow;
use std::fs::File;
use std::io;
use std::path::Path;

use memmap2::Mmap;

/// A text file opened via mmap, with a line index.
pub struct TextFile {
    mmap: Mmap,
    /// Byte offset where each line starts. `len()` == number of lines.
    line_starts: Vec<usize>,
}

impl TextFile {
    /// Opens `path`, maps it into memory and builds the line index.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        // SAFETY: we treat the mapping as read-only and never mutate it. The
        // file could change underneath us; for the spike we assume it stays
        // stable (handling external edits is v1+ work).
        let mmap = unsafe { Mmap::map(&file)? };
        let line_starts = build_line_index(&mmap);
        Ok(Self { mmap, line_starts })
    }

    /// File size in bytes.
    pub fn byte_len(&self) -> usize {
        self.mmap.len()
    }

    /// Number of lines.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Raw bytes of line `idx`, without the trailing newline (LF or CRLF).
    pub fn line_bytes(&self, idx: usize) -> Option<&[u8]> {
        let start = *self.line_starts.get(idx)?;
        let end = self
            .line_starts
            .get(idx + 1)
            .copied()
            .unwrap_or(self.mmap.len());
        let mut slice = &self.mmap[start..end];
        if slice.last() == Some(&b'\n') {
            slice = &slice[..slice.len() - 1];
        }
        if slice.last() == Some(&b'\r') {
            slice = &slice[..slice.len() - 1];
        }
        Some(slice)
    }

    /// Line `idx` as text (lossy on invalid UTF-8).
    pub fn line(&self, idx: usize) -> Option<Cow<'_, str>> {
        self.line_bytes(idx).map(String::from_utf8_lossy)
    }

    /// Heap bytes used by the line index.
    ///
    /// The mmap pages are NOT counted here: the OS manages them and they don't
    /// live on the process heap. This number is the real RAM cost we add.
    pub fn index_heap_bytes(&self) -> usize {
        self.line_starts.capacity() * std::mem::size_of::<usize>()
    }
}

/// Builds the index of line-start offsets by scanning for LF bytes.
///
/// Uses `memchr` (SIMD) to sweep the buffer at memory speed. Design note: this
/// reads the whole file once. For 10 GB with instant open, v1 will need a
/// sampled/lazy index; here we measure the honest cost of the full index.
fn build_line_index(bytes: &[u8]) -> Vec<usize> {
    let mut starts = Vec::with_capacity(1024);
    starts.push(0);
    for pos in memchr::memchr_iter(b'\n', bytes) {
        starts.push(pos + 1);
    }
    // If the file ends in '\n', the last offset points past the end and marks a
    // phantom empty line: drop it (except for an empty file).
    if let Some(&last) = starts.last() {
        if last == bytes.len() && starts.len() > 1 {
            starts.pop();
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(name: &str, content: &[u8]) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(name);
        let mut f = File::create(&p).unwrap();
        f.write_all(content).unwrap();
        p
    }

    #[test]
    fn counts_and_reads_lines() {
        let p = write_tmp("pane_test_basic.txt", b"one\ntwo\nthree\n");
        let tf = TextFile::open(&p).unwrap();
        assert_eq!(tf.line_count(), 3);
        assert_eq!(tf.line(0).unwrap(), "one");
        assert_eq!(tf.line(1).unwrap(), "two");
        assert_eq!(tf.line(2).unwrap(), "three");
        assert!(tf.line(3).is_none());
    }

    #[test]
    fn no_trailing_newline() {
        let p = write_tmp("pane_test_nonl.txt", b"a\nb");
        let tf = TextFile::open(&p).unwrap();
        assert_eq!(tf.line_count(), 2);
        assert_eq!(tf.line(1).unwrap(), "b");
    }

    #[test]
    fn handles_crlf() {
        let p = write_tmp("pane_test_crlf.txt", b"a\r\nb\r\n");
        let tf = TextFile::open(&p).unwrap();
        assert_eq!(tf.line(0).unwrap(), "a");
        assert_eq!(tf.line(1).unwrap(), "b");
    }
}
