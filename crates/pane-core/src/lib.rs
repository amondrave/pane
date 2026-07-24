//! Pane core: memory-mapped file loading and a LAZY line index.
//!
//! Key design (see PRD §9): the original file is mapped read-only (`memmap2`)
//! and the OS manages paging. The line index is built **on demand** — we only
//! scan far enough to answer the lines actually requested (e.g. the visible
//! viewport). Opening a 10 GB file therefore touches almost no pages, keeping
//! peak RSS low, instead of scanning the whole file up front.
//!
//! Interior mutability (`Mutex`) lets the index grow behind a shared `&self`,
//! so a single `TextFile` can be shared (e.g. `Arc`) by the UI while it keeps
//! discovering lines as the user scrolls.

use std::borrow::Cow;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use memmap2::Mmap;

/// A text file opened via mmap, with a lazily-built line index.
pub struct TextFile {
    mmap: Mmap,
    index: Mutex<LineIndex>,
}

struct LineIndex {
    /// Byte offset where each discovered line starts. Always begins with `[0]`.
    line_starts: Vec<usize>,
    /// Byte offset scanned up to so far.
    scanned: usize,
    /// Whether the whole file has been scanned (true line count is known).
    complete: bool,
}

impl TextFile {
    /// Opens `path` and maps it into memory. Does NOT scan the file — the line
    /// index is built lazily on first access.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        // SAFETY: we treat the mapping as read-only and never mutate it. The
        // file could change underneath us; for now we assume it stays stable.
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            mmap,
            index: Mutex::new(LineIndex {
                line_starts: vec![0],
                scanned: 0,
                complete: false,
            }),
        })
    }

    /// File size in bytes.
    pub fn byte_len(&self) -> usize {
        self.mmap.len()
    }

    /// Number of lines discovered so far (grows as the file is scrolled/scanned).
    /// This is NOT the true total unless [`is_complete`](Self::is_complete).
    pub fn indexed_line_count(&self) -> usize {
        self.index.lock().unwrap().line_starts.len()
    }

    /// Whether the entire file has been scanned (true line count is known).
    pub fn is_complete(&self) -> bool {
        self.index.lock().unwrap().complete
    }

    /// True total line count. Forces a full scan if not already complete.
    pub fn line_count(&self) -> usize {
        self.ensure_indexed_to(usize::MAX);
        self.index.lock().unwrap().line_starts.len()
    }

    /// Clamps `idx` to a valid line, scanning on demand up to `idx` (or EOF).
    /// Passing `usize::MAX` forces a full scan and returns the last line index.
    pub fn clamp_to_line(&self, idx: usize) -> usize {
        self.ensure_indexed_to(idx);
        let count = self.index.lock().unwrap().line_starts.len();
        idx.min(count.saturating_sub(1))
    }

    /// Raw bytes of line `idx`, without the trailing newline (LF or CRLF).
    /// Returns `None` if `idx` is past the end of the file.
    pub fn line_bytes(&self, idx: usize) -> Option<&[u8]> {
        self.ensure_indexed_to(idx);
        let (start, end) = {
            let ix = self.index.lock().unwrap();
            let start = *ix.line_starts.get(idx)?;
            let end = ix
                .line_starts
                .get(idx + 1)
                .copied()
                .unwrap_or(self.mmap.len());
            (start, end)
        };
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

    /// Heap bytes used by the line index so far.
    ///
    /// The mmap pages are NOT counted: the OS manages them and they don't live
    /// on the process heap. This is the real RAM cost the index adds.
    pub fn index_heap_bytes(&self) -> usize {
        self.index.lock().unwrap().line_starts.capacity() * std::mem::size_of::<usize>()
    }

    /// Searches the whole file for `pattern`, returning the indices of matching
    /// lines (capped at `max`). `use_regex` selects regex vs literal substring.
    ///
    /// This scans the entire file — as any search must — building the full line
    /// index in the process. Returns an empty vec on an invalid regex.
    pub fn search(&self, pattern: &str, use_regex: bool, max: usize) -> Vec<usize> {
        let mut hits = Vec::new();
        if pattern.is_empty() || max == 0 {
            return hits;
        }
        enum Matcher {
            Re(regex::bytes::Regex),
            Lit(memchr::memmem::Finder<'static>),
        }
        let matcher = if use_regex {
            match regex::bytes::Regex::new(pattern) {
                Ok(re) => Matcher::Re(re),
                Err(_) => return hits,
            }
        } else {
            Matcher::Lit(memchr::memmem::Finder::new(pattern.as_bytes()).into_owned())
        };

        let mut i = 0usize;
        while let Some(line) = self.line_bytes(i) {
            let found = match &matcher {
                Matcher::Re(re) => re.is_match(line),
                Matcher::Lit(f) => f.find(line).is_some(),
            };
            if found {
                hits.push(i);
                if hits.len() >= max {
                    break;
                }
            }
            i += 1;
        }
        hits
    }

    /// Scans forward (via SIMD `memchr`) just enough so that line `target_line`
    /// can be read — i.e. until we know where line `target_line + 1` starts, or
    /// we hit EOF. No-op if already indexed that far. `usize::MAX` = full scan.
    fn ensure_indexed_to(&self, target_line: usize) {
        let mut ix = self.index.lock().unwrap();
        let need = target_line.saturating_add(2);
        while !ix.complete && ix.line_starts.len() < need {
            let from = ix.scanned;
            match memchr::memchr(b'\n', &self.mmap[from..]) {
                Some(rel) => {
                    let next = from + rel + 1;
                    ix.line_starts.push(next);
                    ix.scanned = next;
                }
                None => {
                    ix.complete = true;
                    // Drop the phantom empty line when the file ends in '\n'.
                    if let Some(&last) = ix.line_starts.last() {
                        if last == self.mmap.len() && ix.line_starts.len() > 1 {
                            ix.line_starts.pop();
                        }
                    }
                }
            }
        }
    }
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

    #[test]
    fn indexes_lazily() {
        let p = write_tmp("pane_test_lazy.txt", b"l0\nl1\nl2\nl3\nl4\n");
        let tf = TextFile::open(&p).unwrap();
        // Nothing scanned yet beyond the initial [0].
        assert_eq!(tf.indexed_line_count(), 1);
        assert!(!tf.is_complete());
        // Touching line 2 indexes only up to what's needed, not the whole file.
        assert_eq!(tf.line(2).unwrap(), "l2");
        assert!(tf.indexed_line_count() >= 3);
        assert!(!tf.is_complete());
        // Forcing the count completes the scan.
        assert_eq!(tf.line_count(), 5);
        assert!(tf.is_complete());
    }

    #[test]
    fn clamp_to_line_bounds() {
        let p = write_tmp("pane_test_clamp.txt", b"a\nb\nc\n");
        let tf = TextFile::open(&p).unwrap();
        assert_eq!(tf.clamp_to_line(1), 1);
        assert_eq!(tf.clamp_to_line(usize::MAX), 2); // last line
    }

    #[test]
    fn search_literal_regex_and_cap() {
        let p = write_tmp("pane_test_search.txt", b"alpha\nbeta\ngamma\nalphabet\n");
        let tf = TextFile::open(&p).unwrap();
        assert_eq!(tf.search("alpha", false, 100), vec![0, 3]); // alpha, alphabet
        assert_eq!(tf.search("^beta$", true, 100), vec![1]);
        assert_eq!(tf.search("zzz", false, 100), Vec::<usize>::new());
        assert_eq!(tf.search("a", false, 2).len(), 2); // capped
        assert!(tf.search("(", true, 100).is_empty()); // invalid regex → empty
    }
}
