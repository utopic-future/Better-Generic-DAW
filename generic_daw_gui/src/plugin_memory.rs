use crate::{config::PLUGIN_MEMORY_PATH, plugin_descriptor::PluginDescriptor};
use generic_daw_core::{
	clap_host::{self, Capability, Category, Feature, Subcategory},
	vst2_host,
	vst3_host,
};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::{
	collections::{HashMap, HashSet},
	ffi::CString,
	fs::{read_to_string, write},
	io,
	path::PathBuf,
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

/// How many recently used plugins are remembered.
pub const MAX_RECENT: usize = 50;

/// The format a plugin is in.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PluginFormat {
	Clap,
	Vst2,
	Vst3,
}

/// Tells plugins of different formats with the same name apart, and the same plugin in
/// different places apart.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PluginKey {
	/// The format the plugin is in.
	pub format: PluginFormat,
	/// Where the plugin is.
	pub path: PathBuf,
	/// The CLAP id of the plugin, or the hex digits of its VST3 class id.
	pub id: String,
}

impl PluginKey {
	/// The key of a plugin.
	#[must_use]
	pub fn from_descriptor(descriptor: &PluginDescriptor) -> Self {
		match descriptor {
			PluginDescriptor::Clap(descriptor) => Self {
				format: PluginFormat::Clap,
				path: descriptor.path.to_path_buf(),
				id: descriptor.id.to_string_lossy().into_owned(),
			},
			PluginDescriptor::Vst2(descriptor) => Self {
				format: PluginFormat::Vst2,
				path: descriptor.path.to_path_buf(),
				id: descriptor.unique_id.to_string(),
			},
			PluginDescriptor::Vst3(descriptor) => Self {
				format: PluginFormat::Vst3,
				path: descriptor.path.to_path_buf(),
				id: descriptor.id_hex(),
			},
		}
	}
}

/// What we know about a plugin we've seen.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PluginEntry {
	/// What tells this plugin apart from the others.
	pub key: PluginKey,
	/// The name of the plugin.
	pub name: Arc<str>,
	/// The vendor of the plugin.
	#[serde(default)]
	pub vendor: Option<Arc<str>>,
	/// The version of the plugin.
	#[serde(default)]
	pub version: Option<Arc<str>>,
	/// The features of the plugin, as they're shown to the user. For VST3 plugins these
	/// are the subcategories.
	#[serde(default)]
	pub features: Vec<Arc<str>>,
	/// When the plugin was last found by a scan, in seconds since the unix epoch.
	#[serde(default)]
	pub last_scanned: u64,
	/// When the plugin was last loaded, in seconds since the unix epoch.
	#[serde(default)]
	pub last_used: Option<u64>,
	/// How many times the plugin has been loaded.
	#[serde(default)]
	pub use_count: u32,
	/// The tags the user gave the plugin.
	#[serde(default)]
	pub tags: Vec<Arc<str>>,
	/// Whether the plugin was missing the last time a scan finished.
	#[serde(default)]
	pub missing: bool,
}

impl PluginEntry {
	/// The plugin this entry is about, if it can be restored.
	#[must_use]
	pub fn descriptor(&self) -> Option<PluginDescriptor> {
		match self.key.format {
			PluginFormat::Clap => {
				let id = CString::new(self.key.id.as_str()).ok()?;
				let features = self
					.features
					.iter()
					.filter_map(|feature| parse_feature(feature))
					.collect::<Vec<_>>();

				Some(PluginDescriptor::Clap(clap_host::PluginDescriptor {
					id: Arc::from(id.as_c_str()),
					name: self.name.clone(),
					vendor: self.vendor.clone(),
					version: self.version.clone(),
					features: features.into(),
					path: Arc::from(self.key.path.as_path()),
				}))
			}
			PluginFormat::Vst2 => {
				let unique_id = self.key.id.parse().ok()?;

				Some(PluginDescriptor::Vst2(vst2_host::PluginDescriptor {
					unique_id,
					name: self.name.clone(),
					vendor: self.vendor.clone(),
					version: self.version.clone(),
					category: vst2_host::api::categories::EFFECT,
					path: Arc::from(self.key.path.as_path()),
				}))
			}
			PluginFormat::Vst3 => {
				let id = parse_class_id(&self.key.id)?;

				Some(PluginDescriptor::Vst3(vst3_host::PluginDescriptor {
					id,
					name: self.name.clone(),
					vendor: self.vendor.clone(),
					version: self.version.clone(),
					subcategories: self.features.clone().into(),
					path: Arc::from(self.key.path.as_path()),
				}))
			}
		}
	}
}

/// What we remember about the plugins on this computer.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(from = "PluginMemoryData")]
pub struct PluginMemory {
	/// Every plugin we've ever found.
	pub plugins: Vec<PluginEntry>,
	/// The plugins that were loaded last, the most recently used one first.
	pub recent: Vec<PluginKey>,
	/// The plugins the user marked as favorites.
	pub favorites: Vec<PluginKey>,
	/// Where every plugin is in `plugins`, so that we can find them fast.
	#[serde(skip)]
	index: HashMap<PluginKey, usize>,
}

/// The plugin memory as it's written to disk, without the index.
#[derive(Deserialize)]
struct PluginMemoryData {
	#[serde(default)]
	plugins: Vec<PluginEntry>,
	#[serde(default)]
	recent: Vec<PluginKey>,
	#[serde(default)]
	favorites: Vec<PluginKey>,
}

impl From<PluginMemoryData> for PluginMemory {
	fn from(value: PluginMemoryData) -> Self {
		let mut memory = Self {
			plugins: value.plugins,
			recent: value.recent,
			favorites: value.favorites,
			index: HashMap::new(),
		};

		memory.reindex();

		memory
	}
}

impl PluginMemory {
	/// Reads the memory from disk. There's nothing to remember the first time around.
	#[must_use]
	pub fn read() -> Self {
		let read = match read_to_string(&*PLUGIN_MEMORY_PATH) {
			Ok(read) => match toml::from_str(&read) {
				Ok(read) => read,
				Err(err) => {
					warn!("{err}");
					Self::default()
				}
			},
			Err(err) if err.kind() == io::ErrorKind::NotFound => Self::default(),
			Err(err) => {
				warn!("{err}");
				Self::default()
			}
		};

		info!(
			"loaded plugin memory with {} plugins",
			read.plugins.len()
		);

		read
	}

	/// Writes the memory to disk.
	pub fn write(&self) {
		let Ok(contents) = toml::to_string(self) else {
			warn!("couldn't serialize the plugin memory");
			return;
		};

		if let Err(err) = write(&*PLUGIN_MEMORY_PATH, contents) {
			warn!("{err}");
		}
	}

	/// Records that a scan found a plugin.
	pub fn record_scan(&mut self, descriptor: &PluginDescriptor) {
		let key = PluginKey::from_descriptor(descriptor);
		let now = now();

		if let Some(&i) = self.index.get(&key) {
			let entry = &mut self.plugins[i];
			entry.name = descriptor.name().into();
			entry.vendor = descriptor.vendor().map(Arc::from);
			entry.version = descriptor.version().map(Arc::from);
			entry.features = features_of(descriptor);
			entry.last_scanned = now;
			entry.missing = false;
		} else {
			let entry = PluginEntry {
				key: key.clone(),
				name: descriptor.name().into(),
				vendor: descriptor.vendor().map(Arc::from),
				version: descriptor.version().map(Arc::from),
				features: features_of(descriptor),
				last_scanned: now,
				last_used: None,
				use_count: 0,
				tags: Vec::new(),
				missing: false,
			};

			self.index.insert(key, self.plugins.len());
			self.plugins.push(entry);
		}
	}

	/// Records that a scan finished, marking the plugins that weren't found as missing.
	pub fn finish_scan(&mut self, found: &HashSet<PluginKey>) {
		for entry in &mut self.plugins {
			entry.missing = !found.contains(&entry.key);
		}

		let favorites = &self.favorites;

		self.plugins.retain(|entry| {
			// we remember the plugins that are missing, but only the ones the user
			// cares about
			!entry.missing || entry.use_count > 0 || favorites.contains(&entry.key)
		});

		self.reindex();

		self.recent.retain(|key| self.index.contains_key(key));
		self.favorites.retain(|key| self.index.contains_key(key));
	}

	/// Records that a plugin was loaded.
	pub fn mark_used(&mut self, key: &PluginKey) {
		let now = now();

		if let Some(&i) = self.index.get(key) {
			self.plugins[i].last_used = Some(now);
			self.plugins[i].use_count = self.plugins[i].use_count.saturating_add(1);
		}

		self.recent.retain(|recent| recent != key);
		self.recent.insert(0, key.clone());
		self.recent.truncate(MAX_RECENT);
	}

	/// Adds a plugin to the favorites, or removes it if it's already one. Tells whether
	/// the plugin is a favorite afterwards.
	pub fn toggle_favorite(&mut self, key: &PluginKey) -> bool {
		if let Some(i) = self.favorites.iter().position(|favorite| favorite == key) {
			self.favorites.remove(i);
			false
		} else {
			self.favorites.push(key.clone());
			true
		}
	}

	/// Whether the plugin is one of the user's favorites.
	#[must_use]
	pub fn is_favorite(&self, key: &PluginKey) -> bool {
		self.favorites.contains(key)
	}

	/// What we know about a plugin.
	#[must_use]
	pub fn entry(&self, key: &PluginKey) -> Option<&PluginEntry> {
		self.index.get(key).map(|&i| &self.plugins[i])
	}

	/// The user's favorites, in the order they were marked.
	pub fn favorite_entries(&self) -> impl Iterator<Item = &PluginEntry> {
		self.favorites.iter().filter_map(|key| self.entry(key))
	}

	/// The plugins that were loaded last, the most recently used one first.
	pub fn recent_entries(&self) -> impl Iterator<Item = &PluginEntry> {
		self.recent.iter().filter_map(|key| self.entry(key))
	}

	fn reindex(&mut self) {
		self.index.clear();
		self.index.reserve(self.plugins.len());

		for (i, entry) in self.plugins.iter().enumerate() {
			self.index.insert(entry.key.clone(), i);
		}
	}
}

/// The features of a plugin, as they're shown to the user.
fn features_of(descriptor: &PluginDescriptor) -> Vec<Arc<str>> {
	match descriptor {
		PluginDescriptor::Clap(descriptor) => descriptor
			.features
			.iter()
			.map(|feature| Arc::from(feature.to_string()))
			.collect(),
		// a VST2 plugin only says what kind of plugin it is
		PluginDescriptor::Vst2(descriptor) => vec![Arc::from(vst2_category(descriptor.category))],
		PluginDescriptor::Vst3(descriptor) => descriptor.subcategories.to_vec(),
	}
}

/// The name of the category of a VST2 plugin.
fn vst2_category(category: i32) -> &'static str {
	match category {
		vst2_host::api::categories::SYNTH => "Instrument",
		vst2_host::api::categories::ANALYSIS => "Analyzer",
		_ => "Audio Effect",
	}
}

/// The feature with a name, if there's one.
fn parse_feature(feature: &str) -> Option<Feature> {
	if let Some(category) = Category::VARIANTS
		.iter()
		.find(|category| category.to_string() == feature)
	{
		return Some(Feature::Category(*category));
	}

	if let Some(subcategory) = Subcategory::VARIANTS
		.iter()
		.find(|subcategory| subcategory.to_string() == feature)
	{
		return Some(Feature::Subcategory(*subcategory));
	}

	Capability::VARIANTS
		.iter()
		.find(|capability| capability.to_string() == feature)
		.map(|capability| Feature::Capability(*capability))
}

/// The class id of a VST3 plugin, from its hex digits.
fn parse_class_id(id: &str) -> Option<[u8; 16]> {
	if id.len() != 32 {
		return None;
	}

	let mut class_id = [0; 16];

	for (i, byte) in class_id.iter_mut().enumerate() {
		*byte = u8::from_str_radix(id.get(i * 2..i * 2 + 2)?, 16).ok()?;
	}

	Some(class_id)
}

/// The time now, in seconds since the unix epoch.
fn now() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|duration| duration.as_secs())
		.unwrap_or_default()
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{ffi::CStr, path::Path};

	fn clap(id: &str) -> PluginDescriptor {
		let mut id = id.as_bytes().to_vec();
		id.push(0);

		PluginDescriptor::Clap(clap_host::PluginDescriptor {
			id: Arc::from(CStr::from_bytes_with_nul(&id).unwrap()),
			name: "plugin".into(),
			vendor: Some("vendor".into()),
			version: None,
			features: Arc::from([Feature::Category(Category::AudioEffect)]),
			path: Arc::from(Path::new("plugin.clap")),
		})
	}

	fn vst3(id: [u8; 16]) -> PluginDescriptor {
		PluginDescriptor::Vst3(vst3_host::PluginDescriptor {
			id,
			name: "plugin".into(),
			vendor: None,
			version: Some("1.0".into()),
			subcategories: Arc::from([Arc::from("Fx"), Arc::from("Reverb")]),
			path: Arc::from(Path::new("plugin.vst3")),
		})
	}

	#[test]
	fn plugins_are_told_apart_by_their_key() {
		let one = clap("com.example.plugin");
		let other = clap("com.example.other");

		assert_eq!(
			PluginKey::from_descriptor(&one),
			PluginKey::from_descriptor(&one)
		);
		assert_ne!(
			PluginKey::from_descriptor(&one),
			PluginKey::from_descriptor(&other)
		);
	}

	#[test]
	fn scanned_plugins_are_remembered() {
		let mut memory = PluginMemory::default();
		let descriptor = clap("com.example.plugin");

		memory.record_scan(&descriptor);

		let key = PluginKey::from_descriptor(&descriptor);
		let entry = memory.entry(&key).unwrap();
		assert_eq!(&*entry.name, "plugin");
		assert_eq!(entry.vendor.as_deref(), Some("vendor"));
		assert!(!entry.missing);
		assert_eq!(entry.use_count, 0);
	}

	#[test]
	fn scanning_the_same_plugin_again_updates_it() {
		let mut memory = PluginMemory::default();
		let descriptor = clap("com.example.plugin");

		memory.record_scan(&descriptor);
		memory.record_scan(&descriptor);

		let key = PluginKey::from_descriptor(&descriptor);
		assert_eq!(memory.plugins.len(), 1);
		assert_eq!(memory.entry(&key).unwrap().use_count, 0);
	}

	#[test]
	fn plugins_that_were_not_found_are_missing() {
		let mut memory = PluginMemory::default();
		let found = clap("com.example.plugin");
		let missing = clap("com.example.missing");

		memory.record_scan(&found);
		memory.record_scan(&missing);
		memory.mark_used(&PluginKey::from_descriptor(&missing));

		let mut index = HashSet::new();
		index.insert(PluginKey::from_descriptor(&found));

		memory.finish_scan(&index);

		assert!(!memory.entry(&PluginKey::from_descriptor(&found)).unwrap().missing);
		assert!(memory.entry(&PluginKey::from_descriptor(&missing)).unwrap().missing);
	}

	#[test]
	fn plugins_that_the_user_cares_about_are_remembered() {
		let mut memory = PluginMemory::default();
		let missing = clap("com.example.missing");
		let key = PluginKey::from_descriptor(&missing);

		memory.record_scan(&missing);
		memory.mark_used(&key);

		memory.finish_scan(&HashSet::new());

		assert!(memory.entry(&key).is_some());
		assert!(memory.entry(&key).unwrap().missing);
	}

	#[test]
	fn plugins_that_nobody_cares_about_are_forgotten() {
		let mut memory = PluginMemory::default();
		let missing = clap("com.example.missing");
		let key = PluginKey::from_descriptor(&missing);

		memory.record_scan(&missing);

		memory.finish_scan(&HashSet::new());

		assert!(memory.entry(&key).is_none());
	}

	#[test]
	fn used_plugins_are_the_ones_used_last() {
		let mut memory = PluginMemory::default();
		let one = clap("com.example.one");
		let other = clap("com.example.two");

		memory.record_scan(&one);
		memory.record_scan(&other);

		let one = PluginKey::from_descriptor(&one);
		let other = PluginKey::from_descriptor(&other);

		memory.mark_used(&one);
		memory.mark_used(&other);
		memory.mark_used(&one);

		assert_eq!(memory.recent, vec![one.clone(), other.clone()]);
		assert_eq!(memory.entry(&one).unwrap().use_count, 2);
		assert_eq!(memory.entry(&other).unwrap().use_count, 1);
		assert!(memory.entry(&one).unwrap().last_used.is_some());
	}

	#[test]
	fn only_the_last_plugins_used_are_remembered() {
		let mut memory = PluginMemory::default();

		for i in 0..MAX_RECENT + 10 {
			let descriptor = clap(&format!("com.example.plugin{i}"));
			memory.record_scan(&descriptor);
			memory.mark_used(&PluginKey::from_descriptor(&descriptor));
		}

		assert_eq!(memory.recent.len(), MAX_RECENT);
	}

	#[test]
	fn favorites_can_be_added_and_removed() {
		let mut memory = PluginMemory::default();
		let descriptor = clap("com.example.plugin");
		let key = PluginKey::from_descriptor(&descriptor);

		assert!(!memory.is_favorite(&key));
		assert!(memory.toggle_favorite(&key));
		assert!(memory.is_favorite(&key));
		assert!(!memory.toggle_favorite(&key));
		assert!(!memory.is_favorite(&key));
	}

	#[test]
	fn plugins_can_be_restored() {
		let mut memory = PluginMemory::default();
		let descriptor = vst3([1; 16]);
		memory.record_scan(&descriptor);

		let key = PluginKey::from_descriptor(&descriptor);
		let restored = memory.entry(&key).unwrap().descriptor().unwrap();

		match restored {
			PluginDescriptor::Vst3(restored) => {
				assert_eq!(restored.id, [1; 16]);
				assert_eq!(restored.subcategories.len(), 2);
				assert_eq!(&*restored.subcategories[0], "Fx");
			}
			PluginDescriptor::Clap(_) | PluginDescriptor::Vst2(_) => {
				panic!("the plugin changed format")
			}
		}
	}

	#[test]
	fn the_memory_survives_a_round_trip() {
		let mut memory = PluginMemory::default();
		let descriptor = clap("com.example.plugin");
		memory.record_scan(&descriptor);

		let key = PluginKey::from_descriptor(&descriptor);
		memory.mark_used(&key);
		memory.toggle_favorite(&key);

		let write = toml::to_string(&memory).unwrap();
		let read: PluginMemory = toml::from_str(&write).unwrap();

		assert_eq!(read.plugins.len(), 1);
		assert!(read.is_favorite(&key));
		assert_eq!(read.recent, vec![key.clone()]);
		assert_eq!(read.entry(&key).unwrap().use_count, 1);
		assert_eq!(read.entry(&key).unwrap().descriptor().unwrap().name(), "plugin");
	}
}
