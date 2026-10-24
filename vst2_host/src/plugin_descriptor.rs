use crate::api::categories;
use std::{
	fmt::{Display, Formatter},
	path::Path,
	sync::Arc,
};

/// A plugin that can be loaded.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PluginDescriptor {
	/// The id of the plugin, which tells plugins apart.
	pub unique_id: i32,
	pub name: Arc<str>,
	pub vendor: Option<Arc<str>>,
	pub version: Option<Arc<str>>,
	/// What kind of plugin this is, see [`categories`].
	pub category: i32,
	pub path: Arc<Path>,
}

impl PluginDescriptor {
	/// Whether the plugin makes sound out of notes.
	#[must_use]
	pub const fn is_instrument(&self) -> bool {
		self.category == categories::SYNTH
	}

	/// Whether the plugin only looks at audio.
	#[must_use]
	pub const fn is_analyzer(&self) -> bool {
		self.category == categories::ANALYSIS
	}
}

impl Display for PluginDescriptor {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		self.name.fmt(f)
	}
}
