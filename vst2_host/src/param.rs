use std::sync::Arc;

/// What a parameter can do and how the host should treat it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParamFlags(pub i32);

impl ParamFlags {
	/// The host may automate the parameter.
	pub const CAN_AUTOMATE: i32 = 1;
	/// The parameter is a switch, on or off.
	pub const IS_SWITCH: i32 = 1 << 1;
	/// The parameter has a whole number of steps.
	pub const IS_DISCRETE: i32 = 1 << 2;

	#[must_use]
	pub const fn contains(self, flag: i32) -> bool {
		self.0 & flag == flag
	}
}

/// Everything the plugin says about one parameter.
///
/// All values the host sees and sets are normalized, i.e. in `0.0..=1.0`. The plugin
/// knows how to turn them into display strings and into "plain" values with units.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
	/// The index of the parameter, which is how the plugin knows it.
	pub id: u32,
	pub title: Arc<str>,
	pub short_title: Arc<str>,
	pub units: Arc<str>,
	/// `0` for continuous parameters, `1` for on/off switches and `n` for a parameter
	/// with `n + 1` distinct values.
	pub step_count: i32,
	/// The value the parameter had when the plugin was loaded.
	pub default_normalized: f64,
	pub flags: ParamFlags,
}

impl ParamInfo {
	/// Whether the host may automate the parameter.
	#[must_use]
	pub const fn can_automate(&self) -> bool {
		self.flags.contains(ParamFlags::CAN_AUTOMATE)
	}

	/// Whether the parameter is a switch, on or off.
	#[must_use]
	pub const fn is_switch(&self) -> bool {
		self.flags.contains(ParamFlags::IS_SWITCH)
	}

	/// Whether the parameter has a fixed number of distinct values.
	#[must_use]
	pub const fn is_discrete(&self) -> bool {
		self.step_count > 0
	}

	/// Whether a generic parameter UI should show the parameter.
	#[must_use]
	#[expect(
		clippy::unused_self,
		reason = "it's a method so that it can be used like the ones of other formats"
	)]
	pub const fn is_visible(&self) -> bool {
		true
	}

	/// Clamps `normalized` to `0.0..=1.0`, and snaps it to the nearest step if the
	/// parameter is discrete.
	#[must_use]
	pub fn snap(&self, normalized: f64) -> f64 {
		// NaN would otherwise get passed through to the plugin
		let normalized = if normalized.is_nan() {
			self.default_normalized
		} else {
			normalized.clamp(0.0, 1.0)
		};

		if self.is_discrete() {
			let steps = f64::from(self.step_count);
			(normalized * steps).round() / steps
		} else {
			normalized
		}
	}
}
