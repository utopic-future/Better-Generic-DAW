use crate::AudioLayout;
use dsp::DelayLine;
use std::ptr::null_mut;
use vst3::Steinberg::Vst::{AudioBusBuffers, AudioBusBuffers__type0};

const EMPTY_BUS: AudioBusBuffers = AudioBusBuffers {
	numChannels: 0,
	silenceFlags: 0,
	__field0: AudioBusBuffers__type0 {
		channelBuffers32: null_mut(),
	},
};

/// Planar buffers for the main buses, and the bus descriptions that point into them.
///
/// Everything is allocated once up front, so nothing here allocates while processing. All heap
/// allocations are boxed slices that are never resized, which keeps the pointers stable.
pub struct AudioBuffers {
	layout: AudioLayout,
	max_frames: usize,
	input: Box<[f32]>,
	output: Box<[f32]>,
	_input_channels: Box<[*mut f32]>,
	_output_channels: Box<[*mut f32]>,
	input_buses: Box<[AudioBusBuffers]>,
	output_buses: Box<[AudioBusBuffers]>,
	/// Delays the dry signal by the plugin's latency, so it lines up with the wet signal.
	delay_line: DelayLine,
}

fn channel_ptrs(buffer: &mut [f32], channels: usize, max_frames: usize) -> Box<[*mut f32]> {
	(0..channels)
		.map(|channel| buffer[channel * max_frames..][..max_frames].as_mut_ptr())
		.collect()
}

fn buses(
	count: usize,
	main: Option<(usize, usize)>,
	channels: &mut Box<[*mut f32]>,
) -> Box<[AudioBusBuffers]> {
	let mut buses = vec![EMPTY_BUS; count].into_boxed_slice();

	if let Some((index, count)) = main {
		buses[index] = AudioBusBuffers {
			numChannels: count as i32,
			silenceFlags: 0,
			__field0: AudioBusBuffers__type0 {
				channelBuffers32: channels.as_mut_ptr(),
			},
		};
	}

	buses
}

impl AudioBuffers {
	pub fn new(layout: AudioLayout, max_frames: usize, latency: usize) -> Self {
		let mut input = vec![0.0; layout.input_channels() * max_frames].into_boxed_slice();
		let mut output = vec![0.0; layout.output_channels() * max_frames].into_boxed_slice();

		let mut input_channels = channel_ptrs(&mut input, layout.input_channels(), max_frames);
		let mut output_channels = channel_ptrs(&mut output, layout.output_channels(), max_frames);

		Self {
			layout,
			max_frames,
			input_buses: buses(layout.input_buses, layout.main_input, &mut input_channels),
			output_buses: buses(
				layout.output_buses,
				layout.main_output,
				&mut output_channels,
			),
			input,
			output,
			_input_channels: input_channels,
			_output_channels: output_channels,
			delay_line: DelayLine::new(latency),
		}
	}

	/// Copies `buf` into the plugin's input channels. Stereo is copied as is, and mono is the
	/// average of both channels.
	pub fn read_in(&mut self, buf: &[[f32; 2]]) {
		match self.layout.input_channels() {
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

	/// Silences the outputs and returns the bus descriptions to put in the process data, as
	/// `(inputs, outputs)`, null for buses that don't exist.
	pub fn prepare(&mut self, frames: usize) -> (*mut AudioBusBuffers, *mut AudioBusBuffers) {
		debug_assert!(frames <= self.max_frames);

		for channel in self.output.chunks_mut(self.max_frames) {
			channel[..frames].fill(0.0);
		}

		for bus in self.input_buses.iter_mut().chain(&mut self.output_buses) {
			bus.silenceFlags = 0;
		}

		(
			if self.input_buses.is_empty() {
				null_mut()
			} else {
				self.input_buses.as_mut_ptr()
			},
			if self.output_buses.is_empty() {
				null_mut()
			} else {
				self.output_buses.as_mut_ptr()
			},
		)
	}

	/// Mixes the plugin's output into `buf`, which holds the dry signal.
	///
	/// A `mix_level` of `0.0` is only the dry signal and `1.0` is only the plugin's output.
	pub fn write_out(&mut self, buf: &mut [[f32; 2]], mix_level: f32) {
		self.delay_line.advance_mut(buf);

		let Some((index, channels)) = self.layout.main_output else {
			return;
		};

		// honor the plugin's claim that some channels are silent
		if let Some(bus) = self.output_buses.get(index) {
			for channel in 0..channels {
				if bus.silenceFlags & (1 << channel) != 0 {
					self.output[channel * self.max_frames..][..buf.len()].fill(0.0);
				}
			}
		}

		let buf_mix = 1f32.copysign(mix_level) - mix_level;
		let sample_mix = mix_level.abs();

		if channels == 1 {
			for frame in &mut *buf {
				frame[0] = frame[0] * buf_mix;
				frame[1] = frame[1] * buf_mix;
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

		if self.layout.output_channels() != 0 {
			let gain = 1.0 - mix_level;
			for frame in buf {
				frame[0] *= gain;
				frame[1] *= gain;
			}
		}
	}

	pub fn latency(&self) -> usize {
		self.delay_line.len()
	}

	pub fn set_latency(&mut self, latency: usize) {
		self.delay_line.resize(latency);
	}

	pub fn reset(&mut self) {
		self.delay_line.reset();
	}
}
