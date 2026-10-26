use crate::api::{self, time_flags};

/// What the host tells a plugin about the timeline, at the start of a block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transport {
	pub playing: bool,
	pub recording: bool,
	/// Beats per minute.
	pub tempo: f64,
	pub time_sig_numerator: i32,
	pub time_sig_denominator: i32,
	/// The position of the first frame of the block, in samples from the start of the
	/// project.
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

	/// The time information as the plugin wants it.
	#[must_use]
	pub fn to_time_info(self, sample_rate: f64, continuous_samples: i64) -> api::TimeInfo {
		let mut flags = time_flags::NANOSECONDS_VALID
			| time_flags::TEMPO_VALID
			| time_flags::TIME_SIG_VALID
			| time_flags::PPQ_POS_VALID
			| time_flags::BARS_VALID
			| time_flags::AUTOMATION_READING;

		if self.playing {
			flags |= time_flags::TRANSPORT_PLAYING | time_flags::TRANSPORT_CHANGED;
		}

		if self.recording {
			flags |= time_flags::TRANSPORT_RECORDING;
		}

		let mut info = api::TimeInfo {
			sample_pos: self.position_samples as f64,
			sample_rate,
			nano_seconds: continuous_samples as f64 / sample_rate * 1_000_000_000.0,
			ppq_pos: self.position_quarters,
			tempo: self.tempo,
			time_sig_numerator: self.time_sig_numerator,
			time_sig_denominator: self.time_sig_denominator,
			flags,
			..api::TimeInfo::default()
		};

		// a bar is `numerator` beats, and a beat is `1 / denominator` of a whole note
		let bar_length =
			f64::from(self.time_sig_numerator) * 4.0 / f64::from(self.time_sig_denominator.max(1));
		info.bar_start_pos = (self.position_quarters / bar_length).floor() * bar_length;

		if let Some((start, end)) = self.cycle {
			info.cycle_start_pos = start;
			info.cycle_end_pos = end;
			flags |= time_flags::CYCLE_POS_VALID | time_flags::TRANSPORT_CYCLE_ACTIVE;
			info.flags = flags;
		}

		info
	}
}
