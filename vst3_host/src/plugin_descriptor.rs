use crate::util::non_empty;
use std::{
	fmt::{Display, Formatter},
	path::Path,
	sync::Arc,
};

/// The VST3 class category of audio processors.
pub const AUDIO_MODULE_CLASS: &str = "Audio Module Class";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PluginDescriptor {
	/// The 16-byte class id (`TUID`) used to instantiate the plugin.
	pub id: [u8; 16],
	pub name: Arc<str>,
	pub vendor: Option<Arc<str>>,
	pub version: Option<Arc<str>>,
	/// The subcategories, e.g. `["Fx", "Reverb"]` or `["Instrument", "Synth"]`.
	pub subcategories: Arc<[Arc<str>]>,
	pub path: Arc<Path>,
}

impl PluginDescriptor {
	pub(crate) fn new(
		id: [u8; 16],
		name: String,
		vendor: String,
		version: String,
		subcategories: &str,
		path: &Arc<Path>,
	) -> Self {
		Self {
			id,
			name: name.into(),
			vendor: non_empty(vendor),
			version: non_empty(version),
			subcategories: subcategories
				.split('|')
				.map(str::trim)
				.filter(|s| !s.is_empty())
				.map(Arc::from)
				.collect(),
			path: path.clone(),
		}
	}

	#[must_use]
	pub fn is_instrument(&self) -> bool {
		self.subcategories.iter().any(|s| &**s == "Instrument")
	}

	/// The class id as 32 uppercase hex characters, suitable for project files.
	#[must_use]
	pub fn id_hex(&self) -> String {
		use std::fmt::Write as _;

		self.id.iter().fold(String::with_capacity(32), |mut s, b| {
			write!(s, "{b:02X}").unwrap();
			s
		})
	}
}

impl Display for PluginDescriptor {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		self.name.fmt(f)
	}
}
