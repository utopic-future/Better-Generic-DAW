use generic_daw_core::{
	clap_host::{self, Category, Feature, Subcategory},
	vst2_host,
	vst3_host,
};
use std::path::Path;
use std::sync::Arc;

/// A plugin that can be loaded, of any format.
#[derive(Clone, Debug)]
pub enum PluginDescriptor {
	Clap(clap_host::PluginDescriptor),
	Vst2(vst2_host::PluginDescriptor),
	Vst3(vst3_host::PluginDescriptor),
}

impl PluginDescriptor {
	#[must_use]
	pub fn name(&self) -> &str {
		match self {
			Self::Clap(descriptor) => &descriptor.name,
			Self::Vst2(descriptor) => &descriptor.name,
			Self::Vst3(descriptor) => &descriptor.name,
		}
	}

	#[must_use]
	pub fn vendor(&self) -> Option<&str> {
		match self {
			Self::Clap(descriptor) => descriptor.vendor.as_deref(),
			Self::Vst2(descriptor) => descriptor.vendor.as_deref(),
			Self::Vst3(descriptor) => descriptor.vendor.as_deref(),
		}
	}

	#[must_use]
	pub fn version(&self) -> Option<&str> {
		match self {
			Self::Clap(descriptor) => descriptor.version.as_deref(),
			Self::Vst2(descriptor) => descriptor.version.as_deref(),
			Self::Vst3(descriptor) => descriptor.version.as_deref(),
		}
	}

	/// The name of the plugin format.
	#[must_use]
	pub const fn format(&self) -> &'static str {
		match self {
			Self::Clap(_) => "CLAP",
			Self::Vst2(_) => "VST2",
			Self::Vst3(_) => "VST3",
		}
	}

	/// Plugins of different formats with the same name are different plugins, and CLAP comes
	/// first.
	#[must_use]
	pub const fn format_rank(&self) -> u8 {
		match self {
			Self::Clap(_) => 0,
			Self::Vst2(_) => 1,
			Self::Vst3(_) => 2,
		}
	}

	/// The path to the plugin file.
	#[must_use]
	pub fn path(&self) -> &Arc<Path> {
		match self {
			Self::Clap(descriptor) => &descriptor.path,
			Self::Vst2(descriptor) => &descriptor.path,
			Self::Vst3(descriptor) => &descriptor.path,
		}
	}

	#[must_use]
	pub fn in_category(&self, category: Category) -> bool {
		match self {
			Self::Clap(descriptor) => descriptor.features.contains(&Feature::Category(category)),
			Self::Vst2(descriptor) => match category {
				Category::Instrument => descriptor.is_instrument(),
				Category::Analyzer => descriptor.is_analyzer(),
				// plugins that don't say are taken to be effects
				Category::AudioEffect => !descriptor.is_instrument() && !descriptor.is_analyzer(),
				Category::NoteEffect | Category::NoteDetector => false,
			},
			Self::Vst3(descriptor) => {
				let has = |word: &str| descriptor.subcategories.iter().any(|s| &**s == word);

				match category {
					Category::Instrument => has("Instrument"),
					Category::Analyzer => has("Analyzer"),
					// plugins that don't say are taken to be effects
					Category::AudioEffect => has("Fx") || !has("Instrument") && !has("Analyzer"),
					Category::NoteEffect | Category::NoteDetector => false,
				}
			}
		}
	}

	#[must_use]
	pub fn in_subcategory(&self, subcategory: Subcategory) -> bool {
		match self {
			Self::Clap(descriptor) => descriptor
				.features
				.contains(&Feature::Subcategory(subcategory)),
			// VST2 plugins don't say what kind of effect they are
			Self::Vst2(_) => false,
			Self::Vst3(descriptor) => {
				let words = vst3_words(subcategory);
				descriptor
					.subcategories
					.iter()
					.any(|s| words.contains(&&**s))
			}
		}
	}
}

/// The VST3 subcategories that mean a CLAP subcategory. VST3 has fewer of them.
const fn vst3_words(subcategory: Subcategory) -> &'static [&'static str] {
	match subcategory {
		Subcategory::Synthesizer => &["Synth"],
		Subcategory::Sampler => &["Sampler"],
		Subcategory::DrumMachine => &["Drum"],
		Subcategory::Filter => &["Filter"],
		Subcategory::Equalizer => &["EQ"],
		Subcategory::PitchShifter => &["PitchShift"],
		Subcategory::Distortion => &["Distortion"],
		Subcategory::Compressor
		| Subcategory::Expander
		| Subcategory::Gate
		| Subcategory::Limiter => &["Dynamics"],
		Subcategory::Phaser | Subcategory::Flanger | Subcategory::Chorus | Subcategory::Tremolo => {
			&["Modulation"]
		}
		Subcategory::Delay => &["Delay"],
		Subcategory::Reverb => &["Reverb"],
		Subcategory::Utility => &["Tools"],
		Subcategory::Restoration => &["Restoration"],
		Subcategory::Mastering => &["Mastering"],
		_ => &[],
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{path::Path, sync::Arc};

	fn vst3(subcategories: &[&str]) -> PluginDescriptor {
		let path: Arc<Path> = Path::new("plugin.vst3").into();
		let joined = subcategories.join("|");

		PluginDescriptor::Vst3(vst3_host::PluginDescriptor {
			id: [0; 16],
			name: "plugin".into(),
			vendor: Some("vendor".into()),
			version: None,
			subcategories: joined
				.split('|')
				.filter(|s| !s.is_empty())
				.map(Arc::from)
				.collect(),
			path,
		})
	}

	#[test]
	fn vst3_effects_and_instruments() {
		let reverb = vst3(&["Fx", "Reverb"]);
		assert!(reverb.in_category(Category::AudioEffect));
		assert!(!reverb.in_category(Category::Instrument));
		assert!(reverb.in_subcategory(Subcategory::Reverb));
		assert!(!reverb.in_subcategory(Subcategory::Delay));

		let synth = vst3(&["Instrument", "Synth"]);
		assert!(synth.in_category(Category::Instrument));
		assert!(!synth.in_category(Category::AudioEffect));
		assert!(synth.in_subcategory(Subcategory::Synthesizer));

		let analyzer = vst3(&["Fx", "Analyzer"]);
		assert!(analyzer.in_category(Category::Analyzer));
	}

	#[test]
	fn vst3_plugins_that_dont_say_are_effects() {
		let unknown = vst3(&[]);
		assert!(unknown.in_category(Category::AudioEffect));
		assert!(!unknown.in_category(Category::Instrument));
		assert!(!unknown.in_category(Category::NoteEffect));
	}

	#[test]
	fn vst3_dynamics_cover_the_clap_dynamics() {
		let dynamics = vst3(&["Fx", "Dynamics"]);

		for subcategory in [
			Subcategory::Compressor,
			Subcategory::Expander,
			Subcategory::Gate,
			Subcategory::Limiter,
		] {
			assert!(dynamics.in_subcategory(subcategory), "{subcategory}");
		}
	}

	#[test]
	fn the_format_is_told() {
		assert_eq!(vst3(&[]).format(), "VST3");
		assert!(vst3(&[]).format_rank() > 0);
		assert_eq!(vst3(&[]).name(), "plugin");
		assert_eq!(vst3(&[]).vendor(), Some("vendor"));
		assert_eq!(vst3(&[]).version(), None);
	}
}