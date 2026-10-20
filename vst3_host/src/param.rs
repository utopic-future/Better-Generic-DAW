use crate::util::utf16_array_to_string;
use std::sync::Arc;
use vst3::Steinberg::Vst::{ParameterInfo, ParameterInfo_::ParameterFlags_};

/// What a parameter can do and how the host should treat it (`Vst::ParameterInfo::ParameterFlags`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParamFlags(pub i32);

impl ParamFlags {
	/// The host may automate the parameter.
	pub const CAN_AUTOMATE: i32 = ParameterFlags_::kCanAutomate;
	/// The host must not change the parameter, it only displays it (e.g. a meter).
	pub const IS_READ_ONLY: i32 = ParameterFlags_::kIsReadOnly;
	/// Going past the maximum wraps around to the minimum (e.g. a phase knob).
	pub const IS_WRAP_AROUND: i32 = ParameterFlags_::kIsWrapAround;
	/// The parameter picks from a list of named entries.
	pub const IS_LIST: i32 = ParameterFlags_::kIsList;
	/// The host shouldn't show the parameter.
	pub const IS_HIDDEN: i32 = ParameterFlags_::kIsHidden;
	/// The parameter selects a program (preset), the host shouldn't automate it like a knob.
	pub const IS_PROGRAM_CHANGE: i32 = ParameterFlags_::kIsProgramChange;
	/// The parameter is the plugin's bypass switch.
	pub const IS_BYPASS: i32 = ParameterFlags_::kIsBypass;

	#[must_use]
	pub const fn contains(self, flag: i32) -> bool {
		self.0 & flag == flag
	}
}

/// Everything a controller reports about one parameter.
///
/// All values the host sees and sets are normalized, i.e. in `0.0..=1.0`. The plugin knows how to
/// turn them into display strings and into "plain" values with units.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
	pub id: u32,
	pub title: Arc<str>,
	pub short_title: Arc<str>,
	pub units: Arc<str>,
	/// `0` for continuous parameters, `1` for on/off switches and `n` for a parameter with `n + 1`
	/// distinct values.
	pub step_count: i32,
	pub default_normalized: f64,
	pub unit_id: i32,
	pub flags: ParamFlags,
}

impl ParamInfo {
	pub(crate) fn from_raw(info: &ParameterInfo) -> Self {
		Self {
			id: info.id,
			title: utf16_array_to_string(&info.title).into(),
			short_title: utf16_array_to_string(&info.shortTitle).into(),
			units: utf16_array_to_string(&info.units).into(),
			step_count: info.stepCount,
			default_normalized: info.defaultNormalizedValue,
			unit_id: info.unitId,
			flags: ParamFlags(info.flags),
		}
	}

	#[must_use]
	pub const fn can_automate(&self) -> bool {
		self.flags.contains(ParamFlags::CAN_AUTOMATE)
	}

	#[must_use]
	pub const fn is_read_only(&self) -> bool {
		self.flags.contains(ParamFlags::IS_READ_ONLY)
	}

	#[must_use]
	pub const fn is_hidden(&self) -> bool {
		self.flags.contains(ParamFlags::IS_HIDDEN)
	}

	#[must_use]
	pub const fn is_bypass(&self) -> bool {
		self.flags.contains(ParamFlags::IS_BYPASS)
	}

	#[must_use]
	pub const fn is_list(&self) -> bool {
		self.flags.contains(ParamFlags::IS_LIST)
	}

	#[must_use]
	pub const fn is_program_change(&self) -> bool {
		self.flags.contains(ParamFlags::IS_PROGRAM_CHANGE)
	}

	#[must_use]
	pub const fn is_wrap_around(&self) -> bool {
		self.flags.contains(ParamFlags::IS_WRAP_AROUND)
	}

	/// Whether the parameter has a fixed number of distinct values.
	#[must_use]
	pub const fn is_discrete(&self) -> bool {
		self.step_count > 0
	}

	/// Whether a generic parameter UI should show the parameter.
	#[must_use]
	pub const fn is_visible(&self) -> bool {
		!self.is_hidden() && !self.is_program_change()
	}

	/// Clamps `normalized` to `0.0..=1.0`, and snaps it to the nearest step if the parameter is
	/// discrete.
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
