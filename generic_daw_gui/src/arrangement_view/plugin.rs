use crate::{clap_host::PluginId, daw, plugin_descriptor::PluginDescriptor, vst2_host, vst3_host};
use generic_daw_core::{
	PushSlot,
	clap_host::{self, HostInfo},
	vst2_host as vst2, vst3_host as vst3,
};
use std::sync::mpsc::Receiver;
use utils::NoDebug;

#[derive(Debug)]
pub struct Plugin {
	pub id: PluginId,
	pub descriptor: PluginDescriptor,
	pub active: bool,
	pub mix: f32,
	pub s: oneshot::Sender<PushSlot<Option<generic_daw_core::Plugin>>>,
}

impl Plugin {
	/// Asks for the plugin to be set up for audio.
	pub fn activate_message(&self) -> daw::Message {
		match self.descriptor {
			PluginDescriptor::Clap(_) => {
				daw::Message::ClapHost(clap_host_message::activate(self.id))
			}
			PluginDescriptor::Vst2(_) => {
				daw::Message::Vst2Host(vst2_host::Message::Activate(self.id))
			}
			PluginDescriptor::Vst3(_) => {
				daw::Message::Vst3Host(vst3_host::Message::Activate(self.id))
			}
		}
	}

	/// Asks for the plugin's window to be shown.
	pub fn gui_open_message(&self) -> daw::Message {
		match self.descriptor {
			PluginDescriptor::Clap(_) => {
				daw::Message::ClapHost(clap_host_message::gui_open(self.id))
			}
			PluginDescriptor::Vst2(_) => {
				daw::Message::Vst2Host(vst2_host::Message::GuiOpen(self.id))
			}
			PluginDescriptor::Vst3(_) => {
				daw::Message::Vst3Host(vst3_host::Message::GuiOpen(self.id))
			}
		}
	}

	/// Asks for the plugin to be destroyed, once it isn't active.
	pub fn destroy_inactive_message(&self) -> daw::Message {
		match self.descriptor {
			PluginDescriptor::Clap(_) => {
				daw::Message::ClapHost(clap_host_message::destroy_inactive(self.id))
			}
			PluginDescriptor::Vst2(_) => {
				daw::Message::Vst2Host(vst2_host::Message::DestroyInactive(self.id))
			}
			PluginDescriptor::Vst3(_) => {
				daw::Message::Vst3Host(vst3_host::Message::DestroyInactive(self.id))
			}
		}
	}

	/// Asks for a saved state to be loaded into the plugin.
	pub fn set_state_message(&self, state: Box<[u8]>) -> daw::Message {
		match self.descriptor {
			PluginDescriptor::Clap(_) => {
				daw::Message::ClapHost(clap_host_message::set_state(self.id, state))
			}
			PluginDescriptor::Vst2(_) => {
				daw::Message::Vst2Host(vst2_host::Message::SetState(self.id, NoDebug(state)))
			}
			PluginDescriptor::Vst3(_) => {
				daw::Message::Vst3Host(vst3_host::Message::SetState(self.id, NoDebug(state)))
			}
		}
	}
}

/// The messages that go to the host of CLAP plugins.
mod clap_host_message {
	use crate::clap_host::{Message, PluginId};
	use utils::NoDebug;

	pub const fn activate(id: PluginId) -> Message {
		Message::Activate(id)
	}

	pub const fn gui_open(id: PluginId) -> Message {
		Message::GuiOpen(id)
	}

	pub const fn destroy_inactive(id: PluginId) -> Message {
		Message::DestroyInactive(id)
	}

	pub fn set_state(id: PluginId, state: Box<[u8]>) -> Message {
		Message::SetState(id, NoDebug(state))
	}
}

/// What a plugin is, depending on its format.
pub enum Core {
	Clap(clap_host::Plugin, Receiver<clap_host::MainThreadMessage>),
	Vst2(vst2::Plugin, Receiver<vst2::MainThreadMessage>),
	Vst3(vst3::Plugin, Receiver<vst3::MainThreadMessage>),
}

pub struct PluginPair {
	pub core: Core,
	pub gui: Plugin,
}

impl PluginPair {
	pub fn new(
		descriptor: PluginDescriptor,
		host: HostInfo,
	) -> Option<(Self, PushSlot<Option<generic_daw_core::Plugin>>)> {
		let core = match &descriptor {
			PluginDescriptor::Clap(descriptor) => {
				let (plugin, receiver) = clap_host::Plugin::new(descriptor, host)?;
				Core::Clap(plugin, receiver)
			}
			PluginDescriptor::Vst2(descriptor) => {
				let (plugin, receiver) = vst2::Plugin::new(descriptor)
					.inspect_err(|err| log::warn!("{}: {err}", descriptor.name))
					.ok()?;
				Core::Vst2(plugin, receiver)
			}
			PluginDescriptor::Vst3(descriptor) => {
				let (plugin, receiver) = vst3::Plugin::new(descriptor)
					.inspect_err(|err| log::warn!("{}: {err}", descriptor.name))
					.ok()?;
				Core::Vst3(plugin, receiver)
			}
		};

		let (s, r) = oneshot::channel();
		let gui = Plugin {
			id: PluginId::unique(),
			descriptor,
			active: false,
			mix: 1.0,
			s,
		};

		Some((Self { core, gui }, PushSlot::new(Some(None), r)))
	}
}
