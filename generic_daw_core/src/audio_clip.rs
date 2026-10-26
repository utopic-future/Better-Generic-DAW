use crate::{ClipId, SampleId, Transition, audio_thread::State, time::OffsetBeatSpan};
use dsp::resample_cubic;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioClip {
	pub id: ClipId,
	pub sample: SampleId,
	pub position: OffsetBeatSpan,
	pub volume: f32,
	pub fade_start: Transition,
	pub fade_end: Transition,
	pub stretch: f64,
}

impl AudioClip {
	#[must_use]
	pub fn new(sample: SampleId) -> Self {
		Self {
			id: ClipId::unique(),
			sample,
			position: OffsetBeatSpan::default(),
			volume: 1.0,
			fade_start: Transition::default(),
			fade_end: Transition::default(),
			stretch: 1.0,
		}
	}

	pub fn process(&self, state: &State, audio: &mut [[f32; 2]]) {
		debug_assert!(state.transport.playing);

		let position = state.transport.position.to_frames(&state.transport);

		let (start, end) = self.position.beat_span().to_frames(&state.transport);
		if !(start < position + audio.len() && end >= position) {
			return;
		}

		let sample = &state.samples[&self.sample];

		let play_pos = position.saturating_sub(start);
		let write_start = start.saturating_sub(position);
		let len = end - start;

		let resample_ratio = sample.resample_ratio(&state.transport);
		let offset = (self.position.offset() / resample_ratio).to_frames(&state.transport);

		let resample_ratio = self.stretch / resample_ratio;
		let read_start = if resample_ratio.is_sign_positive() {
			offset
		} else {
			0
		};

		let fade_start = self.fade_start.len.to_frames(&state.transport);
		let fade_end = self.fade_end.len.to_frames(&state.transport);

		let volume = self.volume;
		let samples = &sample.samples[read_start..];
		let _sample_len = samples.len();

		// Stream directly from resampler into output - no intermediate Vec allocation
		let mut resample_iter = resample_cubic(samples, resample_ratio, play_pos);

		for (i, frame) in audio[write_start..]
			.iter_mut()
			.enumerate()
			.take(len - play_pos)
		{
			let Some([l, r]) = resample_iter.next() else {
				break;
			};

			// position in the clip, not in the block
			let pos = play_pos + i;

			let mix = if pos < fade_start {
				self.fade_start.transition(pos as f32 / fade_start as f32)
			} else if pos >= len.saturating_sub(fade_end) {
				self.fade_end
					.transition((len - pos) as f32 / fade_end as f32)
			} else {
				1.0
			};

			frame[0] += l * volume * mix;
			frame[1] += r * volume * mix;
		}
	}
}
