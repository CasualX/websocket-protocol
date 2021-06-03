use core::{fmt, str};
#[cfg(feature = "std")]
use std::error;

/// WebSocket errors.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Error {
	/// The operation needs to block to complete, but the blocking operation was requested to not occur.
	WouldBlock,

	/// Abort the HTTP connection without sending any response.
	///
	/// Note that this is quite rude to the other side of the connection.
	HttpAbort,

	/// HTTP 4xx client error.
	HttpClientError(u8),

	/// HTTP 5xx server error.
	HttpServerError(u8),

	/// There was an error parsing UTF8 encoded text.
	Utf8Error,

	/// There was an error parsing the frame header.
	FrameHeaderError,

	/// Payload length error: [Section 5.2](https://tools.ietf.org/html/rfc6455#section-5.2).
	///
	/// When 64bit length, the most significant bit MUST be 0.
	///
	/// The minimal number of bytes MUST be used to encode the length.
	BadPayloadLength,

	/// Opcode: [Section 5.2](https://tools.ietf.org/html/rfc6455#section-5.2).
	///
	/// If an unknown opcode is received, the receiving endpoint MUST _Fail the WebSocket Connection_.
	UnknownOpcode(u8),

	/// This extension is not supported.
	UnsupportedExtension(u8),

	/// Control Frames: [Section 5.5](https://tools.ietf.org/html/rfc6455#section-5.5).
	///
	/// All control frames MUST have a payload length of 125 bytes or less and MUST NOT be fragmented.
	BadControlFrame,
}

/// HTTP status codes.
impl Error {
	pub const BAD_REQUEST: Error = Error::HttpClientError(0);
	pub const UNAUTHORIZED: Error = Error::HttpClientError(1);
	pub const FORBIDDEN: Error = Error::HttpClientError(3);
	pub const NOT_FOUND: Error = Error::HttpClientError(4);
	pub const METHOD_NOT_ALLOWED: Error = Error::HttpClientError(5);
	pub const REQUEST_TIMEOUT: Error = Error::HttpClientError(8);
	pub const REQUEST_ENTITY_TOO_LARGE: Error = Error::HttpClientError(13);
	pub const REQUEST_URI_TOO_LONG: Error = Error::HttpClientError(14);

	pub const INTERNAL_SERVER_ERROR: Error = Error::HttpServerError(0);
	pub const NOT_IMPLEMENTED: Error = Error::HttpServerError(1);
	pub const SERVICE_UNAVAILABLE: Error = Error::HttpServerError(3);
	pub const HTTP_VERSION_NOT_SUPPORTED: Error = Error::HttpServerError(5);
}

impl Error {
	pub fn description(self) -> &'static str {
		match self {
			Error::WouldBlock => "Would Block",

			Error::HttpAbort => "HTTP Abort",

			Error::BAD_REQUEST => "Bad Request",
			Error::UNAUTHORIZED => "Unauthorized",
			Error::FORBIDDEN => "Forbidden",
			Error::NOT_FOUND => "Not Found",
			Error::METHOD_NOT_ALLOWED => "Method Not Allowed",
			Error::REQUEST_TIMEOUT => "Request Timeout",
			Error::REQUEST_ENTITY_TOO_LARGE => "Request Entity Too Large",
			Error::REQUEST_URI_TOO_LONG => "Request-URI Too Long",
			Error::HttpClientError(_) => "HTTP Client Error",

			Error::INTERNAL_SERVER_ERROR => "Internal Server Error",
			Error::NOT_IMPLEMENTED => "Not Implemented",
			Error::SERVICE_UNAVAILABLE => "Service Unavailable",
			Error::HTTP_VERSION_NOT_SUPPORTED => "HTTP Version Not Supported",
			Error::HttpServerError(_) => "HTTP Server Error",

			Error::Utf8Error => "Utf8 Error",
			Error::FrameHeaderError => "Frame Header Error",
			Error::BadPayloadLength => "Bad Payload Length",
			Error::UnknownOpcode(_) => "Invalid Opcode",
			Error::UnsupportedExtension(_) => "Unsupported Extension",
			Error::BadControlFrame => "Bad Control Frame",
		}
	}
}

impl From<str::Utf8Error> for Error {
	fn from(_: str::Utf8Error) -> Error {
		Error::Utf8Error
	}
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(self.description())
	}
}

#[cfg(feature = "std")]
impl error::Error for Error {}
