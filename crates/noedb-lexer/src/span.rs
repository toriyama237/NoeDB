//! Byte-precise source positions.
//!
//! A [`Span`] is a half-open byte range `[start, end)` into the original
//! SQL source string. Spans are intentionally **byte offsets**, not
//! `(line, column)` pairs:
//!
//! - Token-level operations (parser lookahead, AST construction) only
//!   care about ordering and length, which `u32` offsets answer in O(1).
//! - Line/column display is a presentation concern. It is computed *once*,
//!   on demand, by [`SourceMap`].
//!
//! `u32` is plenty: SQL inputs over 4 GiB are not a thing we want to
//! support, and shrinking from `usize` halves the size of every token on
//! 64-bit systems.

/// A half-open byte range `[start, end)` into some source string.
///
/// Spans are cheap (`Copy`, 8 bytes on every platform) and produced by
/// the lexer for every token, plus on every error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Inclusive byte offset of the first character of the span.
    pub start: u32,
    /// Exclusive byte offset of the byte one past the last character.
    pub end: u32,
}

impl Span {
    /// Construct a span from explicit byte offsets.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `end < start`. Release builds silently
    /// accept the inverted range; callers should not rely on this.
    ///
    /// # Examples
    ///
    /// ```
    /// use noedb_lexer::Span;
    ///
    /// let s = Span::new(0, 6);
    /// assert_eq!(s.len(), 6);
    /// assert!(!s.is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        debug_assert!(end >= start, "Span::new: end must be >= start");
        Self { start, end }
    }

    /// A zero-width span anchored at `offset` (used for EOF and synthetic
    /// tokens).
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `offset` does not fit in `u32`.
    #[inline]
    #[must_use]
    pub const fn empty_at(offset: usize) -> Self {
        debug_assert!(
            offset <= u32::MAX as usize,
            "Span::empty_at: offset > u32::MAX"
        );
        #[allow(clippy::cast_possible_truncation)]
        let o = offset as u32;
        Self { start: o, end: o }
    }

    /// Byte length of the span.
    #[inline]
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    /// `true` if the span contains zero bytes (e.g. EOF).
    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Slice `src` with this span.
    ///
    /// Returns `None` if the span is out of bounds for `src`, or if it
    /// would split a UTF-8 code point. Callers that hold the original
    /// source string can use this to recover the lexeme for diagnostics.
    ///
    /// # Examples
    ///
    /// ```
    /// use noedb_lexer::Span;
    ///
    /// let s = "SELECT 42";
    /// let span = Span::new(7, 9);
    /// assert_eq!(span.slice(s), Some("42"));
    /// ```
    #[inline]
    #[must_use]
    pub fn slice(self, src: &str) -> Option<&str> {
        src.get(self.start as usize..self.end as usize)
    }
}

/// A one-indexed `(line, column)` pair, suitable for human display.
///
/// Columns are counted in **UTF-8 byte offsets from the start of the
/// line**, not in code points or graphemes. This matches what most
/// terminals and editors report and is what `rustc`'s own diagnostics
/// use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineColumn {
    /// One-indexed line number.
    pub line: u32,
    /// One-indexed byte column within the line.
    pub column: u32,
}

/// Lazy `byte-offset -> (line, column)` resolver for one source string.
///
/// `SourceMap` precomputes a sorted index of newline byte offsets *once*,
/// then answers any number of queries in O(log n). Use it from the
/// reporting layer only - the parser and planner never need it.
///
/// # Examples
///
/// ```
/// use noedb_lexer::{LineColumn, SourceMap};
///
/// let src = "SELECT 1\nSELECT 2";
/// let map = SourceMap::new(src);
///
/// assert_eq!(map.line_column(0),  LineColumn { line: 1, column: 1 });
/// assert_eq!(map.line_column(9),  LineColumn { line: 2, column: 1 });
/// assert_eq!(map.line_column(16), LineColumn { line: 2, column: 8 });
/// ```
#[derive(Debug, Clone)]
pub struct SourceMap {
    /// Byte length of the source string (used to clamp queries).
    src_len: u32,
    /// Byte offset of each `\n`, in increasing order.
    newlines: Vec<u32>,
}

impl SourceMap {
    /// Build a `SourceMap` over `src`.
    ///
    /// O(n) in the size of `src`, executed once.
    ///
    /// # Panics
    ///
    /// In debug builds, panics if `src.len()` does not fit in `u32`.
    #[must_use]
    pub fn new(src: &str) -> Self {
        debug_assert!(u32::try_from(src.len()).is_ok(), "SourceMap: src > 4 GiB");
        #[allow(clippy::cast_possible_truncation)]
        let src_len = src.len() as u32;
        let newlines = src
            .bytes()
            .enumerate()
            .filter_map(|(i, b)| {
                if b == b'\n' {
                    #[allow(clippy::cast_possible_truncation)]
                    Some(i as u32)
                } else {
                    None
                }
            })
            .collect();
        Self { src_len, newlines }
    }

    /// Translate a byte offset into a `(line, column)` pair.
    ///
    /// Offsets past the end of the source are clamped to the end.
    /// O(log n) in the number of newlines.
    #[must_use]
    pub fn line_column(&self, byte_offset: u32) -> LineColumn {
        let offset = byte_offset.min(self.src_len);
        match self.newlines.binary_search(&offset) {
            Ok(idx) | Err(idx) => {
                let line_start = if idx == 0 {
                    0
                } else {
                    self.newlines[idx - 1] + 1
                };
                LineColumn {
                    line: u32::try_from(idx + 1).unwrap_or(u32::MAX),
                    column: offset - line_start + 1,
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn span_basics() {
        let s = Span::new(3, 9);
        assert_eq!(s.len(), 6);
        assert!(!s.is_empty());
        let src = "0123456789";
        assert_eq!(s.slice(src), Some("345678"));
        assert!(Span::empty_at(3).is_empty());
    }

    #[test]
    fn source_map_single_line() {
        let map = SourceMap::new("SELECT 1");
        assert_eq!(map.line_column(0), LineColumn { line: 1, column: 1 });
        assert_eq!(map.line_column(7), LineColumn { line: 1, column: 8 });
    }

    #[test]
    fn source_map_handles_newlines() {
        let map = SourceMap::new("a\nbb\nccc");
        assert_eq!(map.line_column(0), LineColumn { line: 1, column: 1 });
        assert_eq!(map.line_column(1), LineColumn { line: 1, column: 2 });
        assert_eq!(map.line_column(2), LineColumn { line: 2, column: 1 });
        assert_eq!(map.line_column(4), LineColumn { line: 2, column: 3 });
        assert_eq!(map.line_column(5), LineColumn { line: 3, column: 1 });
    }

    #[test]
    fn source_map_clamps_out_of_bounds() {
        let map = SourceMap::new("ab");
        assert_eq!(map.line_column(100), LineColumn { line: 1, column: 3 });
    }
}
