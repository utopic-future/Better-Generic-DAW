use generic_daw_project::{Reader, Writer, proto};
use prost::Message;
use std::ffi::CStr;

fn project(plugins: impl IntoIterator<Item = proto::Plugin>) -> Vec<u8> {
	let mut writer = Writer::new(proto::Transport {
		bpm: 140,
		numerator: 4,
		loop_range: None,
	});

	let _ = writer.push_channel(
		"channel",
		plugins,
		1.0,
		proto::OptionPanMode::from(proto::PanModeStereo { pan: 0.0 }),
		true,
		false,
		None,
	);

	writer.finalize()
}

fn plugins_of(gdp: &[u8]) -> Vec<proto::Plugin> {
	let reader = Reader::new(gdp).expect("the project should load");
	let (_, channel) = reader.iter_channels().next().unwrap();
	channel.plugins.clone()
}

const CLASS_ID: [u8; 16] = [
	0x00, 0x01, 0x02, 0xAB, 0xCD, 0xEF, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xFF,
];

#[test]
fn vst3_plugins_round_trip() {
	let state = vec![1, 2, 3, 4, 5];
	let gdp = project([proto::Plugin::vst3(
		CLASS_ID,
		Some(state.clone()),
		0.5,
		false,
	)]);

	let plugins = plugins_of(&gdp);
	assert_eq!(plugins.len(), 1);

	let plugin = &plugins[0];
	assert_eq!(plugin.format(), proto::PluginFormat::Vst3);
	assert_eq!(plugin.vst3_id(), Some(CLASS_ID));
	assert_eq!(plugin.state.as_deref(), Some(&state[..]));
	assert_eq!(plugin.mix, 0.5);
	assert!(!plugin.active);
}

#[test]
fn clap_plugins_round_trip() {
	let id = CStr::from_bytes_with_nul(b"com.example.synth\0").unwrap();
	let gdp = project([proto::Plugin::clap(id, None, 1.0, true)]);

	let plugins = plugins_of(&gdp);
	let plugin = &plugins[0];

	assert_eq!(plugin.format(), proto::PluginFormat::Clap);
	assert_eq!(plugin.id(), id);
	assert_eq!(plugin.vst3_id(), None);
	assert_eq!(plugin.state, None);
	assert!(plugin.active);
}

#[test]
fn formats_can_be_mixed_and_keep_their_order() {
	let clap = CStr::from_bytes_with_nul(b"a.b.c\0").unwrap();
	let gdp = project([
		proto::Plugin::vst3([1; 16], None, 1.0, true),
		proto::Plugin::clap(clap, None, 1.0, true),
		proto::Plugin::vst3([2; 16], None, 1.0, true),
	]);

	let plugins = plugins_of(&gdp);
	let formats = plugins
		.iter()
		.map(proto::Plugin::format)
		.collect::<Vec<_>>();
	assert_eq!(
		formats,
		[
			proto::PluginFormat::Vst3,
			proto::PluginFormat::Clap,
			proto::PluginFormat::Vst3
		]
	);
	assert_eq!(plugins[0].vst3_id(), Some([1; 16]));
	assert_eq!(plugins[2].vst3_id(), Some([2; 16]));
}

/// What a plugin looked like before it had a format.
#[derive(Clone, Message)]
struct OldPlugin {
	#[prost(bytes = "vec")]
	id: Vec<u8>,
	#[prost(bytes = "vec", optional)]
	state: Option<Vec<u8>>,
	#[prost(float, default = 1.0)]
	mix: f32,
	#[prost(bool, default = true)]
	active: bool,
}

#[test]
fn plugins_from_before_vst3_are_clap_plugins() {
	let old = OldPlugin {
		id: b"com.example.old\0".to_vec(),
		state: Some(vec![9, 9]),
		mix: 0.25,
		active: true,
	};

	let plugin = proto::Plugin::decode(old.encode_to_vec().as_slice()).unwrap();

	assert_eq!(plugin.format(), proto::PluginFormat::Clap);
	assert_eq!(plugin.id().to_bytes(), b"com.example.old");
	assert_eq!(plugin.vst3_id(), None);
	assert_eq!(plugin.state.as_deref(), Some(&[9, 9][..]));
	assert_eq!(plugin.mix, 0.25);
}

#[test]
fn versions_without_vst3_read_the_new_files_without_trouble() {
	// a VST3 plugin, written now, read by code that only knows the old message
	let new = proto::Plugin::vst3(CLASS_ID, Some(vec![7]), 0.75, true);
	let old = OldPlugin::decode(new.encode_to_vec().as_slice()).unwrap();

	assert_eq!(old.state, Some(vec![7]));
	assert_eq!(old.mix, 0.75);

	// the id is a proper C string, which is all that such code does with it
	let id = CStr::from_bytes_with_nul(&old.id).expect("old code unwraps this");
	assert_eq!(id.to_bytes().len(), 32);
	assert!(id.to_bytes().iter().all(u8::is_ascii_hexdigit));
}

#[test]
fn damaged_vst3_ids_are_refused() {
	let valid = proto::Plugin::vst3(CLASS_ID, None, 1.0, true);

	for (name, id) in [
		("too short", b"0001\0".to_vec()),
		("too long", [b"0".repeat(34), b"\0".to_vec()].concat()),
		("not hex", [b"G".repeat(32), b"\0".to_vec()].concat()),
		("no terminator", b"0".repeat(32)),
		("empty", Vec::new()),
	] {
		let plugin = proto::Plugin {
			id,
			..valid.clone()
		};
		assert_eq!(plugin.vst3_id(), None, "{name}");
	}

	// the same id is no VST3 id if the plugin isn't a VST3 plugin
	let clap = proto::Plugin {
		format: proto::PluginFormat::Clap as i32,
		..valid.clone()
	};
	assert_eq!(clap.vst3_id(), None);

	// lower case digits are fine
	let lower = proto::Plugin {
		id: valid.id.to_ascii_lowercase(),
		..valid
	};
	assert_eq!(lower.vst3_id(), Some(CLASS_ID));
}

#[test]
fn unknown_formats_are_not_vst3() {
	let plugin = proto::Plugin {
		format: 77,
		..proto::Plugin::vst3(CLASS_ID, None, 1.0, true)
	};

	// prost reads an unknown value as the default, which is CLAP
	assert_eq!(plugin.format(), proto::PluginFormat::Clap);
	assert_eq!(plugin.vst3_id(), None);
}
