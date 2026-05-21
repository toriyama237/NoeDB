//! Runtime values produced by the executor.

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
