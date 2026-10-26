use utils::NoDebug;

/// Delays its input by exactly `len` frames, for any block size.
#[derive(Clone, Debug, Default)]
pub struct DelayLine {
	buf: NoDebug<Vec<[f32; 2]>>,
	head: usize,
}

impl DelayLine {
	#[must_use]
	pub fn new(len: usize) -> Self {
		let mut delay_line = Self::default();
		delay_line.resize(len);
		delay_line
	}

	pub fn resize(&mut self, len: usize) {
		let old = self.buf.len();
		if len == old {
			return;
		}

		// oldest sample first
		self.buf.rotate_left(self.head);
		self.head = 0;

		if len > old {
			self.buf
				.splice(0..0, std::iter::repeat_n([0.0; 2], len - old));
		} else {
			self.buf.drain(..old - len);
		}
	}

	#[must_use]
	pub fn len(&self) -> usize {
		self.buf.len()
	}

	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.buf.is_empty()
	}

	pub fn advance(&mut self, buf: &[[f32; 2]]) {
		let len = self.buf.len();
		if len == 0 {
			return;
		}
		for frame in buf {
			self.buf[self.head] = *frame;
			self.head += 1;
			if self.head == len {
				self.head = 0;
			}
		}
	}

	pub fn advance_mut(&mut self, buf: &mut [[f32; 2]]) {
		let len = self.buf.len();
		if len == 0 {
			return;
		}
		for frame in buf {
			std::mem::swap(&mut self.buf[self.head], frame);
			self.head += 1;
			if self.head == len {
				self.head = 0;
			}
		}
	}

	pub fn reset(&mut self) {
		self.head = 0;
		self.buf.fill([0.0; 2]);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn delays_by_exactly_len_for_any_block_size() {
		for latency in [1, 3, 64, 100] {
			for block in [1, 7, 64, 256] {
				let mut delay = DelayLine::new(latency);
				let mut out = Vec::new();
				for start in (0..1024).step_by(block) {
					let mut buf: Vec<[f32; 2]> = (start..start + block)
						.map(|i| [i as f32 + 1.0; 2])
						.collect();
					delay.advance_mut(&mut buf);
					out.extend(buf);
				}
				for (i, frame) in out.iter().enumerate() {
					let expected = if i < latency {
						0.0
					} else {
						(i - latency) as f32 + 1.0
					};
					assert_eq!(
						frame[0], expected,
						"latency {latency}, block {block}, frame {i}"
					);
				}
			}
		}
	}
}
