//! `TallyQL` — query language for filtering findings.
//!
//! Provides a filter expression language with boolean operators (AND, OR, NOT),
//! comparisons, string operations, date literals, and existence checks.
//! Expressions are parsed by a Chumsky 0.10 recursive descent parser and
//! evaluated in-memory against `Vec<Finding>`.

pub mod ast;
pub mod error;
pub mod eval;
pub mod fields;
pub mod parser;

pub use ast::{CompareOp, FilterExpr, SortSpec, StringOp, Value};
pub use error::TallyQLError;
pub use eval::{apply_filters, apply_sort, evaluate};
pub use fields::{FieldType, KNOWN_FIELDS, field_type, validate_field};
pub use parser::parse_tallyql;
