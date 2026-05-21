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
}
