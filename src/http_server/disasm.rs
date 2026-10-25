//! Decode captured instruction bytes in the unprivileged HTTP process.
//!
//! # Interface
//! [`decode_response`] (`pub(super)`): `fn decode_response(response: &mut MonitorResponse) -> anyhow::Result<()>`.
//! The response envelope is [`MonitorResponse`].

use anyhow::Result;
use iced_x86::{Decoder, DecoderError, DecoderOptions, Formatter, IntelFormatter};
use procinsh_protocol::ipc::schema::{MonitorEvent, MonitorResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_BYTES: usize = 256;
const MAX_INSTRUCTIONS: usize = 32;

#[derive(Serialize)]
struct Instruction {
    address: String,
    bytes: Vec<u8>,
    text: String,
    current: bool,
}

#[derive(Deserialize, Serialize)]
struct Disassembly {
    address: String,
    bytes: Vec<u8>,
    #[serde(skip_deserializing)]
    instructions: Vec<Instruction>,
    error: Option<String>,
}

impl Disassembly {
    /// RIP is a known instruction boundary. Never guess boundaries by decoding
    /// backwards.
    fn decode(&mut self) -> Result<()> {
        let address = u64::from_str_radix(self.address.trim_start_matches("0x"), 16)?;
        self.instructions.clear();
        let bytes = &self.bytes[..self.bytes.len().min(MAX_BYTES)];
        let mut decoder = Decoder::with_ip(64, bytes, address, DecoderOptions::NONE);
        let mut formatter = IntelFormatter::new();
        formatter.options_mut().set_hex_prefix("0x");
        formatter.options_mut().set_hex_suffix("");
        formatter.options_mut().set_uppercase_hex(false);
        for _ in 0..MAX_INSTRUCTIONS {
            if !decoder.can_decode() {
                break;
            }
            let offset = decoder.position();
            let instruction = decoder.decode();
            if instruction.is_invalid() {
                let reason = if decoder.last_error() == DecoderError::NoMoreBytes {
                    "The captured range ends mid-instruction."
                } else {
                    "Invalid or unsupported instruction encoding."
                };
                let detail = format!("0x{:016x}: {reason}", instruction.ip());
                self.error = Some(match self.error.take() {
                    Some(error) => format!("{error} {detail}"),
                    None => detail,
                });
                break;
            }
            let mut text = String::new();
            formatter.format(&instruction, &mut text);
            self.instructions.push(Instruction {
                address: format!("0x{:016x}", instruction.ip()),
                bytes: bytes[offset..decoder.position()].to_vec(),
                text,
                current: offset == 0,
            });
        }
        Ok(())
    }
}

/// IPC observations carry captured bytes and acquisition errors. Decode only
/// after receipt, before any HTTP serialization, preserving acquisition errors.
pub(super) fn decode_response(response: &mut MonitorResponse) -> Result<()> {
    let value = match response {
        MonitorResponse::Data(value) => value,
        MonitorResponse::Started {
            initial: MonitorEvent::Observation(value),
            ..
        }
        | MonitorResponse::Event {
            event: Some(MonitorEvent::Observation(value)),
            ..
        } => value,
        _ => return Ok(()),
    };
    if let Some(samples) = value.get_mut("live_samples").and_then(Value::as_array_mut) {
        for sample in samples {
            if let Some(code) = sample.get_mut("disassembly").filter(|code| !code.is_null()) {
                let mut decoded: Disassembly = serde_json::from_value(code.take())?;
                decoded.decode()?;
                *code = serde_json::to_value(decoded)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Disassembly {
        fn empty(address: u64) -> Self {
            Self {
                address: format!("0x{address:016x}"),
                bytes: Vec::new(),
                instructions: Vec::new(),
                error: None,
            }
        }
    }
    #[test]
    fn decodes_initial_and_polled_observations_after_ipc_round_trip() {
        let raw = serde_json::json!({"live_samples": [
            {"tid": 123, "disassembly": {"address": "0x0000000000001000", "bytes": [144, 72, 139], "error": "Partial read."}},
            {"tid": 124, "disassembly": null},
            {"tid": 125, "disassembly": {"address": "0x0000000000002000", "bytes": [], "error": "Read failed."}}
        ]});
        let responses = [
            MonitorResponse::Data(raw.clone()),
            MonitorResponse::Started {
                session: 1,
                initial: MonitorEvent::Observation(raw.clone()),
            },
            MonitorResponse::Event {
                event: Some(MonitorEvent::Observation(raw)),
                closed: false,
            },
        ];
        for response in responses {
            let mut response =
                serde_json::from_slice(&serde_json::to_vec(&response).unwrap()).unwrap();
            decode_response(&mut response).unwrap();
            let value = match response {
                MonitorResponse::Data(value)
                | MonitorResponse::Started {
                    initial: MonitorEvent::Observation(value),
                    ..
                }
                | MonitorResponse::Event {
                    event: Some(MonitorEvent::Observation(value)),
                    ..
                } => value,
                _ => unreachable!(),
            };
            let samples = &value["live_samples"];
            assert_eq!(samples[0]["tid"], 123);
            let code = &samples[0]["disassembly"];
            assert_eq!(code["address"], "0x0000000000001000");
            assert_eq!(code["bytes"], serde_json::json!([144, 72, 139]));
            assert_eq!(
                code["instructions"][0],
                serde_json::json!({"address": "0x0000000000001000", "bytes": [144], "text": "nop", "current": true})
            );
            assert!(code["error"].as_str().unwrap().starts_with("Partial read."));
            assert!(code["error"].as_str().unwrap().contains("mid-instruction"));
            assert!(samples[1]["disassembly"].is_null());
            assert_eq!(samples[2]["disassembly"]["error"], "Read failed.");
            assert_eq!(
                samples[2]["disassembly"]["instructions"],
                serde_json::json!([])
            );
        }
        decode_response(&mut MonitorResponse::Closed).unwrap();
        decode_response(&mut MonitorResponse::Data(
            serde_json::json!({"live_samples": []}),
        ))
        .unwrap();
    }

    #[test]
    fn decodes_variable_length_instructions_and_runtime_branch_target() {
        let mut code = Disassembly::empty(0x7fff_1234_5000);
        code.bytes = vec![0x55, 0x48, 0x89, 0xe5, 0xe8, 0x10, 0, 0, 0, 0xc3];
        code.decode().unwrap();
        assert!(code.error.is_none());
        assert_eq!(code.instructions.len(), 4);
        assert_eq!(code.instructions[0].text, "push rbp");
        assert_eq!(code.instructions[1].text, "mov rbp,rsp");
        assert_eq!(code.instructions[1].address, "0x00007fff12345001");
        assert_eq!(code.instructions[2].bytes.len(), 5);
        assert!(code.instructions[2].text.contains("7fff12345019"));
        assert!(code.instructions[0].current);
        assert!(code.instructions[1..].iter().all(|i| !i.current));
        let json = serde_json::to_value(&code).unwrap();
        assert_eq!(json["address"], "0x00007fff12345000");
    }

    #[test]
    fn stops_on_truncated_or_invalid_instructions_and_bounds_output() {
        let mut code = Disassembly::empty(0x1000);
        code.bytes = vec![0x90, 0x48, 0x8b];
        code.decode().unwrap();
        assert_eq!(code.instructions.len(), 1);
        assert!(code.error.as_ref().unwrap().contains("mid-instruction"));
        let mut invalid = Disassembly::empty(0x1000);
        invalid.bytes = vec![0xf0, 0x90]; // LOCK NOP is invalid.
        invalid.decode().unwrap();
        assert!(invalid.instructions.is_empty());
        assert!(invalid.error.is_some());
        let mut many = Disassembly::empty(0x1000);
        many.bytes = vec![0x90; 512];
        many.decode().unwrap();
        assert_eq!(many.instructions.len(), MAX_INSTRUCTIONS);
    }
}
