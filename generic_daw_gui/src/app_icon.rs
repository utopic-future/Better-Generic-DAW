use iced::window;

const SIZE: usize = 128;
const PIXEL_BYTES: usize = 4;
const BACKGROUND: [u8; PIXEL_BYTES] = [17, 25, 40, 255];
const PULSE: [u8; PIXEL_BYTES] = [88, 231, 202, 255];

/// Builds the flat pulse mark used for the application window.
pub fn window_icon() -> window::Icon {
	let mut pixels = vec![0; SIZE * SIZE * PIXEL_BYTES];
	fill_rounded_rect(&mut pixels, 4, 4, 124, 124, 28, BACKGROUND);

	let pulse = [
		(14, 66),
		(43, 66),
		(56, 66),
		(65, 41),
		(75, 91),
		(87, 54),
		(94, 66),
		(114, 66),
	];

	for segment in pulse.windows(2) {
		let [(x0, y0), (x1, y1)] = segment else {
			continue;
		};
		draw_line(&mut pixels, *x0, *y0, *x1, *y1, 7, PULSE);
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
				set_pixel(pixels, x as i32, y as i32, color);
			}
		}
	}
}

fn draw_line(
	pixels: &mut [u8],
	x0: i32,
	y0: i32,
	x1: i32,
	y1: i32,
	thickness: i32,
	color: [u8; PIXEL_BYTES],
) {
	let dx = x1 - x0;
	let dy = y1 - y0;
	let steps = dx.abs().max(dy.abs());
	let radius = thickness / 2;

	for step in 0..=steps {
		let x = x0 + dx * step / steps;
		let y = y0 + dy * step / steps;

		for offset_y in -radius..=radius {
			for offset_x in -radius..=radius {
				if offset_x * offset_x + offset_y * offset_y <= radius * radius {
					set_pixel(pixels, x + offset_x, y + offset_y, color);
				}
			}
		}
	}
}

fn set_pixel(pixels: &mut [u8], x: i32, y: i32, color: [u8; PIXEL_BYTES]) {
	if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
		return;
	}

	let offset = (y as usize * SIZE + x as usize) * PIXEL_BYTES;
	pixels[offset..offset + PIXEL_BYTES].copy_from_slice(&color);
}
