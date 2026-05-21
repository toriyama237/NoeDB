//! Fast scalar scanners (8-byte unrolled loops; LLVM often vectorizes these).

/// Skip ASCII whitespace starting at `pos`; returns new position.
#[inline]
pub(crate) fn skip_whitespace(bytes: &[u8], mut pos: usize) -> usize {
    let len = bytes.len();
    while pos + 8 <= len {
        let b0 = bytes[pos];
        let b1 = bytes[pos + 1];
        let b2 = bytes[pos + 2];
        let b3 = bytes[pos + 3];
        let b4 = bytes[pos + 4];
        let b5 = bytes[pos + 5];
        let b6 = bytes[pos + 6];
        let b7 = bytes[pos + 7];
        if !(crate::ascii_lut::is_whitespace(b0)
            && crate::ascii_lut::is_whitespace(b1)
            && crate::ascii_lut::is_whitespace(b2)
            && crate::ascii_lut::is_whitespace(b3)
            && crate::ascii_lut::is_whitespace(b4)
            && crate::ascii_lut::is_whitespace(b5)
            && crate::ascii_lut::is_whitespace(b6)
            && crate::ascii_lut::is_whitespace(b7))
        {
            break;
        }
        pos += 8;
    }
    while pos < len && crate::ascii_lut::is_whitespace(bytes[pos]) {
        pos += 1;
    }
    pos
}
