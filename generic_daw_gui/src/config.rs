use crate::{
	daw::{CONFIG_DIR, DATA_DIR},
	shortcuts::Overrides,
	theme::Theme,
};
use generic_daw_core::{DeviceId, HostId};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::{
	fs::{read_to_string, write},
	io,
	num::NonZero,
	path::Path,
	sync::{Arc, LazyLock},
};

pub static CONFIG_PATH: LazyLock<Arc<Path>> =
	LazyLock::new(|| CONFIG_DIR.join("config.toml").into());

/// Where what we remember about the plugins on this computer is written.
pub static PLUGIN_MEMORY_PATH: LazyLock<Arc<Path>> =
	LazyLock::new(|| CONFIG_DIR.join("plugin_memory.toml").into());

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Config {
	pub sample_paths: Vec<Arc<Path>>,
	pub clap_paths: Vec<Arc<Path>>,
	pub audio: Audio,
	pub midi: Midi,
	pub autosave: Autosave,
	pub open_last_project: bool,
	pub scale_factor: f32,
	pub theme: Theme,
	/// The keyboard shortcuts the user changed.
	pub shortcuts: Overrides,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			sample_paths: vec![DATA_DIR.clone()],
			clap_paths: Vec::new(),
			audio: Audio::default(),
			midi: Midi::default(),
			autosave: Autosave::default(),
			open_last_project: false,
			scale_factor: 1.0,
			theme: Theme::CatppuccinFrappe,
			shortcuts: Overrides::default(),
		}
	}
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Audio {
	pub devices: Devices,
	pub sample_rate: Option<NonZero<u32>>,
	pub buffer_size: Option<NonZero<u32>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Devices {
	#[default]
	Default,
	WithHost {
		host: HostId,
		input: Option<Arc<str>>,
		output: Option<Arc<str>>,
	},
}

impl Devices {
	pub fn get_host(&self) -> Option<HostId> {
		match self {
			Self::Default => None,
			Self::WithHost { host, .. } => Some(*host),
		}
	}

	pub fn set_host(&mut self, host: Option<HostId>) {
		*self = host.map_or_default(|host| Self::WithHost {
			host,
			input: None,
			output: None,
		});
	}

	pub fn get_input(&self) -> Option<DeviceId> {
		match self {
			Self::WithHost {
				host,
				input: Some(input),
				..
			} => Some(DeviceId::new(*host, input)),
			Self::Default | Self::WithHost { .. } => None,
		}
	}

	pub fn set_input(&mut self, input: Option<DeviceId>) {
		*self = match (self.clone(), input) {
			(Self::Default, None) => Self::Default,
			(Self::WithHost { host, output, .. }, None) => Self::WithHost {
				host,
				input: None,
				output,
			},
			(Self::Default, Some(input)) => Self::WithHost {
				host: input.host(),
				input: Some(input.id().into()),
				output: None,
			},
			(Self::WithHost { host, output, .. }, Some(input)) => Self::WithHost {
				output: output.filter(|_| host == input.host()),
				host: input.host(),
				input: Some(input.id().into()),
			},
		};
	}

	pub fn get_output(&self) -> Option<DeviceId> {
		match self {
			Self::WithHost {
				host,
				output: Some(output),
				..
			} => Some(DeviceId::new(*host, output)),
			Self::Default | Self::WithHost { .. } => None,
		}
	}

	pub fn set_output(&mut self, output: Option<DeviceId>) {
		*self = match (self.clone(), output) {
			(Self::Default, None) => Self::Default,
			(Self::WithHost { host, input, .. }, None) => Self::WithHost {
				host,
				input,
				output: None,
			},
			(Self::Default, Some(output)) => Self::WithHost {
				host: output.host(),
				input: None,
				output: Some(output.id().into()),
			},
			(Self::WithHost { host, input, .. }, Some(output)) => Self::WithHost {
				input: input.filter(|_| host == output.host()),
				host: output.host(),
				output: Some(output.id().into()),
			},
		};
	}

	pub fn as_core(&self) -> generic_daw_core::Devices {
		match self.clone() {
			Self::Default => generic_daw_core::Devices::Default,
			Self::WithHost {
				host,
				input,
				output,
			} => generic_daw_core::Devices::WithHost {
				host,
				input,
				output,
			},
		}
	}
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Midi {
	pub input: Option<Arc<str>>,
	pub output: Option<Arc<str>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Autosave {
	pub enabled: bool,
	pub interval: NonZero<u16>,
}

impl Default for Autosave {
	fn default() -> Self {
		Self {
			enabled: true,
			interval: NonZero::new(300).unwrap(),
		}
	}
}

impl Config {
	pub fn read() -> Self {
		let read = match read_to_string(&*CONFIG_PATH) {
			Ok(read) => match toml::from_str(&read) {
				Ok(read) => read,
				Err(err) => {
					warn!("{err}");
					Self::default()
				}
			},
			Err(err) if err.kind() == io::ErrorKind::NotFound => {
				let read = Self::default();
				read.write();
				read
			}
			Err(err) => {
				warn!("{err}");
				Self::default()
			}
		};

		info!("loaded config {read:#?}");

		read
	}

	pub fn write(&self) {
		write(&*CONFIG_PATH, toml::to_string(self).unwrap()).unwrap();
	}
}

#[cfg(test)]
mod tests {
	use crate::shortcuts::{Action, Chord, Key};

	use super::*;

	#[test]
	fn the_config_can_be_written_and_read() {
		let mut config = Config::default();

		config.shortcuts.set(
			Action::SaveFile,
			vec![Chord::commanded(Key::Character('k'))],
		);
		config.shortcuts.set(Action::Delete, Vec::new());

		let text = toml::to_string(&config).unwrap();
		let read = toml::from_str::<Config>(&text).unwrap();

		assert_eq!(read, config);
	}

	#[test]
	fn a_config_without_shortcuts_is_read() {
		let read = toml::from_str::<Config>("scale_factor = 1.0").unwrap();

		assert_eq!(read.shortcuts, Overrides::default());
	}

	#[test]
	fn the_shortcuts_are_read_from_the_config() {
		let text = r#"
scale_factor = 1.0

[[shortcuts.bindings]]
action = "SaveFile"
chords = ["Ctrl+K"]

[[shortcuts.bindings]]
action = "Delete"
chords = []
"#;

		let read = toml::from_str::<Config>(text).unwrap();

		assert_eq!(read.shortcuts.bindings.len(), 2);

		assert_eq!(read.shortcuts.bindings[0].action, Action::SaveFile,);

		assert_eq!(
			read.shortcuts.bindings[0].chords,
			vec![Chord::commanded(Key::Character('k'))],
		);

		assert_eq!(read.shortcuts.bindings[1].action, Action::Delete);
		assert!(read.shortcuts.bindings[1].chords.is_empty());
	}
}

mod devices_serde {
	use serde::{Deserialize, Deserializer, Serialize, Serializer};
	use std::sync::Arc;

	#[derive(Deserialize, Serialize)]
	struct Devices {
		host: Arc<str>,
		input: Option<Arc<str>>,
		output: Option<Arc<str>>,
	}

	impl Serialize for super::Devices {
		fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
			match self {
				Self::Default => None,
				Self::WithHost {
					host,
					input,
					output,
				} => Some(Devices {
					host: host.to_string().into(),
					input: input.clone(),
					output: output.clone(),
				}),
			}
			.serialize(serializer)
		}
	}

	impl<'de> Deserialize<'de> for super::Devices {
		fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
			Ok(match Option::<Devices>::deserialize(deserializer)? {
				None => Self::Default,
				Some(data) => Self::WithHost {
					host: data.host.parse().map_err(serde::de::Error::custom)?,
					input: data.input,
					output: data.output,
				},
			})
		}
	}
}
