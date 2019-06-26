pub mod error;
pub mod framing;
pub mod messages;

use crate::error::Result;
use crate::v1::error::ErrorKind;
use crate::LOGGER;

use crate::v1::framing::Frame;
use crate::v1::framing::Method;

use bitcoin_hashes::hex::{FromHex, ToHex};
use hex::{decode, FromHexError};
use failure::ResultExt;
use serde::{Deserialize, Serialize};
use slog::trace;
use std::convert::TryFrom;
use std::str::FromStr;
use wire::{Message, Payload, ProtocolBase};

pub struct V1Protocol;
impl ProtocolBase for V1Protocol {
    type Handler = V1Handler;
}

/// Specifies all messages to be visited
pub trait V1Handler: 'static {
    /// Handles the result part of the response
    fn visit_stratum_result(
        &mut self,
        _msg: &Message<V1Protocol>,
        _payload: &framing::StratumResult,
    ) {
    }
    /// Handles the error part of the response
    fn visit_stratum_error(
        &mut self,
        _msg: &Message<V1Protocol>,
        _payload: &framing::StratumError,
    ) {
    }

    fn visit_mining_configure(
        &mut self,
        _msg: &Message<V1Protocol>,
        _payload: &messages::Subscribe,
    ) {
    }

    fn visit_subscribe(&mut self, _msg: &Message<V1Protocol>, _payload: &messages::Subscribe) {}

    fn visit_authorize(&mut self, _msg: &Message<V1Protocol>, _payload: &messages::Authorize) {}

    fn visit_set_difficulty(
        &mut self,
        _msg: &Message<V1Protocol>,
        _payload: &messages::SetDifficulty,
    ) {
    }

    fn visit_notify(&mut self, _msg: &Message<V1Protocol>, _payload: &messages::Notify) {}

    fn visit_submit(&mut self, _msg: &Message<V1Protocol>, _payload: &messages::Submit) {}
}

/// TODO: deserialization should be done from &[u8] so that it is consistent with V2
pub fn deserialize_message(src: &str) -> Result<Message<V1Protocol>> {
    let deserialized = framing::Frame::from_str(src)?;

    trace!(
        LOGGER,
        "V1: Deserialized V1 message payload: {:?}",
        deserialized
    );
    let (id, payload) = match deserialized {
        Frame::RpcRequest(request) => match request.payload.method {
            Method::Subscribe => (
                request.id,
                Ok(Box::new(messages::Subscribe::try_from(request)?)
                    as Box<dyn Payload<V1Protocol>>),
            ),
            Method::Submit => (
                request.id,
                Ok(Box::new(messages::Submit::try_from(request)?) as Box<dyn Payload<V1Protocol>>),
            ),
            Method::Authorize => (
                request.id,
                Ok(Box::new(messages::Authorize::try_from(request)?)
                    as Box<dyn Payload<V1Protocol>>),
            ),
            Method::SetDifficulty => (
                request.id,
                Ok(Box::new(messages::SetDifficulty::try_from(request)?)
                    as Box<dyn Payload<V1Protocol>>),
            ),
            Method::Notify => (
                request.id,
                Ok(Box::new(messages::Notify::try_from(request)?) as Box<dyn Payload<V1Protocol>>),
            ),
            _ => (
                None,
                Err(ErrorKind::Rpc(format!("Unsupported request {:?}", request))),
            ),
        },
        // This is not ideal implementation as we clone() the result or error parts of the response.
        // Note, however, the unwrap() is safe as the error/result are 'Some'
        Frame::RpcResponse(response) => {
            let msg = if response.payload.error.is_some() {
                Ok(Box::new(response.payload.error.unwrap().clone())
                    as Box<dyn Payload<V1Protocol>>)
            } else if response.payload.result.is_some() {
                Ok(Box::new(response.payload.result.unwrap().clone())
                    as Box<dyn Payload<V1Protocol>>)
            } else {
                Err(ErrorKind::Rpc(format!(
                    "Malformed response no error, no result specified {:?}",
                    response
                )))
            };
            (Some(response.id), msg)
        }
    };
    trace!(LOGGER, "V1: Deserialized message ID {:?}", id);
    // convert the payload into message
    Ok(payload.map(|p| Message::new(id, p))?)
}

/// Extranonce 1 introduced as new type to provide shared conversions to/from string
/// TODO: find out correct byte order for extra nonce 1
/// TODO: implement deref trait consolidate use of extra nonce 1
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExtraNonce1(pub HexBytes);

/// Helper type that allows simple serialization and deserialization of byte vectors
/// that are represented as hex strings in JSON
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(into = "String", from = "String")]
pub struct HexBytes(Vec<u8>);

impl HexBytes {
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// Referencing the internal part of hex bytes
impl AsRef<Vec<u8>> for HexBytes {
    fn as_ref(&self) -> &Vec<u8> {
        &self.0
    }
}

/// fix for error on odd-length hex sequences
/// FIXME: find a nicer solution
fn hex_decode(s: &str) -> std::result::Result<Vec<u8>, FromHexError> {
    if s.len() % 2 != 0 {
        hex::decode(&format!("0{}", s))
    } else {
        hex::decode(s)
    }
}

impl TryFrom<&str> for HexBytes {
    type Error = crate::error::Error;

    fn try_from(value: &str) -> Result<Self> {
        Ok(HexBytes(
            hex_decode(value).context("Parsing hex bytes failed")?,
        ))
    }
}

/// TODO: this is not the cleanest way as any deserialization error is essentially consumed and
/// manifested as empty vector. However, it is very comfortable to use this trait implementation
/// in Extranonce1 serde support
impl From<String> for HexBytes {
    fn from(value: String) -> Self {
        HexBytes::try_from(value.as_str()).unwrap_or(HexBytes(vec![]))
    }
}

/// Helper Serializer
impl Into<String> for HexBytes {
    fn into(self) -> String {
        hex::encode(self.0)
    }
}

/// Little-endian hex encoded u32
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(into = "String", from = "String")]
pub struct HexU32Le(u32);

impl TryFrom<&str> for HexU32Le {
    type Error = crate::error::Error;

    fn try_from(value: &str) -> Result<Self> {
        let parsed_bytes: [u8; 4] = FromHex::from_hex(value).context("parse u32 hex value")?;
        Ok(HexU32Le(u32::from_le_bytes(parsed_bytes)))
    }
}

/// TODO: this is not the cleanest way as any deserialization error is essentially consumed and
/// manifested as empty vector. However, it is very comfortable to use this trait implementation
/// in Extranonce1 serde support
impl From<String> for HexU32Le {
    fn from(value: String) -> Self {
        HexU32Le::try_from(value.as_str()).unwrap_or(HexU32Le(0))
    }
}

/// Helper Serializer
impl Into<String> for HexU32Le {
    fn into(self) -> String {
        self.0.to_le_bytes().to_hex()
        //hex::encode(self.0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::v1::*;

    /// Test traits that will be used by serded for HexBytes when converting from/to string
    #[test]
    fn test_hex_bytes() {
        let hex_bytes = HexBytes(vec![0xde, 0xad, 0xbe, 0xef, 0x11, 0x22, 0x33]);
        let hex_bytes_str = "deadbeef112233";

        let checked_hex_bytes_str: String = hex_bytes.clone().into();
        assert_eq!(
            hex_bytes_str, checked_hex_bytes_str,
            "Mismatched hex bytes strings",
        );

        let checked_hex_bytes = HexBytes::try_from(hex_bytes_str).expect("");

        assert_eq!(hex_bytes, checked_hex_bytes, "Mismatched hex bytes values",)
    }

    #[test]
    fn test_extra_nonce1() {
        let expected_enonce1 =
            ExtraNonce1(HexBytes(vec![0xde, 0xad, 0xbe, 0xef, 0x11, 0x22, 0x33]));
        let expected_enonce1_str = r#""deadbeef112233""#;

        let checked_enonce1_str: String =
            serde_json::to_string(&expected_enonce1).expect("Serialization failed");
        assert_eq!(
            expected_enonce1_str, checked_enonce1_str,
            "Mismatched extranonce 1 strings",
        );

        let checked_enonce1 =
            serde_json::from_str(expected_enonce1_str).expect("Deserialization failed");

        assert_eq!(
            expected_enonce1, checked_enonce1,
            "Mismatched extranonce 1 values",
        )
    }

    /// This test demonstrates an actual implementation of protocol handler (aka visitor to a set of
    /// desired messsages
    #[test]
    fn test_deserialize_serialize_request_message() {
        for req in V1_TEST_REQUESTS.iter() {
            let msg = deserialize_message(req).expect("Deserialization failed");
            msg.accept(&mut TestIdentityHandler);
        }
    }

    #[test]
    fn test_deserialize_response_message() {
        let msg =
            deserialize_message(MINING_SUBSCRIBE_OK_RESULT_JSON).expect("Deserialization failed");
        msg.accept(&mut TestIdentityHandler);
        // TODO also perform serialization and check the output matches
    }

    // add also a separate stratum error test as per above response
}
