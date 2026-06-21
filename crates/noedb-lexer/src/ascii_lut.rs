//! 256-byte lookup tables for ASCII byte classification (hot path).

#![allow(clippy::cast_possible_truncation)]

/// `1` when `b` is ASCII whitespace (` `, `\t`, `\n`, `\r`, `\x0B`, `\x0C`).
pub(crate) const WHITESPACE: [u8; 256] = lut_whitespace();

/// `1` when `b` can continue an identifier (`[A-Za-z0-9_]`).
#[allow(dead_code)]
pub(crate) const IDENT_CONTINUE: [u8; 256] = lut_ident_continue();

/// `1` when `b` can start an identifier (`[A-Za-z_]`).
pub(crate) const IDENT_START: [u8; 256] = lut_ident_start();

/// `1` when `b` is `A`–`Z` or `_` (fast keyword upper-case check).
pub(crate) const UPPER_OR_UNDERSCORE: [u8; 256] = lut_upper_or_underscore();

const fn lut_upper_or_underscore() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        if b.is_ascii_uppercase() || b == b'_' {
            t[i] = 1;
        }
        i += 1;
    }
    t
}

#[inline]
pub(crate) fn is_upper_keyword_body(bytes: &[u8]) -> bool {
    for &b in bytes {
        if UPPER_OR_UNDERSCORE[b as usize] == 0 {
            return false;
        }
    }
    true
}

const fn lut_whitespace() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        if matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c') {
            t[i] = 1;
        }
        i += 1;
    }
    t
}

#[allow(dead_code)]
const fn lut_ident_continue() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        if b.is_ascii_alphanumeric() || b == b'_' {
            t[i] = 1;
        }
        i += 1;
    }
    t
}

const fn lut_ident_start() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        if b.is_ascii_alphabetic() || b == b'_' {
            t[i] = 1;
        }
        i += 1;
    }
    t
}

#[inline]
pub(crate) const fn is_whitespace(b: u8) -> bool {
    WHITESPACE[b as usize] != 0
}

#[inline]
#[allow(dead_code)]
pub(crate) const fn is_ident_continue(b: u8) -> bool {
    IDENT_CONTINUE[b as usize] != 0
}

#[inline]
pub(crate) const fn is_ident_start(b: u8) -> bool {
    IDENT_START[b as usize] != 0
}
