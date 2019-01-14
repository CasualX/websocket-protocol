/*!
WebSocket Protocol
==================

Defines a low-level webSocket protocol implementation without any reference to networking or async io.
!*/

#![no_std]

#[cfg(feature = "std")]
extern crate std;
#[cfg(feature = "std")]
use std::prelude::v1::*;

use core::{mem, str};

pub mod http;

//----------------------------------------------------------------

/// WebSocket Message.
///
/// The message is borrowed and does not own the underlying data.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Msg<'a> {
	Text(&'a str),
	Binary(&'a [u8]),
	Close(u16, &'a str),
	Ping(&'a [u8]),
	Pong(&'a [u8]),
}
impl<'a> Msg<'a> {
	/// Constructs a new Msg from the given opcode and payload.
	///
	/// If the payload is expected to be a string, an error will be returned if the payload is not valid UTF-8.
	///
	/// # Panics
	///
	/// Panics if `opcode` has the value [`Continue`](Opcode::Continue).
	pub fn new(opcode: Opcode, payload: &'a [u8]) -> Result<Msg<'a>, str::Utf8Error> {
		match opcode {
			Opcode::Continue => panic!("Cannot construct WebSocket Msg from Continue opcode"),
			Opcode::Text => str::from_utf8(payload).map(Msg::Text),
			Opcode::Binary => Ok(Msg::Binary(payload)),
			Opcode::Close => if payload.len() >= 2 {
				let code = (payload[0] as u16) << 8 | payload[1] as u16;
				let reason = str::from_utf8(&payload[2..])?;
				Ok(Msg::Close(code, reason))
			}
			else {
				Ok(Msg::Close(CLOSE_NORMAL, ""))
			},
			Opcode::Ping => Ok(Msg::Ping(payload)),
			Opcode::Pong => Ok(Msg::Pong(payload)),
		}
	}
	/// Returns the associated opcode for this msg.
	pub fn opcode(&self) -> Opcode {
		match self {
			Msg::Text(_) => Opcode::Text,
			Msg::Binary(_) => Opcode::Binary,
			Msg::Close(_, _) => Opcode::Close,
			Msg::Ping(_) => Opcode::Ping,
			Msg::Pong(_) => Opcode::Pong,
		}
	}
}

//----------------------------------------------------------------

/// WebSocket Message.
#[cfg(feature = "std")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Message {
	Text(String),
	Binary(Vec<u8>),
	Close(u16, String),
	Ping(Vec<u8>),
	Pong(Vec<u8>),
}
#[cfg(feature = "std")]
impl Message {
	pub fn new(opcode: Opcode, mut payload: Vec<u8>) -> Result<Message, Vec<u8>> {
		match opcode {
			Opcode::Continue => Err(payload),
			Opcode::Text => String::from_utf8(payload).map(Message::Text).map_err(|err| err.into_bytes()),
			Opcode::Binary => Ok(Message::Binary(payload)),
			Opcode::Close => {
				if payload.len() >= 2 {
					let close_code = (payload[0] as u16) << 8 | payload[1] as u16;
					payload.drain(..2);
					match String::from_utf8(payload) {
						Ok(reason) => Ok(Message::Close(close_code, reason)),
						Err(err) => Err(err.into_bytes()),
					}
				}
				else {
					Ok(Message::Close(CLOSE_NORMAL, String::new()))
				}
			},
			Opcode::Ping => Ok(Message::Ping(payload)),
			Opcode::Pong => Ok(Message::Pong(payload)),
		}
	}
	pub fn as_msg(&self) -> Msg<'_> {
		match self {
			Message::Text(text) => Msg::Text(text),
			Message::Binary(binary) => Msg::Binary(binary),
			Message::Close(code, reason) => Msg::Close(*code, reason),
			Message::Ping(payload) => Msg::Ping(payload),
			Message::Pong(payload) => Msg::Pong(payload),
		}
	}
	pub fn take_payload(self) -> Vec<u8> {
		match self {
			Message::Text(text) => text.into(),
			Message::Binary(binary) => binary,
			Message::Close(_code, reason) => reason.into(),
			Message::Ping(payload) => payload,
			Message::Pong(payload) => payload,
		}
	}
}

pub const CLOSE_NORMAL: u16 = 1000;
pub const CLOSE_GOING_AWAY: u16 = 1001;
pub const CLOSE_PROTOCOL_ERROR: u16 = 1002;
pub const CLOSE_UNSUPPORTED: u16 = 1003;
pub const CLOSE_NO_STATUS: u16 = 1005;
pub const CLOSE_ABNORMAL: u16 = 1006;
pub const CLOSE_UNSUPPORTED_DATA: u16 = 1007;
pub const CLOSE_POLICY_VIOLATION: u16 = 1008;
pub const CLOSE_TOO_LARGE: u16 = 1009;
pub const CLOSE_MISSING_EXTENSION: u16 = 1010;
pub const CLOSE_INTERNAL_ERROR: u16 = 1011;
pub const CLOSE_SERVICE_RESTART: u16 = 1012;
pub const CLOSE_TRY_AGAIN_LATER: u16 = 1013;

//----------------------------------------------------------------

#[cfg(feature = "std")]
#[derive(Debug)]
pub struct WebSocket {
	fin: bool,
	opcode: Opcode,
	buffer: Vec<u8>,
}
#[cfg(feature = "std")]
impl WebSocket {
	pub fn recv(&mut self, stream: &[u8]) -> Result<usize, Error> {
		// Early reject
		if stream.len() == 0 {
			return Err(Error::WouldBlock);
		}
		// Decode the frame header
		let mut frame_header = FrameHeader::default();
		let frame_header_len = decode_header(stream, &mut frame_header)?;
		// Allocate memory in the buffer for the frame
		let start = self.buffer.len();
		let end = start + frame_header.payload_len as usize;
		if self.buffer.capacity() < end {
			let additional = end - self.buffer.capacity();
			self.buffer.reserve(additional);
		}
		// Decode the payload into the buffer
		unsafe {
			let dest = core::slice::from_raw_parts_mut(self.buffer.as_mut_ptr().offset(start as usize), end - start);
			decode_payload(&frame_header, &stream[frame_header_len..], dest)?;
			self.buffer.set_len(end);
		}
		// Stich together a WebSocket message
		self.fin = frame_header.fin;
		
		unimplemented!()
	}
	pub fn fin(&self) -> bool {
		self.fin
	}
	pub fn as_msg(&self) -> Option<Result<Msg<'_>, str::Utf8Error>> {
		if self.fin {
			Some(Msg::new(self.opcode, &self.buffer))
		}
		else {
			None
		}
	}
}

//----------------------------------------------------------------

/// Operation code.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
	/// Indicates a continuation frame of a fragmented message.
	Continue = 0x0,
	/// Indicates a text data frame.
	Text = 0x1,
	/// Indicates a binary data frame.
	Binary = 0x2,
	/// Indicates a close control frame.
	Close = 0x8,
	/// Indicates a ping control frame.
	Ping = 0x9,
	/// Indicates a pong control frame.
	Pong = 0xA,
}
impl Opcode {
	/// Tests whether the opcode indicates a control frame.
	pub fn is_control(self) -> bool {
		(self as u8 & 0x8) != 0
	}
	/// Try to construct an Opcode from a byte.
	///
	/// The opcode is returned as an error if it is not valid.
	pub fn try_from(opcode: u8) -> Result<Opcode, u8> {
		if (opcode & 0xF7) < 3 {
			Ok(unsafe { mem::transmute(opcode) })
		}
		else {
			Err(opcode)
		}
	}
}
impl From<Opcode> for u8 {
	fn from(opcode: Opcode) -> u8 {
		opcode as u8
	}
}

//----------------------------------------------------------------

/// Magic value indicating 16bit payload length.
///
/// When the payload length field contains this value, the next 2 bytes contain the extended payload length in network endian.
pub const PAYLOAD_LEN_16: u8 = 126;
/// Magic value indicating 64bit payload length.
///
/// When the payload length field contains this value, the next 8 bytes contain the extended payload length in network endian.
pub const PAYLOAD_LEN_64: u8 = 127;

/// Minimum 16bit payload length.
///
/// The payload length must be minimally encoded, if it is any less it must be encoded as such.
pub const PAYLOAD_LEN_MIN_16: u16 = 126;
/// Minimum 64bit payload length.
///
/// The payload length must be minimally encoded, if it is any less it must be encoded as such.
pub const PAYLOAD_LEN_MIN_64: u64 = 65536;

//----------------------------------------------------------------

/// Framing errors.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
	/// Payload length error: [Section 5.2](https://tools.ietf.org/html/rfc6455#section-5.2).
	///
	/// When 64bit length, the most significant bit MUST be 0.
	///
	/// The minimal number of bytes MUST be used to encode the length.
	BadLength,
	UnknownOpCode,
	/// Control Frames: [Section 5.5](https://tools.ietf.org/html/rfc6455#section-5.5).
	///
	/// All control frames MUST have a payload length of 125 bytes or less and MUST NOT be fragmented.
	BadControl,
	/// Not enough data was provided to fully decode the input stream.
	/// Accumulate more data in the input stream and call the function again.
	WouldBlock,
}

//----------------------------------------------------------------
/*
Source: https://tools.ietf.org/html/rfc6455#section-5.2

```plaintext
 0               1               2               3              
 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7
+-+-+-+-+-------+-+-------------+-------------------------------+
|F|R|R|R| opcode|M| Payload len |    Extended payload length    |
|I|S|S|S|  (4)  |A|     (7)     |             (16/64)           |
|N|V|V|V|       |S|             |   (if payload len==126/127)   |
| |1|2|3|       |K|             |                               |
+-+-+-+-+-------+-+-------------+ - - - - - - - - - - - - - - - +
 4               5               6               7              
+ - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - +
|     Extended payload length continued, if payload len == 127  |
+ - - - - - - - - - - - - - - - +-------------------------------+
 8               9               10              11             
+ - - - - - - - - - - - - - - - +-------------------------------+
|                               |Masking-key, if MASK set to 1  |
+-------------------------------+-------------------------------+
 12              13              14              15
+-------------------------------+-------------------------------+
| Masking-key (continued)       |          Payload Data         |
+-------------------------------- - - - - - - - - - - - - - - - +
:                     Payload Data continued ...                :
+ - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - +
|                     Payload Data continued ...                |
+---------------------------------------------------------------+
```
*/

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FrameHeader {
	/// Final fragment in a message.
	pub fin: bool,
	/// Negotiated extension bits.
	///
	/// MUST be zero unless an extension is negotiated that defines meanings for non-zero values.
	pub exts: u8,
	/// Defines the interpretation of the payload data.
	///
	/// If an unknown opcode is received, the receiving endpoint MUST _Fail the WebSocket Connection_.
	pub opcode: Opcode,
	/// Defines whether the payload data is masked.
	///
	/// All frames sent from client to server must be masked.
	pub masking_key: Option<[u8; 4]>,
	/// The length of the payload data.
	pub payload_len: u64,
}
impl Default for FrameHeader {
	fn default() -> FrameHeader {
		FrameHeader {
			fin: false,
			exts: 0,
			opcode: Opcode::Continue,
			masking_key: None,
			payload_len: 0,
		}
	}
}

/// Decodes the frame header from the stream.
pub fn decode_frame_header(stream: &[u8], frame_header: &mut FrameHeader) -> Result<usize, Error> {
	// Read the first two control bytes.
	if stream.len() < 2 {
		return Err(Error::WouldBlock);
	}
	frame_header.fin = (stream[0] & 0x80) != 0;
	frame_header.exts = stream[0] & 0x70;
	frame_header.opcode = Opcode::try_from(stream[0] & 0x0F).map_err(|_| Error::UnknownOpCode)?;

	// All control frames MUST have a payload length of 125 bytes or less and MUST NOT be fragmented.
	if frame_header.opcode.is_control() && (!frame_header.fin || (stream[1] & 0x7F) > 125) {
		return Err(Error::BadControl);
	}

	// Extract the payload length
	let mut cursor = 2;
	frame_header.payload_len = match stream[1] & 0x7F {
		PAYLOAD_LEN_16 => {
			// Read the 16 bit payload length
			if stream.len() < cursor + 2 {
				return Err(Error::WouldBlock);
			}
			let len = (stream[cursor] as u16) << 8 | stream[cursor + 1] as u16;
			cursor += 2;
			// The minimal number of bytes MUST be used to encode the length
			if len < PAYLOAD_LEN_MIN_16 {
				return Err(Error::BadLength);
			}
			len as u64
		},
		PAYLOAD_LEN_64 => {
			// Read the 64 bit payload length
			if stream.len() < cursor + 8 {
				return Err(Error::WouldBlock);
			}
			let len =
				(stream[cursor + 0] as u64) << 56 |
				(stream[cursor + 1] as u64) << 48 |
				(stream[cursor + 2] as u64) << 40 |
				(stream[cursor + 3] as u64) << 32 |
				(stream[cursor + 4] as u64) << 24 |
				(stream[cursor + 5] as u64) << 16 |
				(stream[cursor + 6] as u64) << 8 |
				(stream[cursor + 7] as u64) << 0;
			cursor += 8;
			// The most significant bit MUST be 0
			if (len & 0x8000_0000_0000_0000) != 0 {
				return Err(Error::BadLength);
			}
			// The minimal number of bytes MUST be used to encode the length
			if len < PAYLOAD_LEN_MIN_64 {
				return Err(Error::BadLength);
			}
			len
		},
		len => {
			len as u64
		},
	};

	// Extract the masking key
	frame_header.masking_key = if (stream[1] & 0x80) != 0 {
		if stream.len() < cursor + 4 {
			return Err(Error::WouldBlock);
		}
		let mask = [
			stream[cursor + 0],
			stream[cursor + 1],
			stream[cursor + 2],
			stream[cursor + 3],
		];
		cursor += 4;
		Some(mask.into())
	}
	else {
		None
	};

	Ok(cursor)
}

pub fn encode_frame_header(frame_header: &FrameHeader, dest: &mut [u8]) -> Result<usize, Error> {
	// The most significant bit MUST be 0
	if (frame_header.payload_len & 0x8000_0000_0000_0000) != 0 {
		return Err(Error::BadLength);
	}

	// Construct the first control byte
	let word1 =
		if frame_header.fin { 0x80 } else { 0x00 } |
		u8::from(frame_header.exts) |
		u8::from(frame_header.opcode);

	// Construct the second control byte
	let mut word2 = if frame_header.payload_len < PAYLOAD_LEN_MIN_16 as u64 {
		frame_header.payload_len as u8
	}
	else if frame_header.payload_len < PAYLOAD_LEN_MIN_64 {
		PAYLOAD_LEN_16
	}
	else {
		PAYLOAD_LEN_64
	};
	if frame_header.masking_key.is_some() {
		word2 |= 0x80;
	}

	// Write the control word
	if dest.len() < 2 {
		return Err(Error::WouldBlock);
	}
	dest[0] = word1;
	dest[1] = word2;
	let mut cursor = 2;

	// Write the extended payload length
	if frame_header.payload_len >= PAYLOAD_LEN_MIN_64 {
		if dest.len() < cursor + 8 {
			return Err(Error::WouldBlock);
		}
		dest[cursor + 0] = (frame_header.payload_len >> 56) as u8;
		dest[cursor + 1] = (frame_header.payload_len >> 48) as u8;
		dest[cursor + 2] = (frame_header.payload_len >> 40) as u8;
		dest[cursor + 3] = (frame_header.payload_len >> 32) as u8;
		dest[cursor + 4] = (frame_header.payload_len >> 24) as u8;
		dest[cursor + 5] = (frame_header.payload_len >> 16) as u8;
		dest[cursor + 6] = (frame_header.payload_len >> 8) as u8;
		dest[cursor + 7] = (frame_header.payload_len >> 0) as u8;
		cursor += 8;
	}
	else if frame_header.payload_len >= PAYLOAD_LEN_MIN_16 as u64 {
		if dest.len() < cursor + 2 {
			return Err(Error::WouldBlock);
		}
		dest[cursor + 0] = (frame_header.payload_len >> 8) as u8;
		dest[cursor + 1] = (frame_header.payload_len >> 0) as u8;
		cursor += 2;
	}

	// Write the masking key
	if let Some(masking_key) = &frame_header.masking_key {
		if dest.len() < cursor + 4 {
			return Err(Error::WouldBlock);
		}
		dest[cursor + 0] = masking_key[0];
		dest[cursor + 1] = masking_key[1];
		dest[cursor + 2] = masking_key[2];
		dest[cursor + 3] = masking_key[3];
		cursor += 4;
	}

	Ok(cursor)
}

/// Decodes the frame payload into the destination buffer.
pub fn decode_payload(frame_header: &FrameHeader, stream: &[u8], dest: &mut [u8]) -> Result<usize, Error> {
	// The destination buffer must fit the payload length
	if dest.len() as u64 != frame_header.payload_len {
		return Err(Error::BadLength);
	}
	// Zero length payload fast path
	if dest.len() == 0 {
		return Ok(0);
	}
	// If the stream does not contain enough bytes, cannot proceed
	if stream.len() < dest.len() {
		return Err(Error::WouldBlock);
	}
	// Copy and unmask the payload
	if let Some(masking_key) = &frame_header.masking_key {
		for i in 0..dest.len() {
			dest[i] = stream[i] ^ masking_key[i & 3];
		}
	}
	// Fast path for non-masked payloads
	else {
		dest.copy_from_slice(&stream[..dest.len()]);
	}
	Ok(dest.len())
}

pub fn encode_payload(frame_header: &FrameHeader, payload: &[u8], dest: &mut [u8]) -> Result<usize, Error> {
	// Payload must equal the payload length
	if payload.len() as u64 != frame_header.payload_len {
		return Err(Error::BadLength);
	}
	// Zero length payload fast path
	if payload.len() == 0 {
		return Ok(0);
	}
	// If the destination is not large enough, cannot proceed
	if dest.len() < payload.len() {
		return Err(Error::BadLength);
	}
	// Copy and mask the payload
	if let Some(masking_key) = &frame_header.masking_key {
		for i in 0..payload.len() {
			dest[i] = payload[i] ^ masking_key[i & 3];
		}
	}
	// Fast path for non-masked payloads
	else {
		dest[..payload.len()].copy_from_slice(payload);
	}
	Ok(payload.len())
}
