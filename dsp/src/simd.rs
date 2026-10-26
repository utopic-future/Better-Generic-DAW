//! SIMD-optimized DSP functions

#[cfg(all(target_arch = "x86_64", feature = "simd"))]
mod simd_impl {
	use std::arch::x86_64::*;

	/// Apply volume and pan (Stereo mode) using SSE2
	/// Processes two interleaved stereo frames per SIMD iteration.
	#[target_feature(enable = "sse2")]
	unsafe fn utility_process_stereo_sse2_impl(audio: &mut [[f32; 2]], l_gain: f32, r_gain: f32) {
		let len = audio.len();
		let gains = _mm_setr_ps(l_gain, r_gain, l_gain, r_gain);
		let samples = audio.as_mut_ptr().cast::<f32>();
		let mut i = 0;

		while i + 2 <= len {
			let offset = i * 2;
			let input_ptr = unsafe { samples.add(offset) };
			let values = unsafe { _mm_loadu_ps(input_ptr) };
			let output = _mm_mul_ps(values, gains);
			let output_ptr = unsafe { samples.add(offset) };
			unsafe { _mm_storeu_ps(output_ptr, output) };
			i += 2;
		}

		while i < len {
			audio[i][0] *= l_gain;
			audio[i][1] *= r_gain;
			i += 1;
		}
	}

	/// Apply volume and pan (SplitStereo mode) using SSE2
	#[target_feature(enable = "sse2")]
	unsafe fn utility_process_split_stereo_sse2_impl(
		audio: &mut [[f32; 2]],
		ll: f32,
		lr: f32,
		rl: f32,
		rr: f32,
	) {
		let len = audio.len();
		let same_gain = _mm_setr_ps(ll, rr, ll, rr);
		let cross_gain = _mm_setr_ps(rl, lr, rl, lr);
		let samples = audio.as_mut_ptr().cast::<f32>();
		let mut i = 0;

		while i + 2 <= len {
			let offset = i * 2;
			let input_ptr = unsafe { samples.add(offset) };
			let values = unsafe { _mm_loadu_ps(input_ptr) };
			let swapped = _mm_shuffle_ps(values, values, 0xB1);
			let same = _mm_mul_ps(values, same_gain);
			let cross = _mm_mul_ps(swapped, cross_gain);
			let output = _mm_add_ps(same, cross);
			let output_ptr = unsafe { samples.add(offset) };
			unsafe { _mm_storeu_ps(output_ptr, output) };
			i += 2;
		}

		while i < len {
			let ols = audio[i][0];
			audio[i][0] = ols * ll + audio[i][1] * rl;
			audio[i][1] = audio[i][1] * rr + ols * lr;
			i += 1;
		}
	}

	// Safe public wrappers
	pub fn utility_process_stereo_sse2(audio: &mut [[f32; 2]], l_gain: f32, r_gain: f32) {
		unsafe { utility_process_stereo_sse2_impl(audio, l_gain, r_gain) }
	}

	pub fn utility_process_split_stereo_sse2(
		audio: &mut [[f32; 2]],
		ll: f32,
		lr: f32,
		rl: f32,
		rr: f32,
	) {
		unsafe { utility_process_split_stereo_sse2_impl(audio, ll, lr, rl, rr) }
	}
}

#[cfg(not(all(target_arch = "x86_64", feature = "simd")))]
mod simd_impl {
	pub fn utility_process_stereo_sse2(audio: &mut [[f32; 2]], l_gain: f32, r_gain: f32) {
		for [ls, rs] in audio {
			*ls *= l_gain;
			*rs *= r_gain;
		}
	}

	pub fn utility_process_split_stereo_sse2(
		audio: &mut [[f32; 2]],
		ll: f32,
		lr: f32,
		rl: f32,
		rr: f32,
	) {
		for [ls, rs] in audio {
			let ols = *ls;
			*ls = ols * ll + *rs * rl;
			*rs = *rs * rr + ols * lr;
		}
	}
}

pub use simd_impl::*;
#[cfg(test)]
mod tests {
	use super::{utility_process_split_stereo_sse2, utility_process_stereo_sse2};

	#[test]
	fn stereo_gain_preserves_interleaved_channels() {
		let mut audio = [[1.0, 10.0], [2.0, 20.0], [3.0, 30.0]];
		utility_process_stereo_sse2(&mut audio, 2.0, 0.5);
		assert_eq!(audio, [[2.0, 5.0], [4.0, 10.0], [6.0, 15.0]]);
	}

	#[test]
	fn split_stereo_uses_samples_from_the_same_frame() {
		let mut audio = [[1.0, 10.0], [2.0, 20.0], [3.0, 30.0]];
		utility_process_split_stereo_sse2(&mut audio, 2.0, 3.0, 4.0, 5.0);
		assert_eq!(audio, [[42.0, 53.0], [84.0, 106.0], [126.0, 159.0]]);
	}
}
