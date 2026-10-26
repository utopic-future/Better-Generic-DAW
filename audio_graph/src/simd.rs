//! SIMD-optimized audio processing functions
//!
//! These functions use explicit SIMD intrinsics for better performance
//! on x86_64 targets with SSE2/AVX2 support.

#[cfg(all(target_arch = "x86_64", feature = "simd"))]
mod simd_impl {
	use std::arch::x86_64::*;

	/// Check if AVX2 is available at runtime
	#[inline]
	pub fn has_avx2() -> bool {
		std::is_x86_feature_detected!("avx2")
	}

	/// Check if SSE2 is available at runtime (always true on x86_64)
	#[inline]
	pub fn has_sse2() -> bool {
		true
	}

	/// Mix two stereo buffers with a gain factor using SSE2
	/// dst += src * gain
	/// Processes 4 frames (8 samples) at a time
	#[target_feature(enable = "sse2")]
	unsafe fn mix_stereo_sse2_impl(dst: &mut [[f32; 2]], src: &[[f32; 2]], gain: f32) {
		let len = dst.len();
		let gain_vec = _mm_set1_ps(gain);
		let mut i = 0;

		while i + 4 <= len {
			let src_l = _mm_loadu_ps(&src[i][0] as *const f32);
			let src_r = _mm_loadu_ps(&src[i][1] as *const f32);

			let dst_l = _mm_loadu_ps(&dst[i][0] as *const f32);
			let dst_r = _mm_loadu_ps(&dst[i][1] as *const f32);

			let src_l_gained = _mm_mul_ps(src_l, gain_vec);
			let src_r_gained = _mm_mul_ps(src_r, gain_vec);

			let new_l = _mm_add_ps(dst_l, src_l_gained);
			let new_r = _mm_add_ps(dst_r, src_r_gained);

			_mm_storeu_ps(&mut dst[i][0] as *mut f32, new_l);
			_mm_storeu_ps(&mut dst[i][1] as *mut f32, new_r);

			i += 4;
		}

		while i < len {
			dst[i][0] += src[i][0] * gain;
			dst[i][1] += src[i][1] * gain;
			i += 1;
		}
	}

	/// Copy src * gain to dst (first input)
	/// dst = src * gain
	#[target_feature(enable = "sse2")]
	unsafe fn copy_stereo_sse2_impl(dst: &mut [[f32; 2]], src: &[[f32; 2]], gain: f32) {
		let len = dst.len();
		let gain_vec = _mm_set1_ps(gain);
		let mut i = 0;

		while i + 4 <= len {
			let src_l = _mm_loadu_ps(&src[i][0] as *const f32);
			let src_r = _mm_loadu_ps(&src[i][1] as *const f32);

			let dst_l = _mm_mul_ps(src_l, gain_vec);
			let dst_r = _mm_mul_ps(src_r, gain_vec);

			_mm_storeu_ps(&mut dst[i][0] as *mut f32, dst_l);
			_mm_storeu_ps(&mut dst[i][1] as *mut f32, dst_r);

			i += 4;
		}

		while i < len {
			dst[i][0] = src[i][0] * gain;
			dst[i][1] = src[i][1] * gain;
			i += 1;
		}
	}

	/// Mix with delay line - processes scratch buffer
	#[target_feature(enable = "sse2")]
	unsafe fn mix_scratch_sse2_impl(dst: &mut [[f32; 2]], scratch: &[[f32; 2]], gain: f32) {
		mix_stereo_sse2_impl(dst, scratch, gain)
	}

	/// Copy from src to scratch buffer
	#[target_feature(enable = "sse2")]
	unsafe fn copy_to_scratch_sse2_impl(scratch: &mut [[f32; 2]], src: &[[f32; 2]]) {
		let len = scratch.len();
		let mut i = 0;

		while i + 4 <= len {
			let src_l = _mm_loadu_ps(&src[i][0] as *const f32);
			let src_r = _mm_loadu_ps(&src[i][1] as *const f32);

			_mm_storeu_ps(&mut scratch[i][0] as *mut f32, src_l);
			_mm_storeu_ps(&mut scratch[i][1] as *mut f32, src_r);

			i += 4;
		}

		while i < len {
			scratch[i][0] = src[i][0];
			scratch[i][1] = src[i][1];
			i += 1;
		}
	}

	/// Peak meter - find max absolute value in stereo buffer
	#[target_feature(enable = "sse2")]
	unsafe fn peak_stereo_sse2_impl(audio: &[[f32; 2]]) -> [f32; 2] {
		let len = audio.len();
		let mut max_l = _mm_setzero_ps();
		let mut max_r = _mm_setzero_ps();
		let mut i = 0;

		while i + 4 <= len {
			let l = _mm_loadu_ps(&audio[i][0] as *const f32);
			let r = _mm_loadu_ps(&audio[i][1] as *const f32);

			// Absolute value using bitwise AND with sign bit mask
			let abs_l = _mm_and_ps(l, _mm_castsi128_ps(_mm_set1_epi32(0x7fffffff)));
			let abs_r = _mm_and_ps(r, _mm_castsi128_ps(_mm_set1_epi32(0x7fffffff)));

			max_l = _mm_max_ps(max_l, abs_l);
			max_r = _mm_max_ps(max_r, abs_r);

			i += 4;
		}

		// Horizontal max reduction
		let max_l = horizontal_max(max_l);
		let max_r = horizontal_max(max_r);

		// Handle remainder
		let mut max_l_final = max_l;
		let mut max_r_final = max_r;
		while i < len {
			let l = audio[i][0].abs();
			let r = audio[i][1].abs();
			if l > max_l_final {
				max_l_final = l;
			}
			if r > max_r_final {
				max_r_final = r;
			}
			i += 1;
		}

		[max_l_final, max_r_final]
	}

	#[inline]
	unsafe fn horizontal_max(v: __m128) -> f32 {
		// Shuffle and reduce - use stable shuffle intrinsic
		// _MM_SHUFFLE(2, 3, 0, 1) = 0b10_11_00_01 = 0xB1
		let shuf = _mm_shuffle_ps(v, v, 0xB1);
		let max1 = _mm_max_ps(v, shuf);
		// _MM_SHUFFLE(1, 0, 3, 2) = 0b01_00_11_10 = 0x4E
		let shuf2 = _mm_shuffle_ps(max1, max1, 0x4E);
		let max2 = _mm_max_ps(max1, shuf2);
		_mm_cvtss_f32(max2)
	}

	// Safe public wrappers
	pub fn mix_stereo_sse2(dst: &mut [[f32; 2]], src: &[[f32; 2]], gain: f32) {
		unsafe { mix_stereo_sse2_impl(dst, src, gain) }
	}

	pub fn copy_stereo_sse2(dst: &mut [[f32; 2]], src: &[[f32; 2]], gain: f32) {
		unsafe { copy_stereo_sse2_impl(dst, src, gain) }
	}

	pub fn mix_scratch_sse2(dst: &mut [[f32; 2]], scratch: &[[f32; 2]], gain: f32) {
		unsafe { mix_scratch_sse2_impl(dst, scratch, gain) }
	}

	pub fn copy_to_scratch_sse2(scratch: &mut [[f32; 2]], src: &[[f32; 2]]) {
		unsafe { copy_to_scratch_sse2_impl(scratch, src) }
	}

	pub fn peak_stereo_sse2(audio: &[[f32; 2]]) -> [f32; 2] {
		unsafe { peak_stereo_sse2_impl(audio) }
	}
}

// Scalar fallback implementations
#[cfg(not(all(target_arch = "x86_64", feature = "simd")))]
mod simd_impl {
	pub fn has_avx2() -> bool {
		false
	}
	pub fn has_sse2() -> bool {
		false
	}

	pub fn mix_stereo_sse2(dst: &mut [[f32; 2]], src: &[[f32; 2]], gain: f32) {
		for i in 0..dst.len() {
			dst[i][0] += src[i][0] * gain;
			dst[i][1] += src[i][1] * gain;
		}
	}

	pub fn copy_stereo_sse2(dst: &mut [[f32; 2]], src: &[[f32; 2]], gain: f32) {
		for i in 0..dst.len() {
			dst[i][0] = src[i][0] * gain;
			dst[i][1] = src[i][1] * gain;
		}
	}

	pub fn mix_scratch_sse2(dst: &mut [[f32; 2]], scratch: &[[f32; 2]], gain: f32) {
		mix_stereo_sse2(dst, scratch, gain)
	}

	pub fn copy_to_scratch_sse2(scratch: &mut [[f32; 2]], src: &[[f32; 2]]) {
		for i in 0..scratch.len() {
			scratch[i][0] = src[i][0];
			scratch[i][1] = src[i][1];
		}
	}

	pub fn peak_stereo_sse2(audio: &[[f32; 2]]) -> [f32; 2] {
		let mut max_l = 0.0f32;
		let mut max_r = 0.0f32;
		for &[l, r] in audio {
			let l_abs = l.abs();
			let r_abs = r.abs();
			if l_abs > max_l {
				max_l = l_abs;
			}
			if r_abs > max_r {
				max_r = r_abs;
			}
		}
		[max_l, max_r]
	}
}

pub use simd_impl::*;
