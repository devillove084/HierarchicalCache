use std::error::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Protocol is the first four bits
pub const MASK_MESSAGE_PROTOCOL: u8             = 0xF0; // 0b11110000
pub const MESSAGE_PROTOCOL_SAMPLING_MESSAGE: u8 = 0x10; // 0b00010000
pub const MESSAGE_PROTOCOL_HEADER_MESSAGE: u8   = 0x20; // 0b00100000
pub const MESSAGE_PROTOCOL_CONTENT_MESSAGE: u8  = 0x40; // 0b01000000
pub const MESSAGE_PROTOCOL_NOOP_MESSAGE: u8     = 0x80; // 0b10000000

/// The message type. [MessageType::Request] is used to advertise the node data or request advertised data;
/// [MessageType::Response] is used to advertise back in response to a request, or provide the requested data.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    Request = 1,
    Response = 2,
}

/// Message trait with generic implementation for serialization and deserialization
pub trait Message {

    /// The message protocol, used for serialization/deserialization
    fn protocol(&self) -> u8;

    /// Serializes message for sending over the wire
    fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error>>
    where Self: Serialize
    {
        match serde_cbor::to_vec(&self) {
            Ok(bytes) => Ok(bytes),
            Err(e) => Err(e)?,
        }
    }

    /// Deserializes a message
    fn from_bytes<'a>(bytes: &'a [u8]) -> Result<Self, Box<dyn Error>>
    where Self: Sized + Deserialize<'a>
    {
        match serde_cbor::from_slice::<Self>(bytes) {
            Ok(m) => Ok(m),
            Err(e) => Err(e)?
        }
    }
}

/// An empty no-op message used to stop listening TCP connections
#[derive(Debug, Serialize, Deserialize)]
pub struct NoopMessage;
impl Message for NoopMessage {
    fn protocol(&self) -> u8 {
        MESSAGE_PROTOCOL_NOOP_MESSAGE
    }
}

// A message containing the digests of all the active updates on a node.
/// It is used to advertise the updates present at each node.
#[derive(Debug, Serialize, Deserialize)]
pub struct HeaderMessage {
    sender: String,
    message_type: MessageType,
    headers: Vec<String>,
}
impl HeaderMessage {
    pub fn new_request(sender: String) -> Self {
        Self::new(sender, MessageType::Request)
    }
    pub fn new_response(sender: String) -> Self {
        Self::new(sender, MessageType::Response)
    }
    fn new(sender: String, message_type: MessageType) -> Self {
        HeaderMessage {
            sender,
            message_type,
            headers: Vec::new()
        }
    }
    pub fn set_headers(&mut self, headers: Vec<String>) {
        self.headers = headers
    }
    pub fn sender(&self) -> &str {
        &self.sender
    }
    pub fn message_type(&self) -> &MessageType {
        &self.message_type
    }
    pub fn headers(&self) -> &Vec<String> {
        &self.headers
    }
}
impl Message for HeaderMessage {
    fn protocol(&self) -> u8 {
        MESSAGE_PROTOCOL_HEADER_MESSAGE
    }
}

/// A message that is either a request for updates ([MessageType::Request]) or a response
/// containing requested updates ([MessageType::Response]).
#[derive(Debug, Serialize, Deserialize)]
pub struct ContentMessage {
    sender: String,
    message_type: MessageType,
    content: HashMap<String, Vec<u8>>,
}
impl ContentMessage {
    pub fn new_request(sender: String, content: HashMap<String, Vec<u8>>) -> Self {
        Self::new(sender, MessageType::Request, content)
    }
    pub fn new_response(sender: String, content: HashMap<String, Vec<u8>>) -> Self {
        Self::new(sender, MessageType::Response, content)
    }
    fn new(sender: String, message_type: MessageType, content: HashMap<String, Vec<u8>>) -> Self {
        ContentMessage {
            sender,
            message_type,
            content,
        }
    }
    pub fn sender(&self) -> &str {
        &self.sender
    }
    pub fn message_type(&self) -> &MessageType {
        &self.message_type
    }

    pub fn len(&self) -> usize {
        self.content.len()
    }
    /// Returns the content of the message. Moves the message to avoid copying its content.
    pub fn content(self) -> HashMap<String, Vec<u8>> {
        self.content
    }
}
impl Message for ContentMessage {
    fn protocol(&self) -> u8 {
        MESSAGE_PROTOCOL_CONTENT_MESSAGE
    }
}
