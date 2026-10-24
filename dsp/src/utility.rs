use std::f32::consts::{FRAC_PI_4, SQRT_2};

#[cfg(feature = "simd")]
use crate::simd;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PanMode {
	Stereo(f32),
	SplitStereo(f32, f32),
}

#[derive(Clone, Copy, Debug)]
pub struct Utility {
	pub volume: f32,
	pub pan: PanMode,
}

impl Utility {
	pub fn process(&self, audio: &mut [[f32; 2]]) {
		fn split(pan: f32, fac: f32) -> (f32, f32) {
			let angle = (pan + 1.0) * FRAC_PI_4;
			let (sin, cos) = angle.sin_cos();
			(cos * fac, sin * fac)
		}

		match self.pan {
			PanMode::Stereo(pan) => {
				let (l, r) = split(pan, self.volume * SQRT_2);
				#[cfg(feature = "simd")]
				unsafe {
					simd::utility_process_stereo_sse2(audio, l, r);
				}
				#[cfg(not(feature = "simd"))]
				for [ls, rs] in audio {
					*ls *= l;
					*rs *= r;
				}
			}
			PanMode::SplitStereo(l, r) => {
				let (ll, lr) = split(l, self.volume);
				let (rl, rr) = split(r, self.volume);
				#[cfg(feature = "simd")]
				unsafe {
					simd::utility_process_split_stereo_sse2(audio, ll, lr, rl, rr);
				}
				#[cfg(not(feature = "simd"))]
				for [ls, rs] in audio {
					let ols = *ls;
					*ls = *ls * ll + *rs * rl;
					*rs = *rs * rr + ols * lr;
				}
			}
		}
	}
}

impl Default for Utility {
	fn default() -> Self {
		Self {
			volume: 1.0,
			pan: PanMode::Stereo(0.0),
		}
	}
}
