//! SIMD-optimized DSP functions

#[cfg(all(target_arch = "x86_64", feature = "simd"))]
mod simd_impl {
    use std::arch::x86_64::*;

    /// Apply volume and pan (Stereo mode) using SSE2
    /// Processes 4 frames at a time
    #[target_feature(enable = "sse2")]
    unsafe fn utility_process_stereo_sse2_impl(
        audio: &mut [[f32; 2]],
        l_gain: f32,
        r_gain: f32,
    ) {
        let len = audio.len();
        let l_vec = _mm_set1_ps(l_gain);
        let r_vec = _mm_set1_ps(r_gain);
        let mut i = 0;

        while i + 4 <= len {
            let l = _mm_loadu_ps(&audio[i][0] as *const f32);
            let r = _mm_loadu_ps(&audio[i][1] as *const f32);

            let l_new = _mm_mul_ps(l, l_vec);
            let r_new = _mm_mul_ps(r, r_vec);

            _mm_storeu_ps(&mut audio[i][0] as *mut f32, l_new);
            _mm_storeu_ps(&mut audio[i][1] as *mut f32, r_new);

            i += 4;
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
        let ll_vec = _mm_set1_ps(ll);
        let lr_vec = _mm_set1_ps(lr);
        let rl_vec = _mm_set1_ps(rl);
        let rr_vec = _mm_set1_ps(rr);
        let mut i = 0;

        while i + 4 <= len {
            let ls = _mm_loadu_ps(&audio[i][0] as *const f32);
            let rs = _mm_loadu_ps(&audio[i][1] as *const f32);

            // new_ls = ls * ll + rs * rl
            // new_rs = rs * rr + ls * lr
            let new_ls = _mm_add_ps(_mm_mul_ps(ls, ll_vec), _mm_mul_ps(rs, rl_vec));
            let new_rs = _mm_add_ps(_mm_mul_ps(rs, rr_vec), _mm_mul_ps(ls, lr_vec));

            _mm_storeu_ps(&mut audio[i][0] as *mut f32, new_ls);
            _mm_storeu_ps(&mut audio[i][1] as *mut f32, new_rs);

            i += 4;
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