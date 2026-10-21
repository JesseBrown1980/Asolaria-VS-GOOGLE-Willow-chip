//! Graph-runtime events as HBP text rows: the fields `extractNodeFeatures` reads
//! (vendor/legacy/gnnDataExport.js:29-49).

use alloc::{borrow::ToOwned, string::String, vec::Vec};
use core::fmt;

/// One graph-runtime event. Empty text fields are JavaScript-falsy and parse to `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Event {
    /// `event.actor?.id`.
    pub actor_id: Option<String>,
    /// `event.actor?.type`.
    pub actor_type: Option<String>,
    /// `event.target?.id`.
    pub target_id: Option<String>,
    /// `event.target?.type`.
    pub target_type: Option<String>,
    /// `Number(event.risk?.score || 0)`.
    pub risk_score: u32,
    /// `event.status`.
    pub status: Option<String>,
}

/// Largest `risk=` value the parser accepts.
pub const MAX_RISK_SCORE: u32 = 1_000_000;
/// Largest number of `EVENT` rows the parser accepts.
pub const MAX_EVENTS: usize = 1_000_000;

/// A refused row; `line` is 1-based and counts comment and blank lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventError {
    /// The row does not start with the field `EVENT`.
    MissingPrefix { line: usize },
    /// A field has no `=`.
    MissingEquals { line: usize },
    /// A key outside `actor actor_type target target_type risk status`.
    UnknownKey { line: usize, key: String },
    /// A key given twice in one row.
    DuplicateKey { line: usize, key: String },
    /// A `%` not followed by two uppercase hex digits.
    BadEscape { line: usize },
    /// A raw byte below 0x20 or 0x7F inside a value.
    UnescapedControlByte { line: usize },
    /// A value that is not UTF-8 after decoding.
    InvalidUtf8 { line: usize },
    /// A `risk=` value that is empty or not ASCII decimal digits.
    RiskNotDecimal { line: usize },
    /// A `risk=` value above [`MAX_RISK_SCORE`].
    RiskAboveLimit { line: usize },
    /// The row holding event number `MAX_EVENTS + 1`.
    TooManyEvents { line: usize },
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventError::MissingPrefix { line } => {
                write!(f, "line {line}: row must start with EVENT")
            }
            EventError::MissingEquals { line } => write!(f, "line {line}: field without '='"),
            EventError::UnknownKey { line, key } => write!(f, "line {line}: unknown key {key:?}"),
            EventError::DuplicateKey { line, key } => {
                write!(f, "line {line}: duplicate key {key:?}")
            }
            EventError::BadEscape { line } => {
                write!(
                    f,
                    "line {line}: '%' must be followed by two uppercase hex digits"
                )
            }
            EventError::UnescapedControlByte { line } => {
                write!(
                    f,
                    "line {line}: control byte in a value must be percent-escaped"
                )
            }
            EventError::InvalidUtf8 { line } => {
                write!(f, "line {line}: value is not UTF-8 after decoding")
            }
            EventError::RiskNotDecimal { line } => {
                write!(f, "line {line}: risk must be decimal digits")
            }
            EventError::RiskAboveLimit { line } => {
                write!(f, "line {line}: risk above {MAX_RISK_SCORE}")
            }
            EventError::TooManyEvents { line } => {
                write!(f, "line {line}: more than {MAX_EVENTS} events")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EventError {}

const KEYS: [&str; 6] = [
    "actor",
    "actor_type",
    "target",
    "target_type",
    "risk",
    "status",
];

fn is_event_row(row: &str) -> bool {
    !row.is_empty() && !row.starts_with('#')
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_value(raw: &str, line: usize) -> Result<String, EventError> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte < 0x20 || byte == 0x7F {
            return Err(EventError::UnescapedControlByte { line });
        }
        if byte != b'%' {
            out.push(byte);
            i += 1;
            continue;
        }
        let high = bytes.get(i + 1).copied().and_then(hex_digit);
        let low = bytes.get(i + 2).copied().and_then(hex_digit);
        match (high, low) {
            (Some(high), Some(low)) => out.push(high * 16 + low),
            _ => return Err(EventError::BadEscape { line }),
        }
        i += 3;
    }
    String::from_utf8(out).map_err(|_| EventError::InvalidUtf8 { line })
}

fn parse_risk(value: &str, line: usize) -> Result<u32, EventError> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(EventError::RiskNotDecimal { line });
    }
    let mut risk: u32 = 0;
    for digit in value.bytes() {
        risk = risk
            .checked_mul(10)
            .and_then(|r| r.checked_add(u32::from(digit - b'0')))
            .filter(|&r| r <= MAX_RISK_SCORE)
            .ok_or(EventError::RiskAboveLimit { line })?;
    }
    Ok(risk)
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn parse_row(row: &str, line: usize) -> Result<Event, EventError> {
    let mut fields = row.split('|');
    if fields.next() != Some("EVENT") {
        return Err(EventError::MissingPrefix { line });
    }
    let mut seen = [false; KEYS.len()];
    let mut event = Event::default();
    for field in fields {
        let (key, raw) = field
            .split_once('=')
            .ok_or(EventError::MissingEquals { line })?;
        let slot = KEYS
            .iter()
            .position(|&k| k == key)
            .ok_or_else(|| EventError::UnknownKey {
                line,
                key: key.to_owned(),
            })?;
        if seen[slot] {
            return Err(EventError::DuplicateKey {
                line,
                key: key.to_owned(),
            });
        }
        seen[slot] = true;
        let value = decode_value(raw, line)?;
        match key {
            "actor" => event.actor_id = non_empty(value),
            "actor_type" => event.actor_type = non_empty(value),
            "target" => event.target_id = non_empty(value),
            "target_type" => event.target_type = non_empty(value),
            "risk" => event.risk_score = parse_risk(&value, line)?,
            _ => event.status = non_empty(value),
        }
    }
    Ok(event)
}

/// Parses `EVENT|actor=..|actor_type=..|target=..|target_type=..|risk=..|status=..` rows
/// (every key optional, values percent-escaped); `#` comment and empty lines are skipped.
pub fn parse_events(text: &str) -> Result<Vec<Event>, EventError> {
    let mut count = 0usize;
    for (index, row) in text.split('\n').enumerate() {
        if is_event_row(row) {
            count += 1;
            if count > MAX_EVENTS {
                return Err(EventError::TooManyEvents { line: index + 1 });
            }
        }
    }
    let mut events = Vec::with_capacity(count);
    for (index, row) in text.split('\n').enumerate() {
        if is_event_row(row) {
            events.push(parse_row(row, index + 1)?);
        }
    }
    Ok(events)
}
