use utils::NoDebug;

/// Round up to the next power of 2
#[inline]
fn next_pow2(mut n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    n -= 1;
    n |= n >> 1;
    n |= n >> 2;
    n |= n >> 4;
    n |= n >> 8;
    n |= n >> 16;
    #[cfg(target_pointer_width = "64")]
    {
        n |= n >> 32;
    }
    n + 1
}

#[derive(Clone, Debug)]
pub struct DelayLine {
    buf: NoDebug<Vec<[f32; 2]>>,
    head: usize,
    mask: usize,        // buf.len() - 1, for branchless modulo
    cached_len: usize,  // Logical length (<= buf.len())
}

impl DelayLine {
    #[must_use]
    pub fn new(len: usize) -> Self {
        let mut delay_line = Self {
            buf: NoDebug(vec![]),
            head: 0,
            mask: 0,
            cached_len: 0,
        };
        delay_line.resize(len);
        delay_line
    }

    /// Resize the delay line to a new logical length.
    /// Grows the internal buffer to the next power of 2 if needed.
    /// Never shrinks the internal buffer below max observed latency.
    pub fn resize(&mut self, len: usize) {
        if len == self.cached_len {
            return;
        }
        self.cached_len = len;

        // Grow internal buffer to next power of 2 if needed
        if len > self.buf.len() {
            let new_cap = next_pow2(len);
            self.buf.resize(new_cap, [0.0; 2]);
            self.mask = new_cap - 1;
        }
        // Note: We don't shrink the buffer - it only grows to max observed latency.
        // This avoids memmove on latency changes and is fine since max latency is bounded.
    }

    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.cached_len
    }

    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cached_len == 0
    }

    /// Advance delay line by copying audio into it (real-time safe).
    /// Uses branchless ring buffer indexing with power-of-2 mask.
    #[inline]
    pub fn advance(&mut self, buf: &[[f32; 2]]) {
        if self.cached_len == 0 {
            return;
        }

        let write_len = buf.len().min(self.cached_len);
        if write_len == 0 {
            return;
        }

        let mut head = self.head;
        let mask = self.mask;
        let cached_len = self.cached_len;

        if write_len == cached_len {
            // Full buffer write - can copy directly
            for frame in buf.iter() {
                self.buf[head & mask] = *frame;
                head = (head + 1) & mask;
            }
        } else if write_len <= cached_len - head {
            // Single contiguous write
            for (i, frame) in buf.iter().enumerate() {
                self.buf[(head + i) & mask] = *frame;
            }
        } else {
            // Wrapped write
            let first_part = cached_len - head;
            for (i, frame) in buf[..first_part].iter().enumerate() {
                self.buf[head + i] = *frame;
            }
            for (i, frame) in buf[first_part..write_len].iter().enumerate() {
                self.buf[i] = *frame;
            }
        }

        self.head = (head + write_len) & mask;
    }

    /// Advance delay line by swapping with audio buffer (avoids copy, real-time safe).
    /// Uses branchless ring buffer indexing with power-of-2 mask.
    #[inline]
    pub fn advance_mut(&mut self, buf: &mut [[f32; 2]]) {
        if self.cached_len == 0 {
            return;
        }

        let write_len = buf.len().min(self.cached_len);
        if write_len == 0 {
            return;
        }

        let mut head = self.head;
        let mask = self.mask;
        let cached_len = self.cached_len;

        if write_len == cached_len {
            // Full buffer swap - swap directly
            for frame in buf.iter_mut() {
                std::mem::swap(&mut self.buf[head & mask], frame);
                head = (head + 1) & mask;
            }
        } else if write_len <= cached_len - head {
            // Single contiguous swap
            for (i, frame) in buf.iter_mut().enumerate() {
                std::mem::swap(&mut self.buf[(head + i) & mask], frame);
            }
        } else {
            // Wrapped swap
            let first_part = cached_len - head;
            for (i, frame) in buf[..first_part].iter_mut().enumerate() {
                std::mem::swap(&mut self.buf[head + i], frame);
            }
            for (i, frame) in buf[first_part..write_len].iter_mut().enumerate() {
                std::mem::swap(&mut self.buf[i], frame);
            }
        }

        self.head = (head + write_len) & mask;
    }

    #[inline]
    pub fn reset(&mut self) {
        self.head = 0;
        if self.cached_len > 0 {
            self.buf[..self.cached_len].fill([0.0; 2]);
        }
    }
}

impl Default for DelayLine {
    fn default() -> Self {
        Self {
            buf: NoDebug(vec![]),
            head: 0,
            mask: 0,
            cached_len: 0,
        }
    }
}