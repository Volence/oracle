//! Type conventions (`protocol.md` D9): **addresses and byte payloads are hex strings**
//! (`"0x00FFA144"`, `"0x4E71…"`); counts, lengths, slot indices, line numbers and enables are JSON
//! numbers/booleans.
//!
//! Parsing is deliberately **strict about the prefix**. A bare `"1000"` is ambiguous — decimal 1000 and
//! hex 0x1000 are both plausible readings, and an emulator that guesses produces a confidently wrong
//! answer at an address 3 KiB from where the caller meant. So an address must carry `0x` or `$`, and a
//! number where a hex string belongs is refused loudly rather than coerced (the house rule: refuse loud,
//! never clamp or guess).

use crate::rpc::RpcError;
use serde_json::Value;

/// An address, in the spec's spelling: `0x` + 8 uppercase hex digits.
pub fn addr(v: u32) -> String {
    format!("0x{v:08X}")
}

/// A 16-bit value (SR, a VDP register word), `0x` + 4 uppercase hex digits.
pub fn u16_hex(v: u16) -> String {
    format!("0x{v:04X}")
}

/// A byte payload as one hex string. Empty input yields `"0x"` — a valid zero-length payload, not `null`.
pub fn bytes(data: &[u8]) -> String {
    let mut s = String::with_capacity(2 + data.len() * 2);
    s.push_str("0x");
    for b in data {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// Parse a hex-string address. Accepts `0x`/`0X`/`$` prefixes and 1–8 hex digits.
pub fn parse_addr(field: &str, v: &Value) -> Result<u32, RpcError> {
    let Some(s) = v.as_str() else {
        return Err(RpcError::invalid_params(format!(
            "`{field}` must be a hex string like \"0x00FF0000\" (D9), not {}",
            kind_of(v)
        )));
    };
    let digits = strip_prefix(s).ok_or_else(|| {
        RpcError::invalid_params(format!(
            "`{field}` must start with \"0x\" or \"$\" — a bare \"{s}\" is ambiguous between decimal and hex"
        ))
    })?;
    if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(RpcError::invalid_params(format!(
            "`{field}` = \"{s}\" is not 1-8 hex digits"
        )));
    }
    u32::from_str_radix(digits, 16)
        .map_err(|e| RpcError::invalid_params(format!("`{field}` = \"{s}\": {e}")))
}

/// Parse a hex-string byte payload (`"0x4E714E71"`). An odd digit count is refused: a half-byte has no
/// meaning on an 8-bit bus and silently rounding it would fabricate a nibble.
pub fn parse_bytes(field: &str, v: &Value) -> Result<Vec<u8>, RpcError> {
    let Some(s) = v.as_str() else {
        return Err(RpcError::invalid_params(format!(
            "`{field}` must be a hex string like \"0x4E71\" (D9), not {}",
            kind_of(v)
        )));
    };
    let digits = strip_prefix(s).ok_or_else(|| {
        RpcError::invalid_params(format!("`{field}` must start with \"0x\" or \"$\""))
    })?;
    if digits.len() % 2 != 0 {
        return Err(RpcError::invalid_params(format!(
            "`{field}` = \"{s}\" has an odd number of hex digits ({})",
            digits.len()
        )));
    }
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(RpcError::invalid_params(format!(
            "`{field}` = \"{s}\" is not hex"
        )));
    }
    Ok(digits
        .as_bytes()
        .chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).expect("ascii"), 16).expect("hexdigit"))
        .collect())
}

/// Parse a JSON-number count/length (D9), enforcing an inclusive range. Out of range is a **loud
/// refusal**, never a clamp — a clamped `len` returns fewer bytes than asked for and the caller has no
/// way to notice.
pub fn parse_count(field: &str, v: &Value, min: u64, max: u64) -> Result<u64, RpcError> {
    let Some(n) = v.as_u64() else {
        return Err(RpcError::invalid_params(format!(
            "`{field}` must be a non-negative JSON number (D9), not {}",
            kind_of(v)
        )));
    };
    if n < min || n > max {
        return Err(RpcError::invalid_params(format!(
            "`{field}` = {n} is outside {min}..={max}"
        )));
    }
    Ok(n)
}

fn strip_prefix(s: &str) -> Option<&str> {
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .or_else(|| s.strip_prefix('$'))
}

pub(crate) fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::code;
    use serde_json::json;

    #[test]
    fn addresses_round_trip_in_the_spec_spelling() {
        assert_eq!(addr(0x00FF_A144), "0x00FFA144");
        assert_eq!(parse_addr("addr", &json!("0x00FFA144")).unwrap(), 0xFFA144);
        assert_eq!(parse_addr("addr", &json!("$FFA144")).unwrap(), 0xFFA144);
    }

    #[test]
    fn a_bare_number_is_refused_rather_than_guessed() {
        let e = parse_addr("addr", &json!(4096)).unwrap_err();
        assert_eq!(e.code, code::INVALID_PARAMS);
        let e = parse_addr("addr", &json!("1000")).unwrap_err();
        assert!(e.message.contains("ambiguous"), "{}", e.message);
    }

    #[test]
    fn byte_payloads_round_trip_and_odd_nibbles_are_refused() {
        assert_eq!(bytes(&[0x02, 0x60, 0x00, 0x00]), "0x02600000");
        assert_eq!(bytes(&[]), "0x");
        assert_eq!(
            parse_bytes("bytes", &json!("0x4E71")).unwrap(),
            vec![0x4E, 0x71]
        );
        assert!(parse_bytes("bytes", &json!("0x4E7")).is_err());
    }

    #[test]
    fn counts_are_numbers_and_out_of_range_is_refused_not_clamped() {
        assert_eq!(parse_count("len", &json!(16), 1, 4096).unwrap(), 16);
        let e = parse_count("len", &json!(9999), 1, 4096).unwrap_err();
        assert_eq!(e.code, code::INVALID_PARAMS);
        assert!(e.message.contains("9999"), "{}", e.message);
        assert!(parse_count("len", &json!("16"), 1, 4096).is_err());
    }
}
