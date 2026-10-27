use iced::window;

const SIZE: usize = 128;
const PIXEL_BYTES: usize = 4;

/// Builds a small mixer-and-waveform mark for the application window.
pub fn window_icon() -> window::Icon {
	let mut pixels = vec![0; SIZE * SIZE * PIXEL_BYTES];

	fill_rounded_rect(&mut pixels, 4, 4, 124, 124, 28, [17, 25, 40, 255]);

	let track = [49, 66, 86, 255];
	for x in [34, 64, 94] {
		fill_rounded_rect(&mut pixels, x - 4, 25, x + 4, 80, 4, track);
	}

	fill_rounded_rect(&mut pixels, 23, 37, 45, 52, 5, [65, 217, 194, 255]);
	fill_rounded_rect(&mut pixels, 53, 55, 75, 70, 5, [105, 166, 255, 255]);
	fill_rounded_rect(&mut pixels, 83, 32, 105, 47, 5, [177, 139, 255, 255]);

	let waveform = [8, 16, 23, 12, 20, 27, 14, 22, 10];
	for (index, height) in waveform.into_iter().enumerate() {
		let x = 25 + index * 10;
		let top = 101 - height / 2;
		fill_rounded_rect(&mut pixels, x, top, x + 4, top + height, 2, [184, 207, 230, 255]);
	}

	window::icon::from_rgba(pixels, SIZE as u32, SIZE as u32)
		.expect("the built-in application icon has valid RGBA dimensions")
}

fn fill_rounded_rect(
	pixels: &mut [u8],
	left: usize,
	top: usize,
	right: usize,
	bottom: usize,
	radius: usize,
	color: [u8; PIXEL_BYTES],
) {
	for y in top..bottom {
		for x in left..right {
			let dx = if x < left + radius {
				left + radius - x
			} else if x >= right - radius {
				x - (right - radius - 1)
			} else {
				0
			};
			let dy = if y < top + radius {
				top + radius - y
			} else if y >= bottom - radius {
				y - (bottom - radius - 1)
			} else {
				0
			};

			if dx * dx + dy * dy <= radius * radius {
				let offset = (y * SIZE + x) * PIXEL_BYTES;
				pixels[offset..offset + PIXEL_BYTES].copy_from_slice(&color);
			}
		}
	}
}
