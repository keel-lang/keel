//! UTF-8 byte offset ↔ LSP line/character position conversion.

use crate::lexer::Span;
use tower_lsp::lsp_types::{Position, Range};

/// Convert an LSP `Position` (0-based line + UTF-8 column approximation)
/// into a UTF-8 byte offset into `text`.
pub(crate) fn position_to_offset(text: &str, pos: Position) -> usize {
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    let mut offset: usize = 0;
    for ch in text.chars() {
        if line == pos.line && col == pos.character {
            return offset;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        offset += ch.len_utf8();
    }
    offset
}

pub(crate) fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    let mut i = 0;
    for ch in text.chars() {
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
    Position {
        line,
        character: col,
    }
}

/// Convert a byte-offset range to LSP `Range` (0-based line + UTF-16
/// column). v0.1 approximates column as UTF-8 character count — fine
/// for ASCII sources; a follow-up can add true UTF-16 code-unit
/// counting for emoji-dense files.
pub(crate) fn byte_range_to_lsp(text: &str, span: &Span) -> Range {
    Range {
        start: offset_to_position(text, span.start),
        end: offset_to_position(text, span.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── position_to_offset ──────────────────────────────────────────

    #[test]
    fn position_to_offset_start_of_text() {
        let text = "hello world";
        let pos = Position {
            line: 0,
            character: 0,
        };
        assert_eq!(position_to_offset(text, pos), 0);
    }

    #[test]
    fn position_to_offset_middle_of_line() {
        let text = "hello world";
        let pos = Position {
            line: 0,
            character: 6,
        };
        assert_eq!(position_to_offset(text, pos), 6);
    }

    #[test]
    fn position_to_offset_second_line() {
        let text = "first\nsecond\nthird";
        // "second" starts at byte offset 6 (f,i,r,s,t,\n = 6)
        let pos = Position {
            line: 1,
            character: 3,
        };
        // "sec" → bytes: s=6, e=7, c=8 → offset 9
        assert_eq!(position_to_offset(text, pos), 9);
    }

    #[test]
    fn position_to_offset_past_end_returns_text_length() {
        let text = "abc";
        let pos = Position {
            line: 0,
            character: 100,
        };
        assert_eq!(position_to_offset(text, pos), 3);
    }

    #[test]
    fn position_to_offset_past_end_line() {
        let text = "abc\ndef";
        let pos = Position {
            line: 10,
            character: 0,
        };
        assert_eq!(position_to_offset(text, pos), 7); // "abc\ndef" = 7 bytes
    }

    #[test]
    fn position_to_offset_at_newline() {
        let text = "abc\ndef";
        // Position at line 0, character 3 is right after "abc" at the \n
        // But position_to_offset iterates chars; after 'c' char, col=3, then \n resets col to 0
        // So position (0, 3) resolves to byte offset 3 (the \n)
        let pos = Position {
            line: 0,
            character: 3,
        };
        assert_eq!(position_to_offset(text, pos), 3);
    }

    #[test]
    fn position_to_offset_unicode_multibyte() {
        let text = "héllo";
        // 'h' = 1 byte, 'é' = 2 bytes, 'l' = 1 byte
        // character 0: h (offset 0)
        // character 1: é (offset 1, 2 bytes)
        // character 2: l (offset 3)
        let pos = Position {
            line: 0,
            character: 2,
        };
        assert_eq!(position_to_offset(text, pos), 3);
    }

    // ── offset_to_position ──────────────────────────────────────────

    #[test]
    fn offset_to_position_zero() {
        let pos = offset_to_position("hello", 0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn offset_to_position_middle() {
        let pos = offset_to_position("hello", 3);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn offset_to_position_second_line() {
        let text = "abc\ndef";
        // offset 4 = 'd' (line 1, col 0)
        let pos = offset_to_position(text, 4);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn offset_to_position_second_line_middle() {
        let text = "abc\ndef";
        // offset 5 = 'e', offset 6 = 'f'
        let pos = offset_to_position(text, 5);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 1);
    }

    #[test]
    fn offset_to_position_past_end() {
        let pos = offset_to_position("abc", 100);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn offset_to_position_at_newline_byte() {
        // "abc\ndef" → offset 3 is the \n byte
        // The loop increments i after processing \n; at offset 3, i starts at 3
        // First char processed: \n (i=0+1=1), then e,f → no break because i was checked
        // Actually: i=0, ch='a', i+=1 → i=1. ch='b', i=2. ch='c', i=3.
        // Next: ch='\n', i=3 >= offset=3, break!
        // So line=0, col=3 (the \n itself)
        let pos = offset_to_position("abc\ndef", 3);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);
    }

    // ── byte_range_to_lsp ───────────────────────────────────────────

    #[test]
    fn byte_range_to_lsp_simple() {
        let range = byte_range_to_lsp("hello world", &(0..5));
        assert_eq!(
            range.start,
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            range.end,
            Position {
                line: 0,
                character: 5
            }
        );
    }

    #[test]
    fn byte_range_to_lsp_multiline() {
        let text = "line1\nline2\nline3";
        // "line2" is at offset 6..11
        let range = byte_range_to_lsp(text, &(6..11));
        assert_eq!(
            range.start,
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            range.end,
            Position {
                line: 1,
                character: 5
            }
        );
    }
}
