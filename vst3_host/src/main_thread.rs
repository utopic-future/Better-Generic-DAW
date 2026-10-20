use vst3::Steinberg::Vst::RestartFlags_;

/// A message from the plugin to the host, delivered through a channel so that it can be handled
/// on the main thread, regardless of which thread the plugin sent it from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MainThreadMessage {
	/// The user started touching a parameter in the plugin's own UI.
	BeginEdit(u32),
	/// The user moved a parameter in the plugin's own UI, the value is normalized (0 to 1).
	PerformEdit(u32, f64),
	/// The user let go of a parameter in the plugin's own UI.
	EndEdit(u32),
	/// The plugin wants to be restarted or rescanned.
	Restart(RestartFlags),
	/// The plugin's editor wants to be a different size. The host resizes its window and then
	/// calls [`Plugin::apply_editor_size`](crate::Plugin::apply_editor_size).
	ResizeEditor { width: i32, height: i32 },
}

/// What the plugin wants rescanned (`Vst::RestartFlags`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RestartFlags(pub i32);

impl RestartFlags {
	pub const RELOAD_COMPONENT: i32 = RestartFlags_::kReloadComponent;
	pub const IO_CHANGED: i32 = RestartFlags_::kIoChanged;
	pub const PARAM_VALUES_CHANGED: i32 = RestartFlags_::kParamValuesChanged;
	pub const LATENCY_CHANGED: i32 = RestartFlags_::kLatencyChanged;
	pub const PARAM_TITLES_CHANGED: i32 = RestartFlags_::kParamTitlesChanged;

	#[must_use]
	pub const fn contains(self, flag: i32) -> bool {
		self.0 & flag == flag
	}

	/// Whether the audio processor has to be torn down and set up again.
	#[must_use]
	pub const fn needs_restart(self) -> bool {
		self.contains(Self::RELOAD_COMPONENT) || self.contains(Self::IO_CHANGED)
	}
}

#[expect(
	clippy::struct_excessive_bools,
	reason = "these are independent things the host may have to do"
)]
/// What the host has to do after the plugin asked to be restarted. Made by
/// [`Plugin::handle_restart`](crate::Plugin::handle_restart), which has already done the parts
/// that only concern the plugin object.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RestartActions {
	/// The audio processing has to be torn down and set up again, with
	/// [`Plugin::restart`](crate::Plugin::restart). That also picks up a changed latency.
	pub reactivate: bool,
	/// The latency changed, call [`AudioThread::update_latency`](crate::AudioThread::update_latency).
	pub update_latency: bool,
	/// Parameter values changed behind the host's back, read them all again.
	pub reread_param_values: bool,
	/// The list of parameters (or their names) changed. [`Plugin::params`](crate::Plugin::params)
	/// is up to date, so a parameter UI has to be rebuilt.
	pub params_changed: bool,
}
