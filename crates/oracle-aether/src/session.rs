//! The per-connection state machine: `initialize` → `initialized` → everything else
//! (`protocol.md` §2.1, D4, D6).
//!
//! Pure — no sockets, no engine, no threads — so the handshake rules are unit-testable on their own.
//! The one rule with teeth is D6's: **the server MUST NOT push events before `initialized`**, so a
//! connection is registered as an event subscriber at exactly that transition and nowhere earlier.

use crate::rpc::{Incoming, RpcError};
use serde_json::{json, Value};

/// What the connection loop should do with a message that has already been parsed.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Answer the handshake (`initialize`).
    Initialize,
    /// The `initialized` notification arrived; register for events iff the payload says so.
    Subscribe { events: bool },
    /// A normal method call — hand it to the engine.
    Dispatch,
    /// A notification that needs no reply and no action.
    Ignore,
}

/// One connection's handshake state.
#[derive(Debug, Default)]
pub struct Session {
    initialized: bool,
    ready: bool,
    wants_events: bool,
    client_id: Option<String>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `initialized` has been seen (after which events may be pushed).
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn wants_events(&self) -> bool {
        self.wants_events
    }

    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    /// Advance the state machine. Errors here are envelope-level and never reach the engine.
    pub fn on_message(&mut self, msg: &Incoming) -> Result<Action, RpcError> {
        match msg.method.as_str() {
            "initialize" => {
                if msg.is_notification() {
                    return Err(RpcError::invalid_request(
                        "initialize is a request (it needs an `id`), not a notification",
                    ));
                }
                if self.initialized {
                    return Err(RpcError::invalid_request(
                        "this connection has already been initialized",
                    ));
                }
                self.initialized = true;
                self.wants_events = msg
                    .params
                    .get("clientCapabilities")
                    .and_then(|c| c.get("events"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.client_id = msg
                    .params
                    .get("clientId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                Ok(Action::Initialize)
            }
            "initialized" => {
                if !self.initialized {
                    return Err(RpcError::invalid_request(
                        "`initialized` arrived before `initialize`",
                    ));
                }
                if self.ready {
                    // Idempotent rather than fatal: a duplicate is a client bug, not a protocol break,
                    // and dropping the connection over it would lose more than it protects.
                    return Ok(Action::Ignore);
                }
                self.ready = true;
                Ok(Action::Subscribe {
                    events: self.wants_events,
                })
            }
            _ => {
                if !self.initialized {
                    return Err(RpcError::invalid_request(format!(
                        "`initialize` must be the first message on a connection (got {})",
                        msg.method
                    ))
                    .with_data(json!({"expected": "initialize", "got": msg.method})));
                }
                Ok(Action::Dispatch)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{code, parse_line};

    fn msg(line: &str) -> Incoming {
        parse_line(line.as_bytes()).expect("well-formed")
    }

    #[test]
    fn a_method_before_initialize_is_refused() {
        let mut s = Session::new();
        let e = s
            .on_message(&msg(
                r#"{"jsonrpc":"2.0","id":1,"method":"emulator/status"}"#,
            ))
            .unwrap_err();
        assert_eq!(e.code, code::INVALID_REQUEST);
        assert!(!s.is_ready());
    }

    #[test]
    fn initialized_before_initialize_is_refused() {
        let mut s = Session::new();
        assert!(s
            .on_message(&msg(r#"{"jsonrpc":"2.0","method":"initialized"}"#))
            .is_err());
    }

    #[test]
    fn the_happy_path_subscribes_only_when_the_client_asked() {
        let mut s = Session::new();
        assert_eq!(
            s.on_message(&msg(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientId":"mcp","clientCapabilities":{"events":true}}}"#
            ))
            .unwrap(),
            Action::Initialize
        );
        assert_eq!(s.client_id(), Some("mcp"));
        assert!(!s.is_ready(), "events must not flow before `initialized`");
        assert_eq!(
            s.on_message(&msg(r#"{"jsonrpc":"2.0","method":"initialized"}"#))
                .unwrap(),
            Action::Subscribe { events: true }
        );
        assert!(s.is_ready());
        assert_eq!(
            s.on_message(&msg(
                r#"{"jsonrpc":"2.0","id":2,"method":"emulator/status"}"#
            ))
            .unwrap(),
            Action::Dispatch
        );
    }

    #[test]
    fn a_client_that_did_not_opt_in_is_not_subscribed() {
        let mut s = Session::new();
        s.on_message(&msg(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#))
            .unwrap();
        assert_eq!(
            s.on_message(&msg(r#"{"jsonrpc":"2.0","method":"initialized"}"#))
                .unwrap(),
            Action::Subscribe { events: false }
        );
    }

    #[test]
    fn initialize_twice_is_refused_and_a_duplicate_initialized_is_ignored() {
        let mut s = Session::new();
        s.on_message(&msg(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#))
            .unwrap();
        assert!(s
            .on_message(&msg(r#"{"jsonrpc":"2.0","id":2,"method":"initialize"}"#))
            .is_err());
        s.on_message(&msg(r#"{"jsonrpc":"2.0","method":"initialized"}"#))
            .unwrap();
        assert_eq!(
            s.on_message(&msg(r#"{"jsonrpc":"2.0","method":"initialized"}"#))
                .unwrap(),
            Action::Ignore
        );
    }

    #[test]
    fn initialize_sent_as_a_notification_is_refused() {
        let mut s = Session::new();
        assert!(s
            .on_message(&msg(r#"{"jsonrpc":"2.0","method":"initialize"}"#))
            .is_err());
    }
}
