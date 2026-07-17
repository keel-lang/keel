//! Span interning: [`SpanId`] handles into a per-program table of source
//! byte ranges, plus byte-offset -> (line, col) resolution.
//!
//! KIR nodes that need diagnostics (calls, casts, raises) carry a `SpanId`
//! instead of a raw [`keel_syntax::lexer::Span`] so the IR stays small and
//! `Copy`-friendly; the table is consulted only when rendering a dump or an
//! error report.
//!
//! The line/col resolution logic mirrors `src/lsp/position.rs`
//! (`offset_to_position`) in the root crate: 0-based line, 0-based column
//! counted in UTF-8 `char`s (not UTF-16 code units — matches the interpreter's
//! byte-oriented diagnostics, not the LSP's UTF-16 requirement). That file
//! lives in the `keel-lang` binary crate, which `keel-kir` must not depend on
//! (dependency rule in designs/llvm-compilation.md §2.2), so the logic is
//! duplicated here rather than shared.

use keel_syntax::lexer::Span;

/// A handle into a [`SpanTable`]. Cheap to copy and store on every KIR node
/// that needs source-position diagnostics.
pub type SpanId = u32;

/// A resolved source position: 0-based line and column (UTF-8 char count).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePos {
    pub line: u32,
    pub col: u32,
}

/// Per-program interned span table: `SpanId -> byte range`, plus the file
/// name and source text needed to resolve a range to line/col on demand.
///
/// One `SpanTable` per compiled file. `KirProgram::span_table` is the
/// canonical instance; `dump.rs` and future `keel-codegen` debug-info
/// emission both read through it.
#[derive(Debug, Clone)]
pub struct SpanTable {
    file_name: String,
    spans: Vec<Span>,
}

impl SpanTable {
    #[must_use]
    pub fn new(file_name: impl Into<String>) -> Self {
        Self {
            file_name: file_name.into(),
            spans: Vec::new(),
        }
    }

    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Interns `span`, returning a stable [`SpanId`]. Does not deduplicate —
    /// two calls with an identical range yield two distinct ids, which is
    /// fine: ids are never compared for span equality, only dereferenced.
    pub fn intern(&mut self, span: Span) -> SpanId {
        let id = u32::try_from(self.spans.len()).expect("span table overflowed u32::MAX entries");
        self.spans.push(span);
        id
    }

    #[must_use]
    pub fn byte_range(&self, id: SpanId) -> Span {
        self.spans[id as usize].clone()
    }

    /// Resolves `id`'s start position against `source`. `source` must be the
    /// exact text the spans were captured from.
    #[must_use]
    pub fn start_pos(&self, id: SpanId, source: &str) -> SourcePos {
        offset_to_pos(source, self.spans[id as usize].start)
    }

    /// Resolves `id`'s end position against `source`.
    #[must_use]
    pub fn end_pos(&self, id: SpanId, source: &str) -> SourcePos {
        offset_to_pos(source, self.spans[id as usize].end)
    }
}

/// UTF-8 byte offset -> 0-based (line, col). Mirrors
/// `src/lsp/position.rs::offset_to_position` in the root crate.
fn offset_to_pos(source: &str, offset: usize) -> SourcePos {
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    let mut i = 0;
    for ch in source.chars() {
        if i >= offset {
            break;
        }
        i += ch.len_utf8();
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    SourcePos { line, col }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_then_resolve_round_trips_line_col() {
        let source = "line one\nline two\nthird";
        let mut table = SpanTable::new("test.keel");
        let id = table.intern(9..13); // "line" at start of "line two"
        assert_eq!(table.start_pos(id, source), SourcePos { line: 1, col: 0 });
        assert_eq!(table.end_pos(id, source), SourcePos { line: 1, col: 4 });
    }

    #[test]
    fn distinct_ids_for_identical_spans() {
        let mut table = SpanTable::new("test.keel");
        let a = table.intern(0..3);
        let b = table.intern(0..3);
        assert_ne!(a, b);
        assert_eq!(table.byte_range(a), table.byte_range(b));
    }
}
