//! Fast scalar scanners (LUT-backed; tuned for single-space SQL).

/// Skip ASCII whitespace starting at `pos`; returns new position.
#[inline]
pub(crate) fn skip_whitespace(bytes: &[u8], mut pos: usize) -> usize {
    let len = bytes.len();
    while pos < len {
        let b = bytes[pos];
        if !crate::ascii_lut::is_whitespace(b) {
            break;
        }
        pos += 1;
    }
    pos
}
