//! The Aether envelope: **JSON-RPC 2.0 carried as NDJSON** — exactly one message per `\n`-terminated
//! line, in both directions, with no raw newline inside a message (`protocol.md` §2, §7.1).
//!
//! Nothing in here knows what an emulator is. It parses and serialises the envelope, owns the normative
//! error-code table (§5) and the hex-vs-number type conventions (D9), and provides the two structural
//! helpers the recon's non-negotiables demand: a stamp injector and a bounded-array wrapper.

use serde_json::{json, Map, Value};
use std::io::{self, BufRead};

/// The one integer that bumps only on a breaking **envelope** change (D5). Clients never branch on it —
/// they branch on the capability flags and the generated method list that `initialize` returns.
pub const PROTOCOL_VERSION: i64 = 1;

/// The normative error-code table (`protocol.md` §5). Servers MUST use these.
pub mod code {
    /// Invalid JSON on the wire.
    pub const PARSE_ERROR: i64 = -32700;
    /// Bad envelope: not a JSON-RPC 2.0 request, a batch, or a request sent in the wrong session state.
    pub const INVALID_REQUEST: i64 = -32600;
    /// Method not found / not supported. The authoritative set is `initialize`'s `methods`.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// Missing, wrongly typed, or out-of-bounds params.
    pub const INVALID_PARAMS: i64 = -32602;
    /// Internal error.
    pub const INTERNAL_ERROR: i64 = -32603;
    /// Op not wired (the emulator hook pointer is null). Unused here — every advertised method is wired
    /// by construction, because the advertised list *is* the dispatch table.
    #[allow(dead_code)]
    pub const OP_NOT_WIRED: i64 = -32000;
    /// Address out of range / write rejected.
    pub const ADDRESS_OUT_OF_RANGE: i64 = -32004;
    /// **Invalid state for this operation** — the envelope is fine (not [`INVALID_REQUEST`]), the params
    /// are fine (not [`INVALID_PARAMS`]), nothing failed internally (not [`INTERNAL_ERROR`]); the request
    /// is simply wrong for the machine's mode *right now* (§5). `error.data.reason` MUST carry a
    /// machine-readable discriminant — `checkpointCapReached`, `unknownCheckpoint`, `machineRunning` — so
    /// a client branches on the field and never on the message text.
    pub const INVALID_STATE: i64 = -32005;
    /// Operation timed out.
    #[allow(dead_code)]
    pub const TIMED_OUT: i64 = -32010;
    /// No symbol table has been loaded. **Distinct from [`SYMBOL_NOT_FOUND`] on purpose** (§4): a client
    /// must be able to tell "you forgot to load symbols" from "that name does not exist".
    pub const NO_SYMBOLS_LOADED: i64 = -32012;
    /// A symbol table is loaded but does not contain the requested name.
    pub const SYMBOL_NOT_FOUND: i64 = -32013;
    /// `initialize` asked for a `protocolVersion` this server cannot speak.
    pub const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32015;
}

/// A JSON-RPC error object.
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(code::INVALID_PARAMS, message)
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(code::INVALID_REQUEST, message)
    }

    /// A `-32005`: a perfectly good request in the wrong machine state (§5). The `reason` is the
    /// machine-readable discriminant and is merged into `data` here so it can never be forgotten;
    /// `extra` carries the rest of the context (`cap`/`count`, the offending `id`, …).
    pub fn invalid_state(reason: &str, message: impl Into<String>, extra: Value) -> Self {
        let mut data = match extra {
            Value::Object(m) => m,
            Value::Null => Map::new(),
            other => {
                let mut m = Map::new();
                m.insert("detail".into(), other);
                m
            }
        };
        data.insert("reason".into(), json!(reason));
        Self::new(code::INVALID_STATE, message).with_data(Value::Object(data))
    }

    /// The error object as JSON, with `data` merged with the machine stamp (see [`stamp_object`]) so an
    /// error reply is a citable coordinate too — an agent that gets "address out of range" still learns
    /// *when* it asked.
    pub fn to_json(&self, stamp: &Map<String, Value>) -> Value {
        let mut data = match &self.data {
            Some(Value::Object(m)) => m.clone(),
            Some(other) => {
                let mut m = Map::new();
                m.insert("detail".into(), other.clone());
                m
            }
            None => Map::new(),
        };
        for (k, v) in stamp {
            data.insert(k.clone(), v.clone());
        }
        json!({"code": self.code, "message": self.message, "data": Value::Object(data)})
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// One well-formed incoming message. A request has an `id`; a notification does not.
#[derive(Debug, Clone)]
pub struct Incoming {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

impl Incoming {
    /// A notification expects no reply (JSON-RPC 2.0: no `id`).
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// Parse one NDJSON line. On failure the caller still needs an `id` to answer with, so the id (when it
/// could be recovered at all) comes back alongside the error.
pub fn parse_line(line: &[u8]) -> Result<Incoming, (Option<Value>, RpcError)> {
    let value: Value = match serde_json::from_slice(line) {
        Ok(v) => v,
        Err(e) => {
            return Err((
                None,
                RpcError::new(code::PARSE_ERROR, "invalid JSON")
                    .with_data(json!({"detail": e.to_string()})),
            ))
        }
    };

    // Batches are explicitly optional in the thin slice; `protocol.md` §2 says servers MAY reject them
    // with -32600. We do, loudly, rather than half-supporting them.
    if value.is_array() {
        return Err((
            None,
            RpcError::invalid_request("batch requests are not supported"),
        ));
    }
    let Some(obj) = value.as_object() else {
        return Err((
            None,
            RpcError::invalid_request("a message must be a JSON object"),
        ));
    };

    // Recover the id first so even a malformed request gets a correlated answer.
    let id = match obj.get("id") {
        None | Some(Value::Null) => None,
        Some(v @ (Value::String(_) | Value::Number(_))) => Some(v.clone()),
        Some(_) => {
            return Err((
                None,
                RpcError::invalid_request("id must be a string, a number, or absent"),
            ))
        }
    };

    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err((id, RpcError::invalid_request("jsonrpc must be \"2.0\"")));
    }
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return Err((id, RpcError::invalid_request("method must be a string")));
    };
    let params = match obj.get("params") {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(v @ Value::Object(_)) => v.clone(),
        // Positional params are legal JSON-RPC but no Aether method takes them (§6 is by-name
        // throughout), so accepting an array could only ever mean a client bug.
        Some(_) => {
            return Err((
                id,
                RpcError::invalid_params(
                    "params must be an object (positional params unsupported)",
                ),
            ))
        }
    };

    Ok(Incoming {
        id,
        method: method.to_string(),
        params,
    })
}

/// Serialise a success response. Panics never: `Value` always serialises.
pub fn success_response(id: &Value, result: Value) -> String {
    line(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// Serialise an error response. A null `id` is the JSON-RPC convention when the id was unrecoverable.
pub fn error_response(id: Option<&Value>, error: &RpcError, stamp: &Map<String, Value>) -> String {
    let id = id.cloned().unwrap_or(Value::Null);
    line(&json!({"jsonrpc": "2.0", "id": id, "error": error.to_json(stamp)}))
}

/// Serialise a server-push notification (no `id`, no response expected).
pub fn notification(method: &str, params: Value) -> String {
    line(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
}

/// One NDJSON line, newline **not** included (the writer appends exactly one).
///
/// `serde_json` escapes every control character, so a serialised message can never contain a raw
/// newline; the debug assertion pins that invariant rather than trusting it.
fn line(v: &Value) -> String {
    let s = v.to_string();
    debug_assert!(
        !s.contains('\n') && !s.contains('\r'),
        "NDJSON framing violated: a message must not contain a raw newline"
    );
    s
}

/// **Non-negotiable #1 (recon §4).** The stamp every single reply carries — success *and* error, and
/// every pushed event.
///
/// Without it an agent stitching four reads into one conclusion may be reading four different machine
/// states with no way to detect it. Both time fields are **emulated**, never wall-clock (recon §5 C2):
/// two runs of the same ROM with the same input produce identical stamps, so a stamp is a citable
/// coordinate rather than a timing observation.
pub fn stamp_object(frame: u64, mclk: u64, running: bool) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("frame".into(), json!(frame));
    m.insert("mclk".into(), json!(mclk));
    m.insert("running".into(), json!(running));
    m
}

/// The wire form of the machine's timing basis — the meaning of the `frame` in every stamp
/// ([`stamp_object`]), advertised once in `initialize` (`F-TRACE-PAL`).
///
/// Carries the numbers as well as the label so a client never has to look 896_040 up, and never has to
/// guess whether "ntsc" meant 262 lines or 263. Both come from
/// [`System::timing_basis`](oracle_core::system::System::timing_basis), which derives them from the
/// scheduler's own constants — the server cannot advertise a basis its stamps were not computed with.
pub fn timing_basis_object(basis: oracle_core::system::TimingBasis) -> Value {
    json!({
        "standard": basis.standard.as_str(),
        "mclkPerFrame": basis.mclk_per_frame,
        "linesPerFrame": basis.lines_per_frame,
    })
}

/// Merge the stamp into a result object, overwriting any same-named key so the stamp can never be
/// shadowed by a handler. A non-object result is wrapped rather than dropped.
pub fn stamp_result(result: Value, stamp: &Map<String, Value>) -> Value {
    let mut obj = match result {
        Value::Object(m) => m,
        other => {
            let mut m = Map::new();
            m.insert("value".into(), other);
            m
        }
    };
    for (k, v) in stamp {
        obj.insert(k.clone(), v.clone());
    }
    Value::Object(obj)
}

/// **Non-negotiable #2 (recon §4), narrowed to contract §2.4's bounded-list rule.** Every policy-bounded
/// list carries `total`, `returned` and `truncated` — `truncated` even when it is `false`, because absence
/// and `false` must not both mean "you have everything" — plus the `limit` actually applied. No unbounded
/// dumps.
///
/// `total` is the true size before the bound was applied, so a client always learns what it did not get.
/// `offset` is how many items of `total` this page starts past, and exists only to make `truncated` come
/// out right; it is **not** emitted.
///
/// **This helper emits no continuation token, and that is the fix rather than an omission.** It used to
/// return a numeric `cursor`/`nextCursor` pair, which broke two rules at once: §8 item 16 makes every list
/// cursor on the wire a *string*, and §2.4 clause (b) forbids a method that accepts no cursor param from
/// emitting one at all — a token the client can never hand back trains clients that handles are ignorable.
/// Both of this helper's callers wanted it gone. `lookup_symbol` is a resolution primitive with no cursor
/// param, so §2.4 (b) applies literally; `checkpoint_list` does continue, but its token is an **id**, not
/// this helper's position, and it already stringified its own — it used the helper's `nextCursor` only as a
/// boolean, which `truncated` says directly. So continuation belongs to the one caller that can honour it,
/// and the shared envelope stops publishing a promise it cannot keep.
pub fn bounded_array(items: Vec<Value>, total: usize, offset: usize, limit: usize) -> Value {
    let returned = items.len();
    json!({
        "items": items,
        "total": total,
        "returned": returned,
        "limit": limit,
        "truncated": offset + returned < total,
    })
}

/// Ceiling on one NDJSON line. A client that never sends a newline must not be able to grow the server's
/// heap without bound; this is the read-side half of "a bad client cannot wedge the emulator".
pub const MAX_LINE_BYTES: usize = 1 << 20;

/// One outcome of [`read_line_capped`].
#[derive(Debug)]
pub enum LineRead {
    /// A complete line (newline stripped).
    Payload(Vec<u8>),
    /// The line exceeded [`MAX_LINE_BYTES`]; it was drained to the next newline and discarded.
    TooLong,
    /// Clean end of stream.
    Eof,
}

/// Read one `\n`-terminated line with a hard byte ceiling, draining (not buffering) an over-long line so
/// the connection can keep going after refusing it.
pub fn read_line_capped<R: BufRead>(reader: &mut R, max: usize) -> io::Result<LineRead> {
    let mut buf: Vec<u8> = Vec::new();
    let mut overflowed = false;
    loop {
        let available = match reader.fill_buf() {
            Ok(b) => b,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if available.is_empty() {
            return Ok(if overflowed {
                LineRead::TooLong
            } else if buf.is_empty() {
                LineRead::Eof
            } else {
                LineRead::Payload(buf)
            });
        }
        match available.iter().position(|&b| b == b'\n') {
            Some(i) => {
                // The ceiling has to be checked here too, not only in the no-newline branch: a reader
                // whose buffer already holds the whole over-long line plus its terminator never takes
                // that branch at all.
                if !overflowed && buf.len() + i > max {
                    overflowed = true;
                    buf = Vec::new();
                }
                if !overflowed {
                    buf.extend_from_slice(&available[..i]);
                }
                reader.consume(i + 1);
                if overflowed {
                    return Ok(LineRead::TooLong);
                }
                if buf.last() == Some(&b'\r') {
                    buf.pop();
                }
                return Ok(LineRead::Payload(buf));
            }
            None => {
                let n = available.len();
                if !overflowed {
                    buf.extend_from_slice(available);
                    if buf.len() > max {
                        overflowed = true;
                        buf = Vec::new();
                    }
                }
                reader.consume(n);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_request() {
        let inc = parse_line(br#"{"jsonrpc":"2.0","id":7,"method":"emulator/status"}"#).unwrap();
        assert_eq!(inc.method, "emulator/status");
        assert_eq!(inc.id, Some(json!(7)));
        assert!(!inc.is_notification());
        assert_eq!(inc.params, json!({}));
    }

    #[test]
    fn a_message_without_id_is_a_notification() {
        let inc = parse_line(br#"{"jsonrpc":"2.0","method":"initialized"}"#).unwrap();
        assert!(inc.is_notification());
    }

    #[test]
    fn invalid_json_is_a_parse_error() {
        let (id, e) = parse_line(b"{not json").unwrap_err();
        assert_eq!(e.code, code::PARSE_ERROR);
        assert!(id.is_none());
    }

    #[test]
    fn batches_are_rejected_with_invalid_request() {
        let (_, e) = parse_line(br#"[{"jsonrpc":"2.0","id":1,"method":"x"}]"#).unwrap_err();
        assert_eq!(e.code, code::INVALID_REQUEST);
    }

    #[test]
    fn wrong_jsonrpc_version_is_invalid_request_but_keeps_the_id() {
        let (id, e) = parse_line(br#"{"jsonrpc":"1.0","id":3,"method":"x"}"#).unwrap_err();
        assert_eq!(e.code, code::INVALID_REQUEST);
        assert_eq!(
            id,
            Some(json!(3)),
            "the id must survive so the reply correlates"
        );
    }

    #[test]
    fn positional_params_are_refused() {
        let (_, e) =
            parse_line(br#"{"jsonrpc":"2.0","id":1,"method":"x","params":[1,2]}"#).unwrap_err();
        assert_eq!(e.code, code::INVALID_PARAMS);
    }

    #[test]
    fn stamp_cannot_be_shadowed_by_a_handler() {
        let stamp = stamp_object(12, 34, true);
        let out = stamp_result(json!({"frame": 999, "x": 1}), &stamp);
        assert_eq!(out["frame"], json!(12));
        assert_eq!(out["mclk"], json!(34));
        assert_eq!(out["running"], json!(true));
        assert_eq!(out["x"], json!(1));
    }

    #[test]
    fn error_data_carries_the_stamp_too() {
        let stamp = stamp_object(5, 6, false);
        let e = RpcError::new(code::ADDRESS_OUT_OF_RANGE, "nope").with_data(json!({"addr": "0x0"}));
        let j = e.to_json(&stamp);
        assert_eq!(j["data"]["addr"], json!("0x0"));
        assert_eq!(j["data"]["frame"], json!(5));
        assert_eq!(j["data"]["running"], json!(false));
    }

    #[test]
    fn invalid_state_always_carries_a_machine_readable_reason() {
        let stamp = stamp_object(1, 2, false);
        let e = RpcError::invalid_state(
            "checkpointCapReached",
            "full",
            json!({"cap": 8, "count": 8}),
        );
        assert_eq!(e.code, code::INVALID_STATE);
        let j = e.to_json(&stamp);
        assert_eq!(j["data"]["reason"], json!("checkpointCapReached"));
        assert_eq!(j["data"]["cap"], json!(8));
        // The stamp still wins over anything the caller put in `data` (§2.2).
        assert_eq!(j["data"]["frame"], json!(1));

        // The reason is merged in structurally, so a caller cannot shadow it out by accident.
        let e = RpcError::invalid_state("unknownCheckpoint", "gone", json!({"reason": "oops"}));
        assert_eq!(
            e.to_json(&stamp)["data"]["reason"],
            json!("unknownCheckpoint")
        );
    }

    #[test]
    fn bounded_array_flags_truncation_and_emits_no_continuation_token() {
        let full = bounded_array(vec![json!(1), json!(2)], 2, 0, 5);
        assert_eq!(full["truncated"], json!(false));
        assert_eq!(full["returned"], json!(2));

        let cut = bounded_array(vec![json!(1), json!(2)], 9, 0, 2);
        assert_eq!(cut["truncated"], json!(true));
        assert_eq!(cut["total"], json!(9));
        assert_eq!(cut["limit"], json!(2));

        // A page that starts past the front is still complete when it reaches the end — this is the only
        // reason `offset` is a parameter, and it is what `checkpoint_list` feeds from its live slot count.
        let tail = bounded_array(vec![json!(9)], 3, 2, 5);
        assert_eq!(tail["truncated"], json!(false));

        // §8 item 16 + §2.4 clause (b): the shared envelope carries NO continuation token, in either
        // spelling, on either branch. Continuation is the owning method's, because only it knows whether
        // handing the token back means anything.
        for page in [&full, &cut, &tail] {
            assert!(page.get("cursor").is_none(), "{page}");
            assert!(page.get("nextCursor").is_none(), "{page}");
        }
    }

    #[test]
    fn line_reader_splits_on_newlines_and_reports_eof() {
        let data = b"one\ntwo\n".to_vec();
        let mut r = std::io::BufReader::new(&data[..]);
        let a = read_line_capped(&mut r, MAX_LINE_BYTES).unwrap();
        assert!(matches!(a, LineRead::Payload(ref p) if p == b"one"));
        let b = read_line_capped(&mut r, MAX_LINE_BYTES).unwrap();
        assert!(matches!(b, LineRead::Payload(ref p) if p == b"two"));
        assert!(matches!(
            read_line_capped(&mut r, MAX_LINE_BYTES).unwrap(),
            LineRead::Eof
        ));
    }

    #[test]
    fn over_long_lines_are_drained_not_buffered_and_the_stream_survives() {
        let mut data = vec![b'x'; 100];
        data.push(b'\n');
        data.extend_from_slice(b"ok\n");
        let mut r = std::io::BufReader::new(&data[..]);
        assert!(matches!(
            read_line_capped(&mut r, 16).unwrap(),
            LineRead::TooLong
        ));
        // The next line must still parse — refusing one over-long message does not desync the framing.
        assert!(
            matches!(read_line_capped(&mut r, 16).unwrap(), LineRead::Payload(ref p) if p == b"ok")
        );
    }
}
