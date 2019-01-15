/*!
WebSocket Protocol
==================

Defines a low-level webSocket protocol implementation without any reference to networking or async io.

Examples
--------

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
	pub fn new(opcode: Opcode, payload: &'a [u8]) -> MsgResult<'a> {
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
pub type MsgResult<'a> = Result<Msg<'a>, str::Utf8Error>;

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
	pub fn recv(&mut self, stream: &[u8], max_message_size: usize) -> Result<usize, Error> {
		// Early reject
		if stream.len() == 0 {
			return Err(Error::WouldBlock);
		}
		// Decode the frame header
		let mut frame_header = FrameHeader::default();
		let frame_header_len = decode_frame_header(stream, &mut frame_header)?;
		// Allocate memory in the buffer for the frame
		let start = self.buffer.len();
		let end = start + frame_header.payload_len as usize;
		if self.buffer.capacity() < end {
			let additional = end - self.buffer.capacity();
			self.buffer.reserve(additional);
		}
		// Decode the payload into the buffer
		unsafe {
			let dest = core::slice::from_raw_parts_mut(self.buffer.as_mut_ptr().offset(start as isize), end - start);
			decode_frame_payload(&frame_header, &stream[frame_header_len..], dest)?;
			self.buffer.set_len(end);
		}
		// Stich together a WebSocket message
		self.fin = frame_header.fin;
		
		unimplemented!()
	}
	pub fn reset(&mut self) {
		self.fin = true;
		self.opcode = Opcode::Continue;
		self.buffer.clear();
	}
	/// Returns true if a complete WebSocket message has been assembled.
	pub fn fin(&self) -> bool {
		self.fin && self.opcode != Opcode::Continue
	}
	pub fn as_msg(&self) -> Option<Result<Msg<'_>, str::Utf8Error>> {
		if self.fin() {
			Some(Msg::new(self.opcode, &self.buffer))
		}
		else {
			None
		}
	}
}

/// The WebSocket receiver state machine.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WebSocketRx {
	Handshake,
	/// Initial state on construction.
	Init,
	/// A completed message is available with the given opcode.
	Fin { opcode: Opcode },
	/// A fragmented message is being transmitted.
	Frag { frag_opcode: Opcode },
	/// A control frame was received in the middle of a fragmented message.
	Ctrl { ctrl_opcode: Opcode, frag_opcode: Opcode },
	/// A frame was received which is not valid.
	Invalid,
}
impl WebSocketRx {
	pub fn next(self, frame_header: &FrameHeader) -> WebSocketRx {
		match self {
			// The Handshake state is a convenience representing the http upgrade request
			// It does not participate in the WebSocket state machine
			WebSocketRx::Handshake => {
				WebSocketRx::Invalid
			},
			// From the Init and Fin state:
			// * Continue opcode is invalid, as there is no previous frame that started a fragmented message
			// * Fin bit indicates a completed message arrived in a single frame
			// * Otherwise a fragmented message has started, throw out any fragmented control frames
			WebSocketRx::Init | WebSocketRx::Fin { .. } => {
				if frame_header.opcode == Opcode::Continue {
					WebSocketRx::Invalid
				}
				else if frame_header.fin {
					WebSocketRx::Fin { opcode: frame_header.opcode }
				}
				else if frame_header.opcode.is_control() {
					WebSocketRx::Invalid
				}
				else {
					WebSocketRx::Frag { frag_opcode: frame_header.opcode }
				}
			},
			// When expecting a fragmented message:
			// * Control frames are allowed to intersperse the fragmented message, keep track of the fragmented opcode
			// * If not a control frame the opcode must be Continue
			// * Fin bit indicates the final frame, otherwise expect more fragmented frames
			WebSocketRx::Frag { frag_opcode } => {
				if frame_header.opcode.is_control() {
					if frame_header.fin {
						WebSocketRx::Ctrl { ctrl_opcode: frame_header.opcode, frag_opcode }
					}
					else {
						WebSocketRx::Invalid
					}
				}
				else if frame_header.opcode == Opcode::Continue {
					if frame_header.fin {
						WebSocketRx::Fin { opcode: frag_opcode }
					}
					else {
						WebSocketRx::Frag { frag_opcode }
					}
				}
				else {
					WebSocketRx::Invalid
				}
			},
			// Handling a control message in between fragmented frames:
			// 
			WebSocketRx::Ctrl { ctrl_opcode: _, frag_opcode } => {
				if frame_header.opcode.is_control() {
					if frame_header.fin {
						WebSocketRx::Ctrl { ctrl_opcode: frame_header.opcode, frag_opcode }
					}
					else {
						WebSocketRx::Invalid
					}
				}
				else if frame_header.opcode == Opcode::Continue {
					if frame_header.fin {
						WebSocketRx::Fin { opcode: frag_opcode }
					}
					else {
						WebSocketRx::Frag { frag_opcode }
					}
				}
				else {
					WebSocketRx::Invalid
				}
			},
			WebSocketRx::Invalid => {
				WebSocketRx::Invalid
			},
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

/// Minimum 16-bit payload length.
///
/// The payload length must be minimally encoded, if it is any less it must be encoded as such.
pub const PAYLOAD_LEN_MIN_16: u16 = 126;
/// Minimum 64-bit payload length.
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
	BadPayloadLength,
	/// Opcode: [Section 5.2](https://tools.ietf.org/html/rfc6455#section-5.2).
	///
	/// If an unknown opcode is received, the receiving endpoint MUST _Fail the WebSocket Connection_.
	UnknownOpcode,
	/// Control Frames: [Section 5.5](https://tools.ietf.org/html/rfc6455#section-5.5).
	///
	/// All control frames MUST have a payload length of 125 bytes or less and MUST NOT be fragmented.
	BadControlFrame,
	UnsupportedExtension,
	LengthMismatch,
	/// Not enough data in the input stream.
	///
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
	pub extensions: u8,
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
			extensions: 0,
			opcode: Opcode::Continue,
			masking_key: None,
			payload_len: 0,
		}
	}
}

pub fn frame_size(frame_header: &FrameHeader) -> usize {
	let mut len = 2; // two control words

	if frame_header.payload_len >= PAYLOAD_LEN_MIN_64 {
		len += 8;
	}
	else if frame_header.payload_len >= PAYLOAD_LEN_16 as u64 {
		len += 2;
	}

	if frame_header.masking_key.is_some() {
		len += 4;
	}

	len
}

/// Decodes a WebSocket Frame Header from the stream.
///
/// # Return value
///
/// * Returns `Ok(n)` if a frame header was successfully decoded consuming `n` bytes from the stream.
///   The returned size is guaranteed to be greater than 0 and less than the stream length.
///
/// * Returns `Err(WouldBlock)` if there were not enough bytes in the stream to fully decode the frame header.
///   Accumulate more data from the source before trying to decode again.
///
/// * Other errors may be returned if the header violates the WebSocket framing protocol.
///
/// # Examples
///
/// A single-frame unmasked text message:
///
/// ```
/// use websocket_protocol as wsproto;
///
/// let stream = &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
///
/// let mut frame_header = wsproto::FrameHeader::default();
/// assert_eq!(Ok(2), wsproto::decode_frame_header(stream, &mut frame_header));
///
/// let expected_header = wsproto::FrameHeader {
/// 	fin: true,
/// 	extensions: 0,
/// 	opcode: wsproto::Opcode::Text,
/// 	masking_key: None,
/// 	payload_len: 5,
/// };
/// assert_eq!(expected_header, frame_header);
/// ```
///
/// A single-frame masked text message:
///
/// ```
/// use websocket_protocol as wsproto;
///
/// let stream = &[0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58];
///
/// let mut frame_header = wsproto::FrameHeader::default();
/// assert_eq!(Ok(6), wsproto::decode_frame_header(stream, &mut frame_header));
///
/// let expected_header = wsproto::FrameHeader {
/// 	fin: true,
/// 	extensions: 0,
/// 	opcode: wsproto::Opcode::Text,
/// 	masking_key: Some([0x37, 0xfa, 0x21, 0x3d]),
/// 	payload_len: 5,
/// };
/// assert_eq!(expected_header, frame_header);
/// ```
pub fn decode_frame_header(stream: &[u8], frame_header: &mut FrameHeader) -> Result<usize, Error> {
	// Read the first two control bytes.
	if stream.len() < 2 {
		return Err(Error::WouldBlock);
	}
	frame_header.fin = (stream[0] & 0x80) != 0;
	frame_header.extensions = stream[0] & 0x70;
	frame_header.opcode = Opcode::try_from(stream[0] & 0x0F).map_err(|_| Error::UnknownOpcode)?;

	// All control frames MUST have a payload length of 125 bytes or less and MUST NOT be fragmented.
	if frame_header.opcode.is_control() && (!frame_header.fin || (stream[1] & 0x7F) > 125) {
		return Err(Error::BadControlFrame);
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
				return Err(Error::BadPayloadLength);
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
				return Err(Error::BadPayloadLength);
			}
			// The minimal number of bytes MUST be used to encode the length
			if len < PAYLOAD_LEN_MIN_64 {
				return Err(Error::BadPayloadLength);
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
		Some(mask)
	}
	else {
		None
	};

	Ok(cursor)
}

pub fn encode_frame_header(frame_header: &FrameHeader, dest: &mut [u8]) -> Result<usize, Error> {
	// The most significant bit MUST be 0
	if (frame_header.payload_len & 0x8000_0000_0000_0000) != 0 {
		return Err(Error::BadPayloadLength);
	}

	// Construct the first control byte
	let word1 =
		if frame_header.fin { 0x80 } else { 0x00 } |
		u8::from(frame_header.extensions) |
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
///
/// Copies the payload from the stream to the destination buffer.
/// If a masking key is present in the frame header the payload is unmasked.
///
/// # Return value
///
/// * Returns `Ok(n)` if the frame payload was successfully decoded consuming `n` bytes from the stream.
///   The returned size is equal to the frame header's payload_len.
///
/// * Returns `Err(WouldBlock)` if the input stream does not contain at least `frame_header.payload_len` bytes.
///   Accumulate more data from the source before trying to decode again.
///
/// * Returns `Err(LengthMismatch)` if the destination buffer length does not match the frame header's payload length.
///   Callers are encouraged to set a reasonable frame payload size limit.
///
pub fn decode_frame_payload(frame_header: &FrameHeader, stream: &[u8], dest: &mut [u8]) -> Result<usize, Error> {
	// The destination buffer must fit the payload length
	if dest.len() as u64 != frame_header.payload_len {
		return Err(Error::LengthMismatch);
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

pub fn encode_frame_payload(frame_header: &FrameHeader, payload: &[u8], dest: &mut [u8]) -> Result<usize, Error> {
	// Payload must equal the payload length
	if payload.len() as u64 != frame_header.payload_len {
		return Err(Error::LengthMismatch);
	}
	// Zero length payload fast path
	if payload.len() == 0 {
		return Ok(0);
	}
	// If the destination is not large enough, cannot proceed
	if dest.len() < payload.len() {
		return Err(Error::LengthMismatch);
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

/// Decodes the frame payload inplace.
///
/// # Return value
///
/// * Returns `Ok(n)` if the frame payload was successfully decoded consuming `n` bytes from the stream.
///   The returned size is equal to the payload length.
///
/// * Returns `Err(WouldBlock)` if the input stream does not contain at least `frame_header.payload_len` bytes.
///   Accumulate more data from the source before trying to decode again.
///
/// # Examples
///
/// ```
/// use websocket_protocol as wsproto;
///
/// // A single-frame masked text message: "Hello"
/// let mut stream = [0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58];
///
/// // Decode the frame header
/// let mut frame_header = wsproto::FrameHeader::default();
/// assert_eq!(Ok(6), wsproto::decode_frame_header(&stream, &mut frame_header));
///
/// // Decode the frame payload inplace
/// assert_eq!(Ok(5), wsproto::decode_frame_payload_mut(&frame_header, &mut stream[6..]));
/// assert_eq!(b"Hello", &stream[6..]);
/// ```
pub fn decode_frame_payload_mut(frame_header: &FrameHeader, stream: &mut [u8]) -> Result<usize, Error> {
	// Verify there is enough data in the stream
	if frame_header.payload_len > stream.len() as u64 {
		return Err(Error::WouldBlock);
	}
	let stream = &mut stream[..frame_header.payload_len as usize];
	// Unmask the payload if required
	if let Some(masking_key) = &frame_header.masking_key {
		for i in 0..stream.len() {
			stream[i] ^= masking_key[i % 4];
		}
	}
	Ok(stream.len())
}

/// Decodes a frame inplace calling the closure with the decoded frame header and its payload.
///
/// If the frame is masked the payload is decoded inplace, mutating the stream.
/// If successful ensure to throw away the number of bytes returned, do not try to decode these bytes again.
///
/// # Return value
///
/// * Returns `Ok(n)` if the frame was successfully decoded and the closure called consuming `n` bytes from the stream.
///   The returned size is equal to the length of the decoded frame.
///
/// * Returns `Err(WouldBlock)` if the input stream does not contain enough bytes for a complete frame.
///   Accumulate more data from the source before trying to decode again.
///
/// # Examples
///
/// ```
/// use websocket_protocol as wsproto;
///
/// // A single-frame masked text message: "Hello"
/// let mut stream = [0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58];
///
/// let mut asserted = false;
/// let result = wsproto::decode_frame_mut(&mut stream, |frame_header, payload| {
/// 	assert_eq!(b"Hello", payload);
/// 	asserted = true;
/// });
///
/// assert_eq!(Ok(stream.len()), result);
/// assert!(asserted);
/// ```
pub fn decode_frame_mut<'a, F: FnMut(&FrameHeader, &'a [u8])>(stream: &'a mut [u8], mut f: F) -> Result<usize, Error> {
	// Decode a WebSocket frame header
	let mut frame_header = FrameHeader::default();
	let header_len = decode_frame_header(stream, &mut frame_header)?;
	// Decode the WebSocket frame payload
	let stream = &mut stream[header_len..];
	let payload_len = decode_frame_payload_mut(&frame_header, stream)?;
	let payload = &stream[..payload_len];
	// Notify the caller and return the number of bytes consumed
	f(&frame_header, payload);
	Ok(header_len + payload_len)
}

/// Decodes frames inplace, calling the closure for each decoded frame and its payload.
///
/// Decoding stops when the inner `decode_frame_mut` returns `Err(WouldBlock)` at which the total number of consumed bytes are returned.
/// If successful ensure to throw away the number of bytes returned, do not try to decode these bytes again.
///
/// # Return value
///
/// * Returns `Ok(n)` if all the frames were successfully decoded until an `Err(WouldBlock)` was returned.
///   The returned size is equal to the sum of sizes of the decoded frames, if this is `0` no frames have been decoded.
///
/// Never returns `Err(WouldBlock)` as its purpose is to consume until all (zero or more) frames have been decoded.
///
/// # Examples
///
/// ```
/// use websocket_protocol as wsproto;
///
/// // Two text frames, one masked and the other not masked, each containing a "Hello" payload
/// let mut stream = [
/// 	0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f,
/// 	0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
/// ];
///
/// let mut hits = 0;
/// let result = wsproto::decode_frames_mut(&mut stream, |frame_header, payload| {
/// 	assert_eq!(b"Hello", payload);
/// 	hits += 1;
/// });
///
/// assert_eq!(2, hits);
/// assert_eq!(Ok(stream.len()), result);
/// ```
pub fn decode_frames_mut<'a, F: FnMut(&FrameHeader, &'a [u8])>(mut stream: &'a mut [u8], mut f: F) -> Result<usize, Error> {
	let mut consumed = 0;
	loop {
		// Why is this unsafe casting necessary?
		// Since the stream's lifetime and the callback's payload are tagged with the same lifetime,
		// Rust correctly infers that the slice could be leaked there, causing any further mutation of the stream potentially aliasing.
		// However in our case this doesn't happen because the slices are distinct.
		// The slice going into the closure is the next frame in the stream, but we want to keep looking for frames after that.
		// Essentially this wants to split the stream at `frame_len` but we only know that length _after_ we call `decode_frame_mut`.
		// Which could return the split off slices instead of just a length! But such is the consequences of the choice in API.
		// Alternatively the two references could not be tagged with the same lifetime (ensuring the stream reference cannot leak).
		match decode_frame_mut(unsafe { &mut *(stream as *mut _) }, &mut f) {
			Ok(frame_len) => {
				stream = &mut stream[frame_len..];
				consumed += frame_len;
			},
			Err(err) => {
				// Having consumed all there is from the input stream, we are satisfied
				if err == Error::WouldBlock {
					return Ok(consumed);
				}
				// Something happened, cannot proceed
				else {
					return Err(err);
				}
			},
		}
	}
}
