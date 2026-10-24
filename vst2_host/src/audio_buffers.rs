use dsp::DelayLine;

/// Planar buffers for the plugin's input and output.
///
/// Everything is allocated once up front, so nothing here allocates while processing.
/// All heap allocations are boxed slices that are never resized, which keeps the
/// pointers stable.
#[derive(Debug)]
pub struct AudioBuffers {
	max_frames: usize,
	inputs: usize,
	outputs: usize,
	input: Box<[f32]>,
	output: Box<[f32]>,
	input_channels: Box<[*mut f32]>,
	output_channels: Box<[*mut f32]>,
	/// Delays the dry signal by the plugin's latency, so it lines up with the wet
	/// signal.
	delay_line: DelayLine,
}

fn channel_ptrs(buffer: &mut [f32], channels: usize, max_frames: usize) -> Box<[*mut f32]> {
	(0..channels)
		.map(|channel| buffer[channel * max_frames..][..max_frames].as_mut_ptr())
		.collect()
}

impl AudioBuffers {
	/// Makes buffers for a plugin with the given number of channels.
	#[must_use]
	pub fn new(inputs: usize, outputs: usize, max_frames: usize, latency: usize) -> Self {
		// a plugin that has no channels still gets a pointer to read, in case it looks
		// at one anyway
		let inputs = inputs.max(1);
		let outputs = outputs.max(1);

		let mut input = vec![0.0; inputs * max_frames].into_boxed_slice();
		let mut output = vec![0.0; outputs * max_frames].into_boxed_slice();

		let input_channels = channel_ptrs(&mut input, inputs, max_frames);
		let output_channels = channel_ptrs(&mut output, outputs, max_frames);

		Self {
			max_frames,
			inputs,
			outputs,
			input,
			output,
			input_channels,
			output_channels,
			delay_line: DelayLine::new(latency),
		}
	}

	/// Copies `buf` into the plugin's input channels. Stereo is copied as is, and mono
	/// is the average of both channels. The channels the host has no audio for stay
	/// silent.
	pub fn read_in(&mut self, buf: &[[f32; 2]]) {
		match self.inputs {
			0 => {}
			1 => {
				for (sample, frame) in self.input.iter_mut().zip(buf) {
					*sample = (frame[0] + frame[1]) * 0.5;
				}
			}
			_ => {
				let (left, right) = self.input.split_at_mut(self.max_frames);
				for ((left, right), frame) in left.iter_mut().zip(right).zip(buf) {
					*left = frame[0];
					*right = frame[1];
				}
			}
		}
	}

	/// Silences the output channels, and returns the pointers to them.
	pub fn prepare(&mut self, frames: usize) -> (*const *const f32, *mut *mut f32) {
		debug_assert!(frames <= self.max_frames);

		for channel in self.output.chunks_mut(self.max_frames) {
			channel[..frames].fill(0.0);
		}

		(
			self.input_channels.as_ptr().cast::<*const f32>(),
			self.output_channels.as_mut_ptr(),
		)
	}

	/// Mixes the plugin's output into `buf`, which holds the dry signal.
	///
	/// A `mix_level` of `0.0` is only the dry signal and `1.0` is only the plugin's
	/// output. The channels the host has no audio for are left alone.
	pub fn write_out(&mut self, buf: &mut [[f32; 2]], mix_level: f32) {
		self.delay_line.advance_mut(buf);

		if self.outputs == 0 {
			return;
		}

		let buf_mix = 1f32.copysign(mix_level) - mix_level;
		let sample_mix = mix_level.abs();

		if self.outputs == 1 {
			for frame in &mut *buf {
				frame[0] *= buf_mix;
				frame[1] *= buf_mix;
			}
			for (sample, frame) in self.output.iter().zip(&mut *buf) {
				frame[0] += sample * sample_mix;
				frame[1] += sample * sample_mix;
			}
		} else {
			let (left, right) = self.output.split_at(self.max_frames);

			for i in 0..buf.len() {
				buf[i][0] = buf[i][0] * buf_mix + left[i] * sample_mix;
				buf[i][1] = buf[i][1] * buf_mix + right[i] * sample_mix;
			}
		}
	}

	/// For blocks the plugin didn't process: only keeps the dry signal going.
	pub fn flush(&mut self, buf: &mut [[f32; 2]], mix_level: f32) {
		self.delay_line.advance_mut(buf);

		if self.outputs != 0 {
			let gain = 1.0 - mix_level;
			for frame in buf {
				frame[0] *= gain;
				frame[1] *= gain;
			}
		}
	}

	/// The number of samples the plugin delays its output by.
	#[must_use]
	pub fn latency(&self) -> usize {
		self.delay_line.len()
	}

	/// Clears the plugin's internal state, like delay lines and filters.
	pub fn reset(&mut self) {
		self.delay_line.reset();
	}
}
