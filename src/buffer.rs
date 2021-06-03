use core::{cmp, fmt, mem, slice};

pub trait Buffer {
	fn len(&self) -> usize;
	fn getx(&self) -> &[u8];
	fn getx_mut(&mut self) -> &mut [u8];
	fn spare_len(&self) -> usize;
	fn spare_ptr(&mut self) -> *mut u8;
	fn spare(&mut self) -> &mut [u8];
	fn add_len(&mut self, n: usize);
	fn consume(&mut self, n: usize);
	fn clear(&mut self);
	fn write_fmt(&mut self, fmt: fmt::Arguments<'_>);
	fn take(&mut self) -> Self;
	fn store(&mut self, buffer: Self);
	fn append(&mut self, data: &[u8]);
}

/// Implementation details of the WebSocket buffers.
pub trait BufferStrategy {
	/// Receive buffer.
	///
	/// This buffer holds onto incoming bytes which may contain partial inputs.
	type RxBuffer: Buffer;

	/// Transmit buffer.
	///
	/// This buffer holds the outgoing bytes before they're sent back.
	type TxBuffer: Buffer;

	/// Message buffer
	///
	/// Fragmented frames accumulate their payload here.
	type MsgBuffer: Buffer;

	fn new_rx(&self) -> Self::RxBuffer;
	fn new_tx(&self) -> Self::TxBuffer;
	fn new_msg(&self) -> Self::MsgBuffer;
}

//----------------------------------------------------------------

#[cfg(feature = "std")]
impl Buffer for Vec<u8> {
	#[inline]
	fn len(&self) -> usize {
		self.len()
	}
	#[inline]
	fn getx(&self) -> &[u8] {
		self.as_slice()
	}
	#[inline]
	fn getx_mut(&mut self) -> &mut [u8] {
		self.as_mut_slice()
	}
	#[inline]
	fn spare_len(&self) -> usize {
		self.capacity() - self.len()
	}
	#[inline]
	fn spare_ptr(&mut self) -> *mut u8 {
		unsafe { self.as_mut_ptr().add(self.len()) }
	}
	#[inline]
	fn spare(&mut self) -> &mut [u8] {
		unsafe {
			let len = self.spare_len();
			let ptr = self.spare_ptr();
			slice::from_raw_parts_mut(ptr, len)
		}
	}
	#[inline]
	fn add_len(&mut self, n: usize) {
		let new_len = cmp::min(self.capacity(), self.len() + n);
		unsafe { self.set_len(new_len); }
	}
	#[inline]
	fn consume(&mut self, n: usize) {
		if n == self.len() {
			self.clear();
		}
		else {
			self.drain(..n);
		}
	}
	#[inline]
	fn clear(&mut self) {
		self.clear();
	}
	#[inline]
	fn write_fmt(&mut self, fmt: fmt::Arguments<'_>) {
		let _ = std::io::Write::write_fmt(self, fmt);
	}
	#[inline]
	fn take(&mut self) -> Vec<u8> {
		mem::replace(self, Vec::new())
	}
	#[inline]
	fn store(&mut self, buffer: Vec<u8>) {
		let _ = mem::replace(self, buffer);
	}
	#[inline]
	fn append(&mut self, data: &[u8]) {
		self.extend_from_slice(data);
	}
}

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
}
