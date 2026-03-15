# TallyQL Query Language Reference

TallyQL is a filter expression language for querying findings stored by tally. It supports boolean operators, comparisons, string operations, existence checks, set membership, and date literals.

## Grammar (EBNF)

```ebnf
query           = filter_expr ;
filter_expr     = or_expr ;
or_expr         = and_expr , { ( "OR" | "||" ) , and_expr } ;
and_expr        = not_expr , { ( "AND" | "&&" ) , not_expr } ;
not_expr        = ( "NOT" | "!" ) , atom | atom ;
atom            = comparison | string_match | existence | in_list | "(" , filter_expr , ")" ;

comparison      = field , compare_op , value ;
string_match    = field , string_op , quoted_string ;
existence       = ( "HAS" | "MISSING" ) , field ;
in_list         = field , "IN" , "(" , value , { "," , value } , ")" ;

compare_op      = "=" | "!=" | ">" | "<" | ">=" | "<=" ;
string_op       = "CONTAINS" | "STARTSWITH" | "ENDSWITH" ;

field           = "severity" | "status" | "file" | "rule" | "title" | "description"
                | "suggested_fix" | "evidence" | "category" | "agent" | "tag"
                | "created_at" | "updated_at" ;

value           = quoted_string | duration | integer | enum_value ;
quoted_string   = '"' , { char - '"' | escape_seq } , '"' ;
duration        = digit , { digit } , ( "s" | "m" | "h" | "d" ) ;
integer         = [ "-" ] , digit , { digit } ;
enum_value      = identifier ;
```

## Operator Precedence

From lowest to highest:

1. `OR` / `||`
2. `AND` / `&&`
3. `NOT` / `!`
4. Atoms (comparisons, string ops, existence, IN, parenthesized)

Parentheses override precedence: `a AND (b OR c)`.

## Comparison Operators

| Operator | Meaning | Example |
|----------|---------|---------|
| `=` or `==` | Equal | `severity = critical` |
| `!=` | Not equal | `status != closed` |
| `>` | Greater than | `severity > important` |
| `<` | Less than | `severity < critical` |
| `>=` | Greater or equal | `created_at >= 7d` |
| `<=` | Less or equal | `severity <= important` |

## String Operators

All string operations are case-insensitive.

| Operator | Meaning | Example |
|----------|---------|---------|
| `CONTAINS` | Substring match | `title CONTAINS "unwrap"` |
| `STARTSWITH` | Prefix match | `file STARTSWITH "src/api"` |
| `ENDSWITH` | Suffix match | `file ENDSWITH ".rs"` |

## Boolean Operators

| Operator | Alias | Example |
|----------|-------|---------|
| `AND` | `&&` | `severity = critical AND status = open` |
| `OR` | `\|\|` | `severity = critical OR severity = important` |
| `NOT` | `!` | `NOT status = closed` |

All keywords are case-insensitive: `and`, `AND`, `And` all work.

## Existence Operators

| Operator | Meaning | Example |
|----------|---------|---------|
| `HAS` | Field is present/non-empty | `HAS suggested_fix` |
| `MISSING` | Field is absent/empty | `MISSING evidence` |

Semantics by field type:
- `Option<String>` fields (`suggested_fix`, `evidence`): HAS = is_some, MISSING = is_none
- `Vec` fields (`tag`, `agent`): HAS = non-empty, MISSING = empty
- `String` fields (`category`, `title`, etc.): HAS = non-empty string, MISSING = empty string
- Always-present fields (`severity`, `status`, `created_at`, `updated_at`): HAS = always true

## IN Lists

Test if a field value matches any value in a list:

```
severity IN (critical, important)
status IN (open, acknowledged, in_progress)
rule IN ("unsafe-unwrap", "sql-injection")
```

At least one value is required in the list.

## Fields

| Field | Type | Notes |
|-------|------|-------|
| `severity` | Ordered enum | critical > important > suggestion > tech_debt. Supports `>`, `<` comparison. |
| `status` | Enum | open, acknowledged, in_progress, resolved, false_positive, wont_fix, deferred, suppressed, reopened, closed. Equality only (no ordering). |
| `file` | String (multi) | Matches ANY location in the finding's locations array. |
| `rule` | String | Rule ID (e.g., "unsafe-unwrap"). Exact match with `=`. |
| `title` | String | Finding title. |
| `description` | String | Finding description. |
| `suggested_fix` | Optional string | May be absent. Use `HAS`/`MISSING`. |
| `evidence` | Optional string | May be absent. Use `HAS`/`MISSING`. |
| `category` | String | Category for grouping (e.g., "safety", "injection"). |
| `agent` | String (multi) | Matches ANY agent_id in discovered_by array. |
| `tag` | String (multi) | Matches ANY tag in the tags array. |
| `created_at` | DateTime | Supports duration (`7d`) and ISO 8601 (`"2026-03-01"`). |
| `updated_at` | DateTime | Same as created_at. |

Multi-value fields (`file`, `agent`, `tag`): `=` returns true if ANY value matches; `!=` returns true if NO value matches.

## Values

### Quoted Strings

```
"hello world"
"path/to/file.rs"
"2026-03-01T12:00:00Z"
```

Escape sequences: `\"`, `\\`, `\n`, `\t`.

### Enum Values (unquoted)

Severity and status values can be used without quotes:

```
severity = critical
status = open
```

### Duration Literals

Relative time durations for date comparisons:

| Suffix | Meaning | Example |
|--------|---------|---------|
| `s` | Seconds | `60s` |
| `m` | Minutes | `30m` |
| `h` | Hours | `24h` |
| `d` | Days | `7d` |

Duration values resolve to `now - duration` when comparing against DateTime fields.
`created_at > 7d` means "created more recently than 7 days ago".

### ISO 8601 Dates

Use quoted strings for absolute dates:

```
created_at > "2026-03-01"
created_at > "2026-03-01T12:00:00Z"
```

Date-only strings (`"2026-03-01"`) are treated as midnight UTC.

### Integers

```
created_at > -1
```

## Comments

Line comments are supported:

```
severity = critical   # only critical findings
status = open         // filter to open
```

Comments starting with `#` or `//` extend to end of line. Comments inside quoted strings are preserved.

## Limits

| Limit | Value | Reason |
|-------|-------|--------|
| Max query length | 8,192 bytes | DoS prevention (CWE-400) |
| Max nesting depth | 64 levels | Stack overflow prevention (CWE-674) |

## Examples

```bash
# Simple comparison
tally query --filter 'severity = critical'

# Boolean AND
tally query --filter 'severity = critical AND file CONTAINS "api"'

# Boolean OR
tally query --filter 'severity = critical OR severity = important'

# NOT operator
tally query --filter 'NOT status = closed'

# Parenthesized grouping
tally query --filter 'severity = critical AND (file CONTAINS "api" OR file CONTAINS "handler")'

# String operators
tally query --filter 'title CONTAINS "unwrap"'
tally query --filter 'file STARTSWITH "src/api"'
tally query --filter 'file ENDSWITH ".rs"'

# Existence checks
tally query --filter 'HAS suggested_fix'
tally query --filter 'MISSING evidence'

# IN lists
tally query --filter 'severity IN (critical, important)'
tally query --filter 'status IN (open, acknowledged, in_progress)'

# Date filtering with durations
tally query --filter 'created_at > 7d'
tally query --filter 'created_at > 24h'

# Date filtering with ISO 8601
tally query --filter 'created_at > "2026-03-01"'

# Complex expressions
tally query --filter 'severity IN (critical, important) AND file STARTSWITH "src/" AND NOT status = closed'
tally query --filter 'HAS suggested_fix AND tag CONTAINS "security" AND created_at > 30d'

# Combined with CLI flags
tally query --filter 'HAS suggested_fix' --sort severity --sort-dir desc --since 30d

# Operator aliases
tally query --filter 'severity == critical && !status = closed'
tally query --filter 'severity = critical || severity = important'
```

## MCP Usage

When using tally as an MCP server, pass TallyQL expressions via the `filter` parameter:

```json
{
  "tool": "query_findings",
  "arguments": {
    "filter": "severity = critical AND file CONTAINS \"api\"",
    "sort": "-severity",
    "limit": 10
  }
}
```

The `sort` parameter accepts a field name with optional `-` prefix for descending order.
