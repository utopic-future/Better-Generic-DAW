use prost::{Enumeration, Message, Oneof};
use std::ffi::CStr;

#[derive(Clone, Copy, Eq, Hash, Message, PartialEq)]
pub struct SampleIndex {
	#[prost(uint32)]
	pub(crate) index: u32,
}

#[derive(Clone, Copy, Eq, Hash, Message, PartialEq)]
pub struct MidiPatternIndex {
	#[prost(uint32)]
	pub(crate) index: u32,
}

#[derive(Clone, Copy, Eq, Hash, Message, PartialEq)]
pub struct TrackIndex {
	#[prost(uint32)]
	pub(crate) index: u32,
}

#[derive(Clone, Copy, Eq, Hash, Message, PartialEq)]
pub struct ChannelIndex {
	#[prost(uint32)]
	pub(crate) index: u32,
}

#[derive(Clone, Message)]
pub struct Project {
	#[prost(message, required)]
	pub transport: Transport,
	#[prost(message, repeated)]
	pub samples: Vec<Sample>,
	#[prost(message, repeated)]
	pub midi_patterns: Vec<MidiPattern>,
	#[prost(message, repeated)]
	pub tracks: Vec<Track>,
	#[prost(message, repeated)]
	pub channels: Vec<Channel>,
	#[prost(message, optional)]
	pub view: Option<ViewState>,
	#[prost(message, optional)]
	pub solo: Option<TrackIndex>,
}

#[derive(Clone, Copy, Message)]
pub struct Transport {
	#[prost(uint32)]
	pub bpm: u32,
	#[prost(uint32)]
	pub numerator: u32,
	#[prost(message, optional)]
	pub loop_range: Option<BeatRange>,
}

#[derive(Clone, Message)]
pub struct Sample {
	#[prost(string)]
	pub name: String,
	#[prost(uint32)]
	pub crc: u32,
	#[prost(uint64)]
	pub len: u64,
}

#[derive(Clone, Message)]
pub struct MidiPattern {
	#[prost(message, repeated)]
	pub notes: Vec<Note>,
	#[prost(string)]
	pub name: String,
}

#[derive(Clone, Copy, Message)]
pub struct Note {
	#[prost(uint32)]
	pub key: u32,
	#[prost(float, default = 1.0)]
	pub velocity: f32,
	#[prost(message, required)]
	pub position: BeatRange,
}

#[derive(Clone, Copy, Message)]
pub struct Channels {
	#[prost(uint32)]
	pub left: u32,
	#[prost(uint32)]
	pub right: u32,
	#[prost(uint32)]
	pub midi: u32,
	#[prost(bool, default = false)]
	pub enable_audio: bool,
	#[prost(bool, default = false)]
	pub enable_midi: bool,
}

#[derive(Clone, Message)]
pub struct Track {
	#[prost(message, repeated)]
	pub clips: Vec<OptionClip>,
	#[prost(message, required)]
	pub channel: Channel,
	#[prost(message, optional)]
	pub input: Option<Channels>,
}

#[derive(Clone, Message)]
pub struct Channel {
	#[prost(message, repeated)]
	pub connections: Vec<Connection>,
	#[prost(message, repeated)]
	pub plugins: Vec<Plugin>,
	#[prost(float, default = 1.0)]
	pub volume: f32,
	#[prost(message, required)]
	pub pan: OptionPanMode,
	#[prost(bool, default = true)]
	pub enabled: bool,
	#[prost(bool, default = false)]
	pub bypassed: bool,
	#[prost(message, optional)]
	pub output: Option<Channels>,
	#[prost(string)]
	pub name: String,
}

#[derive(Clone, Copy, Message, PartialEq)]
pub struct Connection {
	#[prost(uint32)]
	pub index: u32,
	#[prost(float, default = 1.0)]
	pub mix: f32,
}

#[derive(Clone, Message)]
pub struct Plugin {
	#[prost(bytes = "vec")]
	pub id: Vec<u8>,
	#[prost(bytes = "vec", optional)]
	pub state: Option<Vec<u8>>,
	#[prost(float, default = 1.0)]
	pub mix: f32,
	#[prost(bool, default = true)]
	pub active: bool,
	/// Added later, files without it are CLAP plugins.
	#[prost(enumeration = "PluginFormat", default = "Clap")]
	pub format: i32,
}

/// The plugin format, and with it what [`Plugin::id`] means.
#[derive(Clone, Copy, Debug, Enumeration, Eq, Hash, PartialEq)]
#[repr(i32)]
pub enum PluginFormat {
	/// `id` is the plugin's CLAP id.
	Clap = 0,
	/// `id` is the 16 byte class id as 32 hex digits, see [`Plugin::vst3`].
	Vst3 = 1,
	/// `id` is the plugin's unique id as a number, see [`Plugin::vst2`].
	Vst2 = 2,
}

#[derive(Clone, Copy, Message)]
pub struct OptionPanMode {
	#[prost(oneof = "PanMode", tags = "1, 2")]
	pub pan_mode: Option<PanMode>,
}

#[derive(Clone, Copy, Oneof, PartialEq)]
pub enum PanMode {
	#[prost(message, tag = "1")]
	Stereo(PanModeStereo),
	#[prost(message, tag = "2")]
	SplitStereo(PanModeSplitStereo),
}

#[derive(Clone, Copy, Message, PartialEq)]
pub struct PanModeStereo {
	#[prost(float, default = 0.0)]
	pub pan: f32,
}

#[derive(Clone, Copy, Message, PartialEq)]
pub struct PanModeSplitStereo {
	#[prost(float, default = -1.0)]
	pub l: f32,
	#[prost(float, default = 1.0)]
	pub r: f32,
}

#[derive(Clone, Copy, Message)]
pub struct OptionClip {
	#[prost(oneof = "Clip", tags = "1, 2")]
	pub clip: Option<Clip>,
}

#[derive(Clone, Copy, Oneof)]
pub enum Clip {
	#[prost(message, tag = "1")]
	Audio(AudioClip),
	#[prost(message, tag = "2")]
	Midi(MidiClip),
}

#[derive(Clone, Copy, Message)]
pub struct AudioClip {
	#[prost(message, required)]
	pub sample: SampleIndex,
	#[prost(message)]
	pub position_compat: Option<OffsetBeatRange>,
	#[prost(float, optional)]
	pub stretch_compat: Option<f32>,
	#[prost(message, required)]
	pub position: OffsetBeatSpan,
	#[prost(double, default = 1.0)]
	pub stretch: f64,
	#[prost(message, required)]
	pub fade_start: Transition,
	#[prost(message, required)]
	pub fade_end: Transition,
	#[prost(float, default = 1.0)]
	pub volume: f32,
}

#[derive(Clone, Copy, Message)]
pub struct MidiClip {
	#[prost(message, required)]
	pub pattern: MidiPatternIndex,
	#[prost(message, required)]
	pub position: OffsetBeatRange,
}

#[derive(Clone, Copy, Message)]
pub struct BeatRange {
	#[prost(uint64)]
	pub start: u64,
	#[prost(uint64)]
	pub end: u64,
}

#[derive(Clone, Copy, Message)]
pub struct OffsetBeatRange {
	#[prost(message, required)]
	pub position: BeatRange,
	#[prost(uint64)]
	pub offset: u64,
}

#[derive(Clone, Copy, Message)]
pub struct BeatSpan {
	#[prost(uint64)]
	pub start: u64,
	#[prost(uint64)]
	pub len: u64,
}

#[derive(Clone, Copy, Message)]
pub struct OffsetBeatSpan {
	#[prost(message, required)]
	pub position: BeatSpan,
	#[prost(uint64)]
	pub offset: u64,
}

#[derive(Clone, Copy, Message)]
pub struct Transition {
	#[prost(message, required)]
	pub len: u64,
	#[prost(message, required)]
	pub p: Point,
	#[prost(bool, default = false)]
	pub symmetric: bool,
}

#[derive(Clone, Copy, Message)]
pub struct ViewState {
	#[prost(message, required)]
	pub playlist: TabState,
	#[prost(message, required)]
	pub piano_roll: TabState,
}

#[derive(Clone, Copy, Message)]
pub struct TabState {
	#[prost(message, required)]
	pub position: Vector,
	#[prost(message, required)]
	pub scale: Vector,
}

#[derive(Clone, Copy, Message)]
pub struct Point {
	#[prost(float, default = 0.5)]
	pub x: f32,
	#[prost(float, default = 0.5)]
	pub y: f32,
}

#[derive(Clone, Copy, Message)]
pub struct Vector {
	#[prost(float)]
	pub x: f32,
	#[prost(float)]
	pub y: f32,
}

impl Plugin {
	/// A CLAP plugin.
	#[must_use]
	pub fn clap(id: &CStr, state: Option<Vec<u8>>, mix: f32, active: bool) -> Self {
		Self {
			id: id.to_bytes_with_nul().to_vec(),
			state,
			mix,
			active,
			format: PluginFormat::Clap as i32,
		}
	}

	/// A VST3 plugin, from its class id.
	///
	/// The id is stored as hex digits in a C string, which is what versions that don't know about
	/// VST3 expect to find. They don't find such a plugin, and say so, instead of failing.
	#[must_use]
	pub fn vst3(class_id: [u8; 16], state: Option<Vec<u8>>, mix: f32, active: bool) -> Self {
		let mut id = class_id
			.iter()
			.flat_map(|byte| {
				[
					b"0123456789ABCDEF"[usize::from(byte >> 4)],
					b"0123456789ABCDEF"[usize::from(byte & 0xf)],
				]
			})
			.collect::<Vec<_>>();
		id.push(0);

		Self {
			id,
			state,
			mix,
			active,
			format: PluginFormat::Vst3 as i32,
		}
	}

	/// A VST2 plugin, from its unique id.
	///
	/// The id is stored as a number in a C string, which is what versions that don't know
	/// about VST2 expect to find. They don't find such a plugin, and say so, instead of
	/// failing.
	#[must_use]
	pub fn vst2(unique_id: i32, state: Option<Vec<u8>>, mix: f32, active: bool) -> Self {
		let mut id = unique_id.to_string().into_bytes();
		id.push(0);

		Self {
			id,
			state,
			mix,
			active,
			format: PluginFormat::Vst2 as i32,
		}
	}

	/// The id as a C string, which for VST3 plugins is the class id in hex.
	#[must_use]
	pub fn id(&self) -> &CStr {
		CStr::from_bytes_with_nul(&self.id).unwrap()
	}

	/// The VST3 class id, if this is a VST3 plugin with a valid one.
	#[must_use]
	pub fn vst3_id(&self) -> Option<[u8; 16]> {
		if self.format() != PluginFormat::Vst3 {
			return None;
		}

		let digits = self.id.strip_suffix(&[0])?;
		if digits.len() != 32 {
			return None;
		}

		let mut class_id = [0; 16];
		for (byte, pair) in class_id.iter_mut().zip(digits.chunks_exact(2)) {
			let digit = |c: u8| char::from(c).to_digit(16);
			*byte = u8::try_from(digit(pair[0])? << 4 | digit(pair[1])?).ok()?;
		}

		Some(class_id)
	}

	/// The VST2 unique id, if this is a VST2 plugin with a valid one.
	#[must_use]
	pub fn vst2_id(&self) -> Option<i32> {
		if self.format() != PluginFormat::Vst2 {
			return None;
		}

		let digits = self.id.strip_suffix(&[0])?;
		let text = std::str::from_utf8(digits).ok()?;

		text.parse().ok()
	}
}

impl From<PanModeStereo> for OptionPanMode {
	fn from(value: PanModeStereo) -> Self {
		Self {
			pan_mode: Some(PanMode::Stereo(value)),
		}
	}
}

impl From<PanModeSplitStereo> for OptionPanMode {
	fn from(value: PanModeSplitStereo) -> Self {
		Self {
			pan_mode: Some(PanMode::SplitStereo(value)),
		}
	}
}

impl From<AudioClip> for OptionClip {
	fn from(value: AudioClip) -> Self {
		Self {
			clip: Some(Clip::Audio(value)),
		}
	}
}

impl From<MidiClip> for OptionClip {
	fn from(value: MidiClip) -> Self {
		Self {
			clip: Some(Clip::Midi(value)),
		}
	}
}
