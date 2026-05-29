//! Runtime values produced by the executor.

#![allow(clippy::cast_precision_loss, clippy::match_same_arms)]

/// A single cell in a result row.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// SQL `NULL`.
    Null,
    /// Signed integer.
    Integer(i64),
    /// IEEE-754 float.
    Float(f64),
    /// UTF-8 text / bytes.
    Bytes(Vec<u8>),
    /// Boolean.
    Bool(bool),
    /// `DATE` (`YYYY-MM-DD` as UTF-8 bytes).
    Date(Vec<u8>),
    /// `TIMESTAMP` (Unix seconds, UTC).
    Timestamp(i64),
}

fn parse_bytes_as_f64(b: &[u8]) -> Option<f64> {
    std::str::from_utf8(b).ok()?.trim().parse().ok()
}

impl Value {
    /// Canonical byte representation for index keys / joins.
    #[must_use]
    pub fn as_bytes(&self) -> Vec<u8> {
        match self {
            Self::Null => Vec::new(),
            Self::Integer(n) => n.to_be_bytes().to_vec(),
            Self::Float(f) => f.to_be_bytes().to_vec(),
            Self::Bool(b) => vec![u8::from(*b)],
            Self::Bytes(b) => b.clone(),
            Self::Date(b) => b.clone(),
            Self::Timestamp(ts) => ts.to_be_bytes().to_vec(),
        }
    }

    /// Compare for equality with SQL `NULL` semantics (only `Null` equals `Null`).
    #[must_use]
    pub fn sql_eq(&self, other: &Self) -> Option<bool> {
        match (self, other) {
            (Self::Null, Self::Null) => Some(true),
            (Self::Null, _) | (_, Self::Null) => None,
            (Self::Integer(a), Self::Integer(b)) => Some(a == b),
            (Self::Float(a), Self::Float(b)) => Some(a == b),
            (Self::Bool(a), Self::Bool(b)) => Some(a == b),
            (Self::Bytes(a), Self::Bytes(b)) => Some(a == b),
            (Self::Date(a), Self::Date(b)) => Some(a == b),
            (Self::Timestamp(a), Self::Timestamp(b)) => Some(a == b),
            (Self::Integer(a), Self::Float(b)) => Some(*a as f64 == *b),
            (Self::Float(a), Self::Integer(b)) => Some(*a == *b as f64),
            (Self::Bytes(a), Self::Integer(b)) => parse_bytes_as_f64(a).map(|x| x == *b as f64),
            (Self::Integer(a), Self::Bytes(b)) => parse_bytes_as_f64(b).map(|x| *a as f64 == x),
            (Self::Bytes(a), Self::Float(b)) => parse_bytes_as_f64(a).map(|x| x == *b),
            (Self::Float(a), Self::Bytes(b)) => parse_bytes_as_f64(b).map(|x| *a == x),
            _ => Some(false),
        }
    }
}

/// One output row from an operator.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Record {
    /// Named columns (storage column names or aliases).
    pub fields: Vec<(String, Value)>,
}

impl Record {
    /// Values only, in field order.
    #[must_use]
    pub fn cells(&self) -> Vec<Value> {
        self.fields.iter().map(|(_, v)| v.clone()).collect()
    }
}
