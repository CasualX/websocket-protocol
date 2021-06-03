use core::str;
use crate::*;

pub(crate) fn handle_frame<B: BufferStrategy>(buffer: &mut [u8], ws_rx: FrameSTM, ws: &mut WebSocket<B>, handler: &mut dyn Handler) -> Result<usize> {
	// Decode just the frame header
	let mut header = FrameHeader::default();
	let mut consumed = FrameHeader::decode_into(buffer, &mut header)?;
	handler.frame_header(ws.inner.as_tx(), &header)?;

	// No extensions are supported
	if header.extensions != 0 {
		return Err(Error::UnsupportedExtension(header.extensions));
	}

	// Follow up by decoding the frame payload
	// If not enough bytes are available then wait for more bytes
	// WARNING! This means that the frame header handler can be called multiple times!
	let payload = &mut buffer[consumed..];
	let payload_len = decode_frame_payload_mut(&header, payload)?;
	consumed += payload_len;
	let payload = &payload[..payload_len];

	// Let the handler know a full frame was received
	handler.frame(ws.inner.as_tx(), &header, payload)?;

	// Translate the frame into a message
	match header.opcode {
		crate::Opcode::CONTINUE => {
			ws.payload.append(payload);
			if header.fin {
				unimplemented!()
			}
		},
		crate::Opcode::TEXT => {
			if header.fin {
				handler.message(ws.inner.as_tx(), crate::Msg::Text(str::from_utf8(payload)?));
			}
			else {
				ws.payload.append(payload);
			}
		},
		crate::Opcode::BINARY => {
			if header.fin {
				handler.message(ws.inner.as_tx(), crate::Msg::Binary(payload));
			}
			else {
				ws.payload.append(payload);
			}
		},
		crate::Opcode::CLOSE => {
			let (close, reason) = if payload.len() < 2 {
				(crate::close::CLOSE_NORMAL, None)
			}
			else {
				(u16::from_le_bytes([payload[0], payload[1]]), Some(str::from_utf8(&payload[2..])?))
			};
			handler.message(ws.inner.as_tx(), crate::Msg::Close(close, reason));
		},
		crate::Opcode::PING => {
			handler.message(ws.inner.as_tx(), crate::Msg::Ping(payload));
		},
		crate::Opcode::PONG => {
			handler.message(ws.inner.as_tx(), crate::Msg::Pong(payload));
		},
		crate::Opcode { byte } => {
			return Err(crate::Error::UnknownOpcode(byte));
		},
	}

	// When no errors have occurred navigate the websocket state machine
	ws.stm = ConnSTM::WebSocket(ws_rx.next(&header));

	Ok(consumed)
}

/// Magic value indicating 16bit payload length.
///
/// When the payload length field contains this value, the next 2 bytes contain the extended payload length in network endian.
const PAYLOAD_LEN_16: u8 = 126;
/// Magic value indicating 64bit payload length.
///
/// When the payload length field contains this value, the next 8 bytes contain the extended payload length in network endian.
const PAYLOAD_LEN_64: u8 = 127;

/// Minimum 16-bit payload length.
///
/// The payload length must be minimally encoded, if it is any less it must be encoded as such.
const PAYLOAD_LEN_MIN_16: u16 = 126;
/// Minimum 64-bit payload length.
///
/// The payload length must be minimally encoded, if it is any less it must be encoded as such.
const PAYLOAD_LEN_MIN_64: u64 = 65536;

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

/// WebSocket frame header.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FrameHeader {
	/// Final fragment in a message.
	pub fin: bool,
	/// Negotiated extension bits.
	///
	/// MUST be zero unless an extension is negotiated that defines meanings for non-zero values.
	/// NB. No extensions are supported, non-zero values are rejected while processing frames.
	pub extensions: u8,
	/// Defines the interpretation of the payload data.
	///
	/// If an unknown opcode is received, the receiving endpoint MUST _Fail the WebSocket Connection_.
	/// NB. This is validated by construction in this library, you'll never see invalid opcodes.
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
			opcode: Opcode::CONTINUE,
			masking_key: None,
			payload_len: 0,
		}
	}
}

impl FrameHeader {
	pub const fn size(&self) -> usize {
		let mut len = 2; // two control words

		if self.payload_len >= PAYLOAD_LEN_MIN_64 {
			len += 8;
		}
		else if self.payload_len >= PAYLOAD_LEN_16 as u64 {
			len += 2;
		}

		if self.masking_key.is_some() {
			len += 4;
		}

		len
	}
}

impl FrameHeader {
	pub(crate) fn decode_into(buffer: &[u8], dest: &mut FrameHeader) -> Result<usize> {
		decode_frame_header(buffer, dest)
	}
	// pub fn decode(buffer: &[u8]) -> Result<(usize, FrameHeader)> {
	// 	unimplemented!()
	// }
	pub(crate) fn encode(&self, dest: &mut [u8]) -> Result<usize> {
		encode_frame_header(self, dest)
	}
}

//----------------------------------------------------------------

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
/// use websock as wsproto;
///
/// let stream = &[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
///
/// let mut header = wsproto::FrameHeader::default();
/// assert_eq!(Ok(2), wsproto::decode_frame_header(stream, &mut header));
///
/// let expected_header = wsproto::FrameHeader {
/// 	fin: true,
/// 	extensions: 0,
/// 	opcode: wsproto::Opcode::Text,
/// 	masking_key: None,
/// 	payload_len: 5,
/// };
/// assert_eq!(expected_header, header);
/// ```
///
/// A single-frame masked text message:
///
/// ```
/// use websock as wsproto;
///
/// let stream = &[0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58];
///
/// let mut header = wsproto::FrameHeader::default();
/// assert_eq!(Ok(6), wsproto::decode_frame_header(stream, &mut header));
///
/// let expected_header = wsproto::FrameHeader {
/// 	fin: true,
/// 	extensions: 0,
/// 	opcode: wsproto::Opcode::Text,
/// 	masking_key: Some([0x37, 0xfa, 0x21, 0x3d]),
/// 	payload_len: 5,
/// };
/// assert_eq!(expected_header, header);
/// ```
pub fn decode_frame_header(stream: &[u8], header: &mut FrameHeader) -> Result<usize> {
	// Read the first two control bytes.
	if stream.len() < 2 {
		return Err(Error::WouldBlock);
	}
	header.fin = (stream[0] & 0x80) != 0;
	header.extensions = stream[0] & 0x70;
	let opcode = stream[0] & 0x0F;
	header.opcode = Opcode(opcode);
	if !header.opcode.is_valid() {
		return Err(Error::UnknownOpcode(opcode));
	}

	// All control frames MUST have a payload length of 125 bytes or less and MUST NOT be fragmented.
	if header.opcode.is_control() && (!header.fin || (stream[1] & 0x7F) > 125) {
		return Err(Error::BadControlFrame);
	}

	// Extract the payload length
	let mut cursor = 2;
	header.payload_len = match stream[1] & 0x7F {
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
	header.masking_key = if (stream[1] & 0x80) != 0 {
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

/// Decodes the frame payload inplace.
///
/// # Return value
///
/// * Returns `Ok(n)` if the frame payload was successfully decoded consuming `n` bytes from the stream.
///   The returned size is equal to the payload length.
///
/// * Returns `Err(WouldBlock)` if the input stream does not contain at least `header.payload_len` bytes.
///   Accumulate more data from the source before trying to decode again.
///
/// # Examples
///
/// ```
/// use websock as wsproto;
///
/// // A single-frame masked text message: "Hello"
/// let mut stream = [0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58];
///
/// // Decode the frame header
/// let mut header = wsproto::FrameHeader::default();
/// assert_eq!(Ok(6), wsproto::decode_frame_header(&stream, &mut header));
///
/// // Decode the frame payload inplace
/// assert_eq!(Ok(5), wsproto::decode_frame_payload_mut(&header, &mut stream[6..]));
/// assert_eq!(b"Hello", &stream[6..]);
/// ```
pub fn decode_frame_payload_mut(header: &FrameHeader, buffer: &mut [u8]) -> Result<usize> {
	// Verify there is enough data in the stream
	if header.payload_len > buffer.len() as u64 {
		return Err(Error::WouldBlock);
	}
	let stream = &mut buffer[..header.payload_len as usize];
	// Unmask the payload if required
	if let Some(masking_key) = &header.masking_key {
		for i in 0..stream.len() {
			stream[i] ^= masking_key[i % 4];
		}
	}
	Ok(stream.len())
}

pub fn encode_frame_header(header: &FrameHeader, buffer: &mut [u8]) -> Result<usize> {
	// The most significant bit MUST be 0
	if (header.payload_len & 0x8000_0000_0000_0000) != 0 {
		return Err(Error::BadPayloadLength);
	}

	// Construct the first control byte
	let word1 =
		if header.fin { 0x80 } else { 0x00 } |
		u8::from(header.extensions) |
		u8::from(header.opcode);

	// Construct the second control byte
	let mut word2 = if header.payload_len < PAYLOAD_LEN_MIN_16 as u64 {
		header.payload_len as u8
	}
	else if header.payload_len < PAYLOAD_LEN_MIN_64 {
		PAYLOAD_LEN_16
	}
	else {
		PAYLOAD_LEN_64
	};
	if header.masking_key.is_some() {
		word2 |= 0x80;
	}

	// Write the control word
	if buffer.len() < 2 {
		return Err(Error::WouldBlock);
	}
	buffer[0] = word1;
	buffer[1] = word2;
	let mut cursor = 2;

	// Write the extended payload length
	if header.payload_len >= PAYLOAD_LEN_MIN_64 {
		if buffer.len() < cursor + 8 {
			return Err(Error::WouldBlock);
		}
		buffer[cursor + 0] = (header.payload_len >> 56) as u8;
		buffer[cursor + 1] = (header.payload_len >> 48) as u8;
		buffer[cursor + 2] = (header.payload_len >> 40) as u8;
		buffer[cursor + 3] = (header.payload_len >> 32) as u8;
		buffer[cursor + 4] = (header.payload_len >> 24) as u8;
		buffer[cursor + 5] = (header.payload_len >> 16) as u8;
		buffer[cursor + 6] = (header.payload_len >> 8) as u8;
		buffer[cursor + 7] = (header.payload_len >> 0) as u8;
		cursor += 8;
	}
	else if header.payload_len >= PAYLOAD_LEN_MIN_16 as u64 {
		if buffer.len() < cursor + 2 {
			return Err(Error::WouldBlock);
		}
		buffer[cursor + 0] = (header.payload_len >> 8) as u8;
		buffer[cursor + 1] = (header.payload_len >> 0) as u8;
		cursor += 2;
	}

	// Write the masking key
	if let Some(masking_key) = &header.masking_key {
		if buffer.len() < cursor + 4 {
			return Err(Error::WouldBlock);
		}
		buffer[cursor + 0] = masking_key[0];
		buffer[cursor + 1] = masking_key[1];
		buffer[cursor + 2] = masking_key[2];
		buffer[cursor + 3] = masking_key[3];
		cursor += 4;
	}

	Ok(cursor)
}
