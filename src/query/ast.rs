//! `TallyQL` AST types.
//!
//! All filter expressions parse into this shared AST, which is then
//! evaluated against `Finding` instances by the evaluator.

use std::fmt;
use std::time::Duration;

/// Filter expression — boolean tree of field predicates.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FilterExpr {
    /// Field comparison: `severity = critical`, `created_at > 7d`.
    Comparison {
        field: String,
        op: CompareOp,
        value: Value,
    },
    /// Logical AND: `a AND b`.
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical OR: `a OR b`.
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// Logical NOT: `NOT a`.
    Not(Box<FilterExpr>),
    /// Field existence: `HAS suggested_fix`.
    Has(String),
    /// Field absence: `MISSING evidence`.
    Missing(String),
    /// String match: `title CONTAINS "unwrap"`.
    StringMatch {
        field: String,
        op: StringOp,
        value: String,
    },
    /// Set membership: `severity IN (critical, important)`.
    InList { field: String, values: Vec<Value> },
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Lt,
    GtEq,
    LtEq,
}

impl fmt::Display for CompareOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eq => write!(f, "="),
            Self::Ne => write!(f, "!="),
            Self::Gt => write!(f, ">"),
            Self::Lt => write!(f, "<"),
            Self::GtEq => write!(f, ">="),
            Self::LtEq => write!(f, "<="),
        }
    }
}

/// String match operators (case-insensitive evaluation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringOp {
    Contains,
    StartsWith,
    EndsWith,
}

impl fmt::Display for StringOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contains => write!(f, "CONTAINS"),
            Self::StartsWith => write!(f, "STARTSWITH"),
            Self::EndsWith => write!(f, "ENDSWITH"),
        }
    }
}

/// Query value — typed literal from the expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Quoted string literal: `"hello"`.
    String(String),
    /// Integer literal: `42`, `-1`.
    Integer(i64),
    /// Relative duration: `7d`, `24h`, `30m`.
    Duration(Duration),
    /// Unquoted enum value for severity/status: `critical`, `open`.
    Enum(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => write!(f, "\"{s}\""),
            Self::Integer(n) => write!(f, "{n}"),
            Self::Duration(d) => write!(f, "{}s", d.as_secs()),
            Self::Enum(s) => write!(f, "{s}"),
        }
    }
}

/// Sort specification for ordering results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortSpec {
    /// Field name to sort by.
    pub field: String,
    /// Whether to sort in descending order.
    pub descending: bool,
}
