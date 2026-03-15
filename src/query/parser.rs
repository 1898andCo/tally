//! Chumsky 0.10 parser for `TallyQL` filter expressions.
//!
//! Entry point: [`parse_tallyql()`] with length/depth guards.
//!
//! # Case-Insensitive Keywords
//!
//! All keywords are case-insensitive. This is implemented via a `kw()` helper
//! that uses `text::ident()` with case-insensitive comparison — the idiomatic
//! Chumsky 0.10 pattern (zesterer/chumsky#699).
//!
//! # Thread Safety: `Rc<Cell>` vs `Arc<AtomicUsize>`
//!
//! The nesting depth counter uses `Rc<Cell<usize>>` (not `Arc<AtomicUsize>`).
//! `parse_tallyql()` is synchronous and stack-scoped — never held across
//! `.await` points. `Rc<Cell>` is faster because the compiler can coalesce
//! adjacent inc/dec operations.

use std::cell::Cell;
use std::rc::Rc;

use chumsky::prelude::*;

use crate::query::ast::{CompareOp, FilterExpr, StringOp, Value};
use crate::query::error::TallyQLError;
use crate::query::fields::validate_field;

/// Maximum query length in bytes to prevent parser-level `DoS` (`CWE-400`).
const MAX_QUERY_LENGTH: usize = 8192; // 8 KB

/// Maximum nesting depth for recursive expressions to prevent stack overflow (CWE-674).
const MAX_NESTING_DEPTH: usize = 64;

/// Case-insensitive keyword parser.
///
/// Uses `text::ident()` with case-insensitive comparison. `text::keyword()`
/// is case-sensitive by default in Chumsky 0.10 (zesterer/chumsky#699).
#[allow(clippy::elidable_lifetime_names)]
fn kw<'a>(keyword: &'a str) -> Boxed<'a, 'a, &'a str, (), extra::Err<Rich<'a, char>>> {
    text::ident()
        .try_map(move |ident: &str, span| {
            if ident.eq_ignore_ascii_case(keyword) {
                Ok(())
            } else {
                Err(Rich::custom(span, format!("expected keyword '{keyword}'")))
            }
        })
        .boxed()
}

/// Parse a quoted string with escape handling.
fn quoted_string<'a>() -> impl Parser<'a, &'a str, String, extra::Err<Rich<'a, char>>> {
    let known_escape = just('\\').ignore_then(choice((
        just('"').to('"'),
        just('\\').to('\\'),
        just('n').to('\n'),
        just('t').to('\t'),
    )));

    let unknown_escape = just('\\').then(none_of('"')).map(|(bs, c)| {
        let mut s = String::with_capacity(2);
        s.push(bs);
        s.push(c);
        s
    });

    let regular_char = none_of(['"', '\\']).map(|c: char| String::from(c));

    known_escape
        .map(String::from)
        .or(unknown_escape)
        .or(regular_char)
        .repeated()
        .collect::<Vec<String>>()
        .map(|parts| parts.join(""))
        .delimited_by(just('"'), just('"'))
}

/// Parse a non-negative integer with overflow protection (CWE-190).
fn integer<'a>() -> impl Parser<'a, &'a str, i64, extra::Err<Rich<'a, char>>> {
    text::int(10).try_map(|s: &str, span| {
        s.parse::<i64>()
            .map_err(|_| Rich::custom(span, format!("integer '{s}' is out of range")))
    })
}

/// Parse a signed integer: optional `-` prefix followed by digits.
fn signed_integer<'a>() -> impl Parser<'a, &'a str, Value, extra::Err<Rich<'a, char>>> {
    just('-')
        .or_not()
        .then(integer())
        .map(|(sign, val)| Value::Integer(if sign.is_some() { -val } else { val }))
}

/// Parse a relative duration: `7d`, `24h`, `30m`, `60s`.
///
/// Duration literals are unambiguous because field names cannot start with
/// digits (identifiers must start with a letter or underscore).
fn duration_parser<'a>() -> impl Parser<'a, &'a str, Value, extra::Err<Rich<'a, char>>> {
    text::int(10)
        .then(one_of("smhd"))
        .try_map(|(digits, unit): (&str, char), span| {
            let n = digits.parse::<u64>().map_err(|_| {
                Rich::custom(span, format!("duration value '{digits}' out of range"))
            })?;
            let duration = match unit {
                's' => std::time::Duration::from_secs(n),
                'm' => std::time::Duration::from_secs(n * 60),
                'h' => std::time::Duration::from_secs(n * 3600),
                'd' => std::time::Duration::from_secs(n * 86400),
                _ => unreachable!("one_of only matches s/m/h/d"),
            };
            Ok(Value::Duration(duration))
        })
}

/// Parse a value: quoted string, duration, signed integer, or unquoted enum.
///
/// Order matters: duration (most specific: number+unit) before integer
/// (less specific: bare number). Enum values (unquoted identifiers like
/// `critical`, `open`) are tried last.
fn value_parser<'a>() -> impl Parser<'a, &'a str, Value, extra::Err<Rich<'a, char>>> {
    let string_val = quoted_string().map(Value::String);

    // Unquoted enum value: identifiers that aren't keywords.
    // Severity/status values like critical, open, in_progress.
    let enum_val = text::ident()
        .map(|s: &str| Value::Enum(s.to_string()))
        .labelled("enum value");

    choice((string_val, duration_parser(), signed_integer(), enum_val)).labelled("value")
}

/// Parse a field name and validate it against known fields.
fn field_parser<'a>() -> impl Parser<'a, &'a str, String, extra::Err<Rich<'a, char>>> {
    // Field names: letter or underscore start, then alphanumeric/underscore
    let ident_start = any().filter(|c: &char| c.is_alphabetic() || *c == '_');
    let ident_rest = any()
        .filter(|c: &char| c.is_alphanumeric() || *c == '_')
        .repeated()
        .collect::<String>();

    ident_start
        .then(ident_rest)
        .map(|(first, rest)| format!("{first}{rest}"))
        .try_map(|name, span| {
            validate_field(&name).map_err(|msg| Rich::custom(span, msg))?;
            Ok(name)
        })
        .labelled("field name")
}

/// Parse comparison operators: `=`, `!=`, `>=`, `<=`, `>`, `<`.
fn compare_op<'a>() -> impl Parser<'a, &'a str, CompareOp, extra::Err<Rich<'a, char>>> {
    choice((
        just("!=").to(CompareOp::Ne),
        just(">=").to(CompareOp::GtEq),
        just("<=").to(CompareOp::LtEq),
        just("==").to(CompareOp::Eq),
        just('=').to(CompareOp::Eq),
        just('>').to(CompareOp::Gt),
        just('<').to(CompareOp::Lt),
    ))
    .labelled("comparison operator")
}

/// Parse string operators: CONTAINS, STARTSWITH, ENDSWITH.
fn string_op_parser<'a>() -> impl Parser<'a, &'a str, StringOp, extra::Err<Rich<'a, char>>> {
    text::ident()
        .try_map(|s: &str, span| match s.to_ascii_uppercase().as_str() {
            "CONTAINS" => Ok(StringOp::Contains),
            "STARTSWITH" => Ok(StringOp::StartsWith),
            "ENDSWITH" => Ok(StringOp::EndsWith),
            _ => Err(Rich::custom(
                span,
                format!("expected string operator, found '{s}'"),
            )),
        })
        .labelled("string operator")
}

/// Build the recursive filter expression parser.
///
/// Operator precedence (lowest to highest):
/// 1. OR  / ||
/// 2. AND / &&
/// 3. NOT / !
/// 4. Atoms: comparison, string match, HAS/MISSING, IN list, parenthesized
fn filter_parser<'a>() -> impl Parser<'a, &'a str, FilterExpr, extra::Err<Rich<'a, char>>> {
    let depth = Rc::new(Cell::new(0usize));
    recursive(move |expr| {
        // --- Atom parsers ---

        // HAS field / MISSING field
        let has = kw("HAS")
            .padded()
            .ignore_then(field_parser())
            .map(FilterExpr::Has);

        let missing = kw("MISSING")
            .padded()
            .ignore_then(field_parser())
            .map(FilterExpr::Missing);

        // field IN (val1, val2, ...)
        let in_list = field_parser()
            .then_ignore(kw("IN").padded())
            .then(
                value_parser()
                    .separated_by(just(',').padded())
                    .collect::<Vec<_>>()
                    .delimited_by(just('(').padded(), just(')').padded()),
            )
            .map(|(field, values)| FilterExpr::InList { field, values });

        // field CONTAINS/STARTSWITH/ENDSWITH "value"
        let string_op = field_parser()
            .then(string_op_parser().padded())
            .then(quoted_string())
            .map(|((field, op), value)| FilterExpr::StringMatch { field, op, value });

        // field op value (comparison)
        let field_cmp = field_parser()
            .then(compare_op().padded())
            .then(value_parser())
            .map(|((field, op), value)| FilterExpr::Comparison { field, op, value });

        // Parenthesized expression with depth guard
        let depth_open = Rc::clone(&depth);
        let depth_close = Rc::clone(&depth);
        let depth_guarded_paren = just('(')
            .padded()
            .try_map(move |_, span| {
                let d = depth_open.get() + 1;
                if d > MAX_NESTING_DEPTH {
                    Err(Rich::custom(
                        span,
                        format!("expression nesting exceeds maximum depth of {MAX_NESTING_DEPTH}"),
                    ))
                } else {
                    depth_open.set(d);
                    Ok(())
                }
            })
            .ignore_then(expr.clone())
            .then_ignore(just(')').padded().map(move |_| {
                depth_close.set(depth_close.get().saturating_sub(1));
            }));

        // Atom: use choice() for compile time (not or() chains)
        let atom = choice((
            has,
            missing,
            in_list,
            string_op,
            field_cmp,
            depth_guarded_paren,
        ))
        .boxed();

        // --- Precedence layers ---

        // NOT / !
        let not = choice((kw("NOT").to(()), just('!').to(())))
            .padded()
            .ignore_then(atom.clone())
            .map(|e| FilterExpr::Not(Box::new(e)))
            .or(atom);

        // AND / &&
        let and = not.clone().foldl(
            choice((kw("AND").to(()), just("&&").to(())))
                .padded()
                .ignore_then(not)
                .repeated(),
            |left, right| FilterExpr::And(Box::new(left), Box::new(right)),
        );

        // OR / ||
        and.clone().foldl(
            choice((kw("OR").to(()), just("||").to(())))
                .padded()
                .ignore_then(and)
                .repeated(),
            |left, right| FilterExpr::Or(Box::new(left), Box::new(right)),
        )
    })
    .labelled("filter expression")
}

/// Strip line comments (`//` and `#`) from input, replacing comment
/// content with spaces to preserve byte offsets for error span reporting.
fn strip_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            result.push(c);
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    result.push(escaped);
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            result.push(c);
        } else if c == '/' && chars.peek() == Some(&'/') {
            result.push(' ');
            chars.next();
            result.push(' ');
            for remaining in chars.by_ref() {
                if remaining == '\n' {
                    result.push('\n');
                    break;
                }
                result.push(' ');
            }
        } else if c == '#' {
            result.push(' ');
            for remaining in chars.by_ref() {
                if remaining == '\n' {
                    result.push('\n');
                    break;
                }
                result.push(' ');
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Parse a `TallyQL` filter expression string into an AST.
///
/// Returns `Ok(FilterExpr)` on success, or a list of parse errors.
///
/// # Errors
///
/// Returns errors for:
/// - Empty input
/// - Input exceeding 8 KB (CWE-400)
/// - Nesting exceeding depth 64 (CWE-674)
/// - Syntax errors with span, expected, found, and optional hint
pub fn parse_tallyql(input: &str) -> std::result::Result<FilterExpr, Vec<TallyQLError>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(vec![TallyQLError::unexpected_eof(
            0..0,
            "expected query expression",
        )]);
    }

    if trimmed.len() > MAX_QUERY_LENGTH {
        return Err(vec![TallyQLError::unexpected_token(
            0..0,
            "query within 8KB limit",
            format!("found {} bytes", trimmed.len()),
        )]);
    }

    let stripped = strip_comments(trimmed);
    let parser = filter_parser().padded().then_ignore(end());
    let (output, errs) = parser.parse(stripped.as_str()).into_output_errors();

    if let Some(expr) = output {
        if errs.is_empty() {
            return Ok(expr);
        }
    }

    let errors: Vec<TallyQLError> = errs
        .into_iter()
        .map(|e| {
            let span = e.span().into_range();
            let expected = format!(
                "expected {}",
                e.expected()
                    .map(|x| format!("{x}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            match e.found() {
                Some(c) => TallyQLError::unexpected_token(span, expected, format!("'{c}'")),
                None => TallyQLError::unexpected_eof(span, expected),
            }
        })
        .collect();

    Err(errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_comments_removes_hash() {
        let input = "severity = critical # filter by sev";
        let result = strip_comments(input);
        assert!(result.starts_with("severity = critical "));
        assert!(!result.contains('#'));
    }

    #[test]
    fn strip_comments_removes_double_slash() {
        let input = "status = open // only open ones";
        let result = strip_comments(input);
        assert!(result.starts_with("status = open "));
        assert!(!result.contains("//"));
    }

    #[test]
    fn strip_comments_preserves_string_content() {
        let input = r#"title CONTAINS "hello # world""#;
        let result = strip_comments(input);
        assert!(result.contains("hello # world"));
    }

    #[test]
    fn strip_comments_preserves_offsets() {
        let input = "abc # comment\nxyz";
        let result = strip_comments(input);
        assert_eq!(result.len(), input.len());
    }
}
