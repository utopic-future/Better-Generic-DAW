/// Cubic interpolation resampling - optimized for real-time use
/// 
/// Uses Catmull-Rom spline interpolation for smooth resampling.
/// This version uses bounds checking but is structured for auto-vectorization.
pub fn resample_cubic(
	samples: &[[f32; 2]],
	resample_ratio: f64,
	offset: usize,
) -> impl Iterator<Item = [f32; 2]> + '_ {
	let len = samples.len();
	
	let mut frame = offset as f64 * resample_ratio;
	
	if resample_ratio.is_sign_negative() {
		frame += len as f64;
	}
	
	std::iter::repeat_with(move || {
		let fract = frame.fract() as f32;
		let idx = frame as usize;
		frame += resample_ratio;
		
		// Catmull-Rom spline interpolation with bounds clamping
		// Indices: idx-1, idx, idx+1, idx+2
		let idx_m1 = idx.saturating_sub(1).min(len - 1);
		let idx_0 = idx.min(len - 1);
		let idx_p1 = (idx + 1).min(len - 1);
		let idx_p2 = (idx + 2).min(len - 1);
		
		let s0 = samples[idx_m1];
		let s1 = samples[idx_0];
		let s2 = samples[idx_p1];
		let s3 = samples[idx_p2];
		
		[
			interp_cubic(s0[0], s1[0], s2[0], s3[0], fract),
			interp_cubic(s0[1], s1[1], s2[1], s3[1], fract),
		]
	})
}

/// Fast linear interpolation - much cheaper than cubic
#[inline]
pub fn resample_linear(
	samples: &[[f32; 2]],
	resample_ratio: f64,
	offset: usize,
) -> impl Iterator<Item = [f32; 2]> + '_ {
	let mut frame = offset as f64 * resample_ratio;
	let len = samples.len();
	
	if resample_ratio.is_sign_negative() {
		frame += len as f64;
	}
	
	std::iter::repeat_with(move || {
		let idx = frame as usize;
		let fract = frame.fract() as f32;
		frame += resample_ratio;
		
		// Get samples with clamping for safety
		let s1 = samples.get(idx.min(len - 1)).copied().unwrap_or([0.0; 2]);
		let s2 = samples.get((idx + 1).min(len - 1)).copied().unwrap_or([0.0; 2]);
		
		[
			s1[0] + (s2[0] - s1[0]) * fract,
			s1[1] + (s2[1] - s1[1]) * fract,
		]
	})
}

/// Cubic interpolation (Catmull-Rom spline)
/// Coefficients computed once for efficiency
#[inline]
fn interp_cubic(s0: f32, s1: f32, s2: f32, s3: f32, fract: f32) -> f32 {
	// Pre-compute fractions
	let t = fract;
	let t2 = t * t;
	let t3 = t2 * t;
	
	// Catmull-Rom coefficients
	let c0 = s1;
	let c1 = (s2 - s0) * 0.5;
	let c2 = s1 - s2 * 2.5 + s0 * 2.0 - s3 * 0.5;
	let c3 = (s2 - s1) * 1.5 - s0 * 0.5 + s3 * 0.5;
	
	c3 * t3 + c2 * t2 + c1 * t + c0
}

/// Nearest-neighbor resampling (for very fast, low-quality playback)
#[inline]
pub fn resample_nearest(
	samples: &[[f32; 2]],
	resample_ratio: f64,
	offset: usize,
) -> impl Iterator<Item = [f32; 2]> + '_ {
	let mut frame = offset as f64 * resample_ratio;
	let len = samples.len();
	
	if resample_ratio.is_sign_negative() {
		frame += len as f64;
	}
	
	std::iter::repeat_with(move || {
		let idx = frame as usize;
		frame += resample_ratio;
		
		samples.get(idx.min(len - 1)).copied().unwrap_or([0.0; 2])
	})
}