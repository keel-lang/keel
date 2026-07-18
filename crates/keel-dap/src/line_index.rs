//! Byte-offset ↔ 1-indexed line/column conversion for one module's source
//! text. Kept local to `keel-dap` rather than shared with the LSP's
//! `position.rs` (which is `pub(crate)` to the root `keel-lang` crate and
//! coupled to `tower_lsp` types) — a future shared implementation belongs in
//! `keel-syntax`, alongside `Span` itself, once a second consumer needs it.

/// Byte offsets of every line start in a source text, built once per module.
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    /// 1-indexed line number containing byte offset `offset`.
    pub fn line_at(&self, offset: usize) -> u32 {
        match self.line_starts.binary_search(&offset) {
            Ok(i) => i as u32 + 1,
            Err(i) => i as u32, // i-1 is the containing line (0-indexed) -> +1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_is_one() {
        let idx = LineIndex::new("abc\ndef\n");
        assert_eq!(idx.line_at(0), 1);
        assert_eq!(idx.line_at(2), 1);
    }

    #[test]
    fn second_line_starts_after_newline() {
        let idx = LineIndex::new("abc\ndef\n");
        assert_eq!(idx.line_at(4), 2);
        assert_eq!(idx.line_at(6), 2);
    }
}
