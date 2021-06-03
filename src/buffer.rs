use core::{fmt, mem, slice};

/// Implementation details of the WebSocket buffers.
pub trait BufferStrategy {
	/// Receive buffer.
	///
	/// This buffer holds onto incoming bytes which may contain partial inputs.
	type RxBuffer;

	/// Transmit buffer.
	///
	/// This buffer holds the outgoing bytes before they're sent back.
	type TxBuffer;

	/// Message buffer
	///
	/// Fragmented frames accumulate their payload here.
	type MsgBuffer;

	fn new_rx(&self) -> Self::RxBuffer;
	fn new_tx(&self) -> Self::TxBuffer;
	fn new_msg(&self) -> Self::MsgBuffer;

	fn recv_rx<'a>(&self, this: &'a mut Self::RxBuffer) -> &'a mut [u8];
	fn recvn_rx(&self, this: &mut Self::RxBuffer, n: usize);
	fn consume_rx(&self, this: &mut Self::RxBuffer, n: usize);
	fn get_rx<'a>(&self, this: &'a mut Self::RxBuffer, offset: usize) -> &'a mut [u8];
	fn take_rx(&self, this: &mut Self::RxBuffer) -> Self::RxBuffer;
	fn store_rx(&self, this: &mut Self::RxBuffer, buffer: Self::RxBuffer);

	fn len_tx(&self, this: &Self::TxBuffer) -> usize;
	fn append_tx(&self, this: &mut Self::TxBuffer, data: &[u8]);
	fn consume_tx(&self, this: &mut Self::TxBuffer, n: usize);
	fn bytes_tx<'a>(&self, this: &'a Self::TxBuffer) -> &'a [u8];
	fn write_fmt_tx(&self, this: &mut Self::TxBuffer, fmt: fmt::Arguments<'_>);

	fn append_msg(&self, this: &mut Self::MsgBuffer, payload: &[u8]);
	fn get_msg<'a>(&self, this: &'a Self::MsgBuffer) -> &'a [u8];
	fn clear_msg(&self, this: &mut Self::MsgBuffer);
}

//----------------------------------------------------------------

/// Buffer strategy using std lib's [`Vec`] type.
///
/// The receive buffer uses a preallocated `Vec` backed with `capacity` bytes.
/// It will never grow, the whole HTTP request and each frame are limited to this max size.
#[cfg(feature = "std")]
pub struct VecStrategy {
	pub capacity: usize,
}
#[cfg(feature = "std")]
impl BufferStrategy for VecStrategy {
	type RxBuffer = Vec<u8>;
	type TxBuffer = Vec<u8>;
	type MsgBuffer = Vec<u8>;

	#[inline]
	fn new_rx(&self) -> Self::RxBuffer {
		let mut buf = vec![0u8; self.capacity];
		buf.clear();
		buf
	}
	#[inline]
	fn new_tx(&self) -> Self::TxBuffer {
		Vec::new()
	}
	#[inline]
	fn new_msg(&self) -> Self::MsgBuffer {
		Vec::new()
	}

	#[inline]
	fn recv_rx<'a>(&self, this: &'a mut Self::RxBuffer) -> &'a mut [u8] {
		let len = this.len();
		let capacity = this.capacity();
		unsafe {
			slice::from_raw_parts_mut(this.as_mut_ptr().add(len), capacity - len)
		}
	}
	#[inline]
	fn recvn_rx(&self, this: &mut Self::RxBuffer, n: usize) {
		unsafe {
			this.set_len(this.len() + n);
		}
	}
	#[inline]
	fn consume_rx(&self, this: &mut Self::RxBuffer, n: usize) {
		if n >= this.len() {
			this.clear();
		}
		else {
			let _ = this.drain(..n);
		}
	}
	#[inline]
	fn get_rx<'a>(&self, this: &'a mut Self::RxBuffer, offset: usize) -> &'a mut [u8] {
		this.get_mut(offset..).unwrap_or(&mut [])
	}
	#[inline]
	fn take_rx(&self, this: &mut Self::RxBuffer) -> Self::RxBuffer {
		mem::replace(this, Vec::new())
	}
	#[inline]
	fn store_rx(&self, this: &mut Self::RxBuffer, buffer: Self::RxBuffer) {
		let _ = mem::replace(this, buffer);
	}

	#[inline]
	fn len_tx(&self, this: &Self::TxBuffer) -> usize {
		this.len()
	}
	#[inline]
	fn append_tx(&self, this: &mut Self::TxBuffer, data: &[u8]) {
		this.extend_from_slice(data);
	}
	#[inline]
	fn consume_tx(&self, this: &mut Self::TxBuffer, n: usize) {
		if n >= this.len() {
			this.clear();
		}
		else {
			let _ = this.drain(..n);
		}
	}
	#[inline]
	fn bytes_tx<'a>(&self, this: &'a Self::TxBuffer) -> &'a [u8] {
		this.as_slice()
	}
	#[inline]
	fn write_fmt_tx(&self, this: &mut Self::TxBuffer, fmt: fmt::Arguments<'_>) {
		use std::io::Write;
		let _ = this.write_fmt(fmt);
	}

	#[inline]
	fn append_msg(&self, this: &mut Self::MsgBuffer, payload: &[u8]) {
		this.extend_from_slice(payload);
	}
	#[inline]
	fn get_msg<'a>(&self, this: &'a Self::MsgBuffer) -> &'a [u8] {
		this.as_slice()
	}
	#[inline]
	fn clear_msg(&self, this: &mut Self::MsgBuffer) {
		// Only used for fragmented messages, which are probably large
		// Free the whole buffer to save memory
		*this = Vec::new();
	}
}
