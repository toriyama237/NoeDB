//! Storage engine trait — the contract every backend must satisfy.

/// Key-value storage backend (MemTable today, LSM tree in Week 16).
pub trait StorageEngine {
    /// Error type returned by engine operations.
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Fetch the value for `key`, or `None` if missing.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] when the key violates engine invariants.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Insert or overwrite `key` with `value`.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] when the key violates engine invariants.
    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;

    /// Delete `key` if present.
    ///
    /// Returns `true` when a value was removed.
    ///
    /// # Errors
    ///
    /// Returns [`Self::Error`] when the key violates engine invariants.
    fn delete(&mut self, key: &[u8]) -> Result<bool, Self::Error>;

    /// Iterate all live entries in ascending lexicographic key order.
    fn iter(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_;

    /// Iterate live entries whose keys fall in `[start, end)`.
    ///
    /// The default implementation filters [`StorageEngine::iter`]; backends
    /// override it with bounded scans that avoid touching unrelated keys.
    fn range<'a>(
        &'a self,
        start: &'a [u8],
        end: &'a [u8],
    ) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a {
        self.iter()
            .skip_while(move |(k, _)| k.as_slice() < start)
            .take_while(move |(k, _)| k.as_slice() < end)
    }
}

/// Exclusive upper bound covering every key that starts with `prefix`.
///
/// Increments the last non-`0xFF` byte; an all-`0xFF` prefix yields an empty
/// vector, which callers must treat as "unbounded".
#[must_use]
pub fn prefix_end(prefix: &[u8]) -> Vec<u8> {
    let mut end = prefix.to_vec();
    while let Some(b) = end.last_mut() {
        if *b == 255 {
            end.pop();
        } else {
            *b += 1;
            return end;
        }
    }
    end
}
