use crate::{MainThreadMessage, host::LIVE_HOST_OBJECTS, main_thread::RestartFlags};
use std::sync::{
	Arc,
	atomic::{AtomicI32, Ordering::Relaxed},
	mpsc::Sender,
};
use vst3::{
	Class,
	Steinberg::{
		Vst::{IComponentHandler, IComponentHandlerTrait, ParamID, ParamValue},
		int32, kResultOk, tresult,
	},
};

/// The host side of `IComponentHandler`: the plugin calls this when the user edits a parameter in
/// the plugin's UI, or when it wants the host to restart it.
///
/// Calls can come from any thread, so everything is forwarded through a channel.
#[derive(Debug)]
pub struct ComponentHandler {
	sender: Sender<MainThreadMessage>,
	/// The restart flags that were requested since the audio thread last looked, so that it can
	/// react without waiting for the main thread.
	restart_flags: Arc<AtomicI32>,
}

impl ComponentHandler {
	#[must_use]
	pub fn new(sender: Sender<MainThreadMessage>) -> Self {
		Self::with_restart_flags(sender, Arc::new(AtomicI32::new(0)))
	}

	#[must_use]
	pub fn with_restart_flags(
		sender: Sender<MainThreadMessage>,
		restart_flags: Arc<AtomicI32>,
	) -> Self {
		LIVE_HOST_OBJECTS.fetch_add(1, Relaxed);
		Self {
			sender,
			restart_flags,
		}
	}

	fn send(&self, message: MainThreadMessage) -> tresult {
		// If the receiver is gone the plugin is being torn down, which isn't an error.
		let _ = self.sender.send(message);
		kResultOk
	}
}

impl Drop for ComponentHandler {
	fn drop(&mut self) {
		LIVE_HOST_OBJECTS.fetch_sub(1, Relaxed);
	}
}

impl Class for ComponentHandler {
	type Interfaces = (IComponentHandler,);
}

#[expect(
	clippy::renamed_function_params,
	reason = "the names of the trait's parameters aren't snake case"
)]
impl IComponentHandlerTrait for ComponentHandler {
	unsafe fn beginEdit(&self, id: ParamID) -> tresult {
		self.send(MainThreadMessage::BeginEdit(id))
	}

	unsafe fn performEdit(&self, id: ParamID, value_normalized: ParamValue) -> tresult {
		self.send(MainThreadMessage::PerformEdit(id, value_normalized))
	}

	unsafe fn endEdit(&self, id: ParamID) -> tresult {
		self.send(MainThreadMessage::EndEdit(id))
	}

	unsafe fn restartComponent(&self, flags: int32) -> tresult {
		self.restart_flags.fetch_or(flags, Relaxed);
		self.send(MainThreadMessage::Restart(RestartFlags(flags)))
	}
}
