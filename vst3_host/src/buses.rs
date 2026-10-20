use crate::Error;
use std::ptr::null_mut;
use vst3::{
	ComPtr,
	Steinberg::{
		kResultOk,
		Vst::{
			BusDirection, BusDirections_, BusInfo, BusTypes_, IAudioProcessor,
			IAudioProcessorTrait as _, IComponent, IComponentTrait as _, MediaType, MediaTypes_,
			SpeakerArr, SpeakerArrangement,
		},
	},
};

const AUDIO: MediaType = MediaTypes_::kAudio as MediaType;
const EVENT: MediaType = MediaTypes_::kEvent as MediaType;
const INPUT: BusDirection = BusDirections_::kInput as BusDirection;
const OUTPUT: BusDirection = BusDirections_::kOutput as BusDirection;

/// How a plugin's buses are used by the host.
///
/// Only the main audio buses are used, as mono or stereo. Every other audio bus (e.g. a sidechain
/// input) is deactivated, but still needs an (empty) entry in the process data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioLayout {
	/// The number of audio input buses, active or not.
	pub input_buses: usize,
	/// The number of audio output buses, active or not.
	pub output_buses: usize,
	/// The index and channel count of the main audio input bus, if there is one.
	pub main_input: Option<(usize, usize)>,
	/// The index and channel count of the main audio output bus, if there is one.
	pub main_output: Option<(usize, usize)>,
	pub event_inputs: usize,
	pub event_outputs: usize,
}

impl AudioLayout {
	/// The number of channels the host feeds to the plugin, `0` for instruments.
	#[must_use]
	pub fn input_channels(&self) -> usize {
		self.main_input.map_or(0, |(_, channels)| channels)
	}

	/// The number of channels the plugin writes, `0` for plugins that only handle events.
	#[must_use]
	pub fn output_channels(&self) -> usize {
		self.main_output.map_or(0, |(_, channels)| channels)
	}
}

fn bus_count(component: &ComPtr<IComponent>, media: MediaType, dir: BusDirection) -> usize {
	// SAFETY:
	// `component` is a valid, initialized component.
	let count = unsafe { component.getBusCount(media, dir) };
	usize::try_from(count).unwrap_or(0)
}

fn bus_info(
	component: &ComPtr<IComponent>,
	media: MediaType,
	dir: BusDirection,
	index: usize,
) -> Option<BusInfo> {
	// SAFETY:
	// `BusInfo` is plain old data, all zeroes is a valid value.
	let mut info = unsafe { std::mem::zeroed::<BusInfo>() };

	// SAFETY:
	// `info` is a valid out pointer.
	let result = unsafe { component.getBusInfo(media, dir, index as i32, &raw mut info) };

	(result == kResultOk).then_some(info)
}

/// The index of the first main bus, or of the first bus if none is marked as main.
fn main_bus(component: &ComPtr<IComponent>, dir: BusDirection, count: usize) -> Option<usize> {
	(0..count).find(|&index| {
		bus_info(component, AUDIO, dir, index)
			.is_some_and(|info| info.busType == BusTypes_::kMain as i32)
	})
}

fn arrangement(
	component: &ComPtr<IComponent>,
	processor: &ComPtr<IAudioProcessor>,
	dir: BusDirection,
	index: usize,
) -> SpeakerArrangement {
	let mut arrangement = 0;

	// SAFETY:
	// `arrangement` is a valid out pointer.
	let result = unsafe { processor.getBusArrangement(dir, index as i32, &raw mut arrangement) };

	if result == kResultOk {
		return arrangement;
	}

	// the plugin doesn't say, so go by the channel count it reports
	match bus_info(component, AUDIO, dir, index).map(|info| info.channelCount) {
		Some(1) => SpeakerArr::kMono,
		Some(2) => SpeakerArr::kStereo,
		_ => 0,
	}
}

const fn channels(arrangement: SpeakerArrangement) -> Option<usize> {
	match arrangement {
		SpeakerArr::kMono => Some(1),
		SpeakerArr::kStereo => Some(2),
		_ => None,
	}
}

/// Asks the plugin for stereo main buses, and works out what it actually agreed to. Then
/// activates the main audio buses and the event buses, and deactivates the other audio buses.
pub fn negotiate(
	component: &ComPtr<IComponent>,
	processor: &ComPtr<IAudioProcessor>,
) -> Result<AudioLayout, Error> {
	let input_buses = bus_count(component, AUDIO, INPUT);
	let output_buses = bus_count(component, AUDIO, OUTPUT);
	let main_input = main_bus(component, INPUT, input_buses);
	let main_output = main_bus(component, OUTPUT, output_buses);

	let mut inputs = (0..input_buses)
		.map(|index| arrangement(component, processor, INPUT, index))
		.collect::<Vec<_>>();
	let mut outputs = (0..output_buses)
		.map(|index| arrangement(component, processor, OUTPUT, index))
		.collect::<Vec<_>>();

	if let Some(index) = main_input {
		inputs[index] = SpeakerArr::kStereo;
	}
	if let Some(index) = main_output {
		outputs[index] = SpeakerArr::kStereo;
	}

	let inputs_ptr = if inputs.is_empty() { null_mut() } else { inputs.as_mut_ptr() };
	let outputs_ptr = if outputs.is_empty() { null_mut() } else { outputs.as_mut_ptr() };

	// Whether the plugin accepts is less interesting than what it ends up with, a plugin that
	// refuses keeps its own arrangement, and we adapt to that.
	// SAFETY:
	// Both pointers are valid for the given counts, or null for a count of zero.
	let _ = unsafe {
		processor.setBusArrangements(inputs_ptr, input_buses as i32, outputs_ptr, output_buses as i32)
	};

	let main_input = main_input
		.map(|index| {
			channels(arrangement(component, processor, INPUT, index))
				.map(|channels| (index, channels))
				.ok_or(Error::UnsupportedBusLayout)
		})
		.transpose()?;
	let main_output = main_output
		.map(|index| {
			channels(arrangement(component, processor, OUTPUT, index))
				.map(|channels| (index, channels))
				.ok_or(Error::UnsupportedBusLayout)
		})
		.transpose()?;

	for index in 0..input_buses {
		let active = main_input.is_some_and(|(main, _)| main == index);

		// SAFETY:
		// `component` is a valid, initialized component.
		unsafe { component.activateBus(AUDIO, INPUT, index as i32, active.into()); }
	}

	for index in 0..output_buses {
		let active = main_output.is_some_and(|(main, _)| main == index);

		// SAFETY:
		// `component` is a valid, initialized component.
		unsafe { component.activateBus(AUDIO, OUTPUT, index as i32, active.into()); }
	}

	let event_inputs = bus_count(component, EVENT, INPUT);
	let event_outputs = bus_count(component, EVENT, OUTPUT);

	for index in 0..event_inputs {
		// SAFETY:
		// `component` is a valid, initialized component.
		unsafe { component.activateBus(EVENT, INPUT, index as i32, 1); }
	}

	for index in 0..event_outputs {
		// SAFETY:
		// `component` is a valid, initialized component.
		unsafe { component.activateBus(EVENT, OUTPUT, index as i32, 1); }
	}

	Ok(AudioLayout {
		input_buses,
		output_buses,
		main_input,
		main_output,
		event_inputs,
		event_outputs,
	})
}
