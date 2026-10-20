use std::mem::zeroed;
use vst3::Steinberg::Vst::{
	ProcessContext,
	ProcessContext_::{StatesAndFlags, StatesAndFlags_},
};

/// `ProcessContext::state` is always a `u32`, but the flag constants are `c_int` on Windows.
#[cfg_attr(
	not(windows),
	expect(clippy::unnecessary_cast, reason = "the constants are `u32` here")
)]
const fn flag(flag: StatesAndFlags) -> u32 {
	flag as u32
}

/// What the host tells a plugin about the timeline, at the start of a block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transport {
	pub playing: bool,
	pub recording: bool,
	/// Beats per minute.
	pub tempo: f64,
	pub time_sig_numerator: i32,
	pub time_sig_denominator: i32,
	/// The position of the first frame of the block, in samples from the start of the project.
	pub position_samples: i64,
	/// The position of the first frame of the block, in quarter notes.
	pub position_quarters: f64,
	/// The active loop, as start and end in quarter notes.
	pub cycle: Option<(f64, f64)>,
}

impl Transport {
	/// The transport `samples` frames later, assuming the tempo stays the same.
	#[must_use]
	pub fn advanced(&self, samples: usize, sample_rate: f64) -> Self {
		if !self.playing || samples == 0 {
			return *self;
		}

		Self {
			position_samples: self.position_samples + samples as i64,
			position_quarters: self.position_quarters
				+ samples as f64 / sample_rate * self.tempo / 60.0,
			..*self
		}
	}

	pub(crate) fn to_process_context(
		transport: Option<&Self>,
		sample_rate: f64,
		continuous_samples: i64,
	) -> ProcessContext {
		// SAFETY:
		// `ProcessContext` is plain old data, all zeroes is a valid value.
		let mut context = unsafe { zeroed::<ProcessContext>() };

		context.sampleRate = sample_rate;
		context.continousTimeSamples = continuous_samples;
		context.state = flag(StatesAndFlags_::kContTimeValid);

		let Some(transport) = transport else {
			return context;
		};

		context.state |= flag(StatesAndFlags_::kTempoValid)
			| flag(StatesAndFlags_::kTimeSigValid)
			| flag(StatesAndFlags_::kProjectTimeMusicValid)
			| flag(StatesAndFlags_::kBarPositionValid);

		if transport.playing {
			context.state |= flag(StatesAndFlags_::kPlaying);
		}

		if transport.recording {
			context.state |= flag(StatesAndFlags_::kRecording);
		}

		context.tempo = transport.tempo;
		context.timeSigNumerator = transport.time_sig_numerator;
		context.timeSigDenominator = transport.time_sig_denominator;
		context.projectTimeSamples = transport.position_samples;
		context.projectTimeMusic = transport.position_quarters;

		// a bar is `numerator` beats, and a beat is `1 / denominator` of a whole note
		let bar_length = f64::from(transport.time_sig_numerator) * 4.0
			/ f64::from(transport.time_sig_denominator.max(1));
		context.barPositionMusic = (transport.position_quarters / bar_length).floor() * bar_length;

		if let Some((start, end)) = transport.cycle {
			context.state |=
				flag(StatesAndFlags_::kCycleValid) | flag(StatesAndFlags_::kCycleActive);
			context.cycleStartMusic = start;
			context.cycleEndMusic = end;
		}

		context
	}
}
