/*!
WebSocket
=========
*/

#![cfg_attr(not(feature = "std"), no_std)]

use core::{fmt, mem, result};

mod http;
mod opcode;
mod frame;
mod stm;
mod ws;
mod error;
mod buffer;

use self::stm::{ConnSTM, FrameSTM};

pub use self::opcode::Opcode;
pub use self::frame::FrameHeader;
pub use self::error::Error;
pub use self::buffer::*;

pub type Result<T> = result::Result<T, Error>;

/// WebSocket event handlers.
///
/// # Control flow
///
/// This section describes the order and conditions in which these callbacks are invoked.
///
/// ## HTTP state
///
/// When the request line of a HTTP request is received the `http_request` callback is invoked.
/// If the HTTP version is anything but `HTTP/1.1` the connection is failed with a `HTTP Version Not Supported` error.
/// The callback can fail the connection on invalid or unsupported HTTP methods and inspect the request URI.
///
/// For each header in the HTTP headers the `http_header` callback is invoked.
/// The WebSocket relevant headers are processed automatically without failure.
/// The callback can fail the connection if desired.
///
/// After all headers are received the HTTP request is checked for a valid WebSocket upgrade request.
/// The connection is failed with a `Bad Request` error.
///
/// The `http_finish` callback is invoked.
/// The callback can decide to fail the connection based on the information received so far.
///
/// If the connection errors at this stage an appropriate HTTP error response is transmitted.
/// After this point no more implicit responses are transmitted.
///
/// The WebSocket is now fully upgraded to a WebSocket connection and WebSocket frames are exchanged.
///
/// ## WebSocket state
///
/// Every time enough bytes in the recv buffer are found to describe a frame header the `frame_header` callback is invoked.
/// This may cause the `frame_header` callback to be invoked multiple times for the same frame if the payload is not yet available.
/// Invalid frame headers (incorrect payload length encoding, invalid opcode) an error is returned before the `frame_header` callback is invoked.
///
/// When the recv buffer contains enough bytes for a frame header and payload the payload is decoded and passed to the `frame` callback.
///
/// Finally the particular ordering of frames is validated (in regards to opcodes and fin) and the `message` callback is invoked.
///
/// No message is automatically sent back in response to `Close` or `Ping` messages.
/// It is the responsibility of the application to respond appropriately.
#[allow(unused_variables)]
pub trait Handler {
	/// Handle the request line of a HTTP request.
	///
	/// Reject invalid or unsupported methods and inspect the request URI.
	fn http_request(&mut self, method: &str, uri: &str) -> Result<()> { Ok(()) }

	/// Handle the status line of a HTTP response.
	fn http_response(&mut self, status: i32, reason: &str) -> Result<()> { Ok(()) }

	/// Handle HTTP headers.
	///
	/// Reject requests with bad headers, wait for `http_finish` to validate relationship between headers.
	fn http_header(&mut self, key: &str, value: &str) -> Result<()> { Ok(()) }

	/// Handle the completed HTTP request.
	///
	/// Last chance to fail the HTTP request before a HTTP upgrade response is sent back.
	fn http_finish(&mut self) -> Result<()> { Ok(()) }

	/// Frame header was received.
	///
	/// Use this to enforce limits on payload size and extensions before the payload is received.
	/// The frame opcode has already been checked for invalid values.
	///
	/// Note that this may be called multiple times for the same frame if the payload is not yet available.
	fn frame_header(&mut self, ws: &mut WebSocketTx, header: &FrameHeader) -> Result<()> { Ok(()) }

	/// A completed frame was received.
	///
	/// The specific order of incoming frame opcodes is not yet validated, it is checked after this callback.
	fn frame(&mut self, ws: &mut WebSocketTx, header: &FrameHeader, payload: &[u8]) -> Result<()> { Ok(()) }

	/// A WebSocket message was received.
	fn message(&mut self, ws: &mut WebSocketTx, msg: Msg<'_>);
}

//----------------------------------------------------------------

/// WebSocket message.
#[derive(Copy, Clone, Debug)]
pub enum Msg<'a> {
	Text(&'a str),
	Binary(&'a [u8]),
	Close(u16, Option<&'a str>),
	Ping(&'a [u8]),
	Pong(&'a [u8]),
}

/// WebSocket close codes.
pub mod close {
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
}

//----------------------------------------------------------------

// This trait makes the Handler independent of the buffer strategy
trait WebSocketTxTrait {
	fn len(&self) -> usize;
	fn append(&mut self, data: &[u8]);
}

/// WebSocket transmit interface.
///
/// This struct is nothing more than a simple wrapper around a buffer.
/// It provides helper methods to serialize WebSocket messages.
///
/// The application is responsible for periodically draining this buffer and sending its contents over to its destination.
#[repr(transparent)]
pub struct WebSocketTx(dyn WebSocketTxTrait);

struct WebSocketInner<B: BufferStrategy> {
	txbuffer: B::TxBuffer,
}
impl<B: BufferStrategy> WebSocketTxTrait for WebSocketInner<B> {
	#[inline]
	fn len(&self) -> usize {
		self.txbuffer.len()
	}
	#[inline]
	fn append(&mut self, data: &[u8]) {
		self.txbuffer.append(data);
	}
}
impl<B: BufferStrategy> WebSocketInner<B> {
	#[inline]
	fn as_tx(&mut self) -> &mut WebSocketTx {
		let this: &mut dyn WebSocketTxTrait = self;
		unsafe { mem::transmute(this) }
	}
	#[inline]
	fn write_fmt(&mut self, fmt: fmt::Arguments<'_>) {
		self.txbuffer.write_fmt(fmt)
	}
}

/// WebSocket state.
pub struct WebSocket<B: BufferStrategy> {
	/// Transmit buffer with outgoing bytes.
	/// Also contains the strategy.
	inner: WebSocketInner<B>,
	/// Recv buffer with incoming bytes.
	rxbuffer: B::RxBuffer,
	/// HTTP WebSocket handshake state.
	handshake: http::Handshake,
	/// WebSocket state machine.
	stm: ConnSTM,
	/// Buffer for fragmented messages.
	payload: B::MsgBuffer,
}
