use crate::vst2_host;
use clap_host::{
	ClapId, Cookie,
	events::{
		self, ClapEvent, Event as _, EventFlags, Match, Midi2Event, MidiEvent, NoteDialect,
		NoteOffEvent, NoteOnEvent, ParamValueEvent, Pckn, UnknownEvent, spaces::CoreEventSpace,
	},
};

#[derive(Clone, Copy, Debug)]
pub enum Event {
	On {
		time: u32,
		key: u8,
		velocity: f32,
		note_id: Match<u32>,
		flags: EventFlags,
	},
	Off {
		time: u32,
		key: u8,
		velocity: f32,
		note_id: Match<u32>,
		flags: EventFlags,
	},
	ParamValue {
		time: u32,
		param_id: ClapId,
		value: f32,
		flags: EventFlags,
	},
}

impl Event {
	/// The VST3 form of the event. VST3 has no flags and no note dialects.
	///
	/// Parameter values are passed on as they are, which means they have to be normalized
	/// (`0.0..=1.0`) for VST3 plugins.
	#[must_use]
	pub fn to_vst3(self) -> vst3_host::Event {
		let note_id = |note_id: Match<u32>| match note_id {
			Match::Specific(id) => i32::try_from(id).ok(),
			Match::All => None,
		};

		match self {
			Self::On {
				time,
				key,
				velocity,
				note_id: id,
				..
			} => vst3_host::Event::NoteOn {
				time,
				channel: 0,
				pitch: i16::from(key),
				velocity,
				note_id: note_id(id),
			},
			Self::Off {
				time,
				key,
				velocity,
				note_id: id,
				..
			} => vst3_host::Event::NoteOff {
				time,
				channel: 0,
				pitch: i16::from(key),
				velocity,
				note_id: note_id(id),
			},
			Self::ParamValue {
				time,
				param_id,
				value,
				..
			} => vst3_host::Event::ParamValue {
				time,
				id: param_id.get(),
				value: f64::from(value),
			},
		}
	}

	/// The VST2 form of the event.
	///
	/// VST2 has no channels, so the notes are on the first one, and its parameters
	/// change at the start of the block instead of at a time.
	#[must_use]
	pub fn to_vst2(self) -> vst2_host::Event {
		match self {
			Self::On {
				time,
				key,
				velocity,
				..
			} => vst2_host::Event::NoteOn {
				time,
				key,
				velocity,
			},
			Self::Off {
				time,
				key,
				velocity,
				..
			} => vst2_host::Event::NoteOff {
				time,
				key,
				velocity,
			},
			Self::ParamValue {
				time,
				param_id,
				value,
				..
			} => vst2_host::Event::ParamValue {
				time,
				id: param_id.get(),
				value,
			},
		}
	}

	/// The inverse of [`to_vst2`](Self::to_vst2), `None` for the one parameter id that
	/// CLAP reserves.
	#[must_use]
	pub fn from_vst2(event: vst2_host::Event) -> Option<Self> {
		match event {
			vst2_host::Event::NoteOn {
				time,
				key,
				velocity,
			} => Some(Self::On {
				time,
				key,
				velocity,
				note_id: Match::All,
				flags: EventFlags::empty(),
			}),
			vst2_host::Event::NoteOff {
				time,
				key,
				velocity,
			} => Some(Self::Off {
				time,
				key,
				velocity,
				note_id: Match::All,
				flags: EventFlags::empty(),
			}),
			vst2_host::Event::ParamValue { time, id, value } => Some(Self::ParamValue {
				time,
				param_id: ClapId::from_raw(id)?,
				value,
				flags: EventFlags::empty(),
			}),
		}
	}

	/// The inverse of [`to_vst3`](Self::to_vst3), `None` for events that can't be represented
	/// (notes outside of the MIDI range, and the one parameter id that CLAP reserves).
	#[must_use]
	pub fn from_vst3(event: vst3_host::Event) -> Option<Self> {		let note_id = |note_id: Option<i32>| {
			note_id
				.and_then(|id| u32::try_from(id).ok())
				.map_or(Match::All, Match::Specific)
		};

		match event {
			vst3_host::Event::NoteOn {
				time,
				pitch,
				velocity,
				note_id: id,
				..
			} => Some(Self::On {
				time,
				key: u8::try_from(pitch).ok()?,
				velocity,
				note_id: note_id(id),
				flags: EventFlags::empty(),
			}),
			vst3_host::Event::NoteOff {
				time,
				pitch,
				velocity,
				note_id: id,
				..
			} => Some(Self::Off {
				time,
				key: u8::try_from(pitch).ok()?,
				velocity,
				note_id: note_id(id),
				flags: EventFlags::empty(),
			}),
			vst3_host::Event::ParamValue { time, id, value } => Some(Self::ParamValue {
				time,
				param_id: ClapId::from_raw(id)?,
				value: value as f32,
				flags: EventFlags::empty(),
			}),
		}
	}
}

impl audio_graph::EventImpl for Event {
	fn time(&self) -> usize {
		match *self {
			Self::On { time, .. } | Self::Off { time, .. } | Self::ParamValue { time, .. } => {
				time as usize
			}
		}
	}

	fn at(&self, at: usize) -> Self {
		let mut this = *self;
		match &mut this {
			Self::On { time, .. } | Self::Off { time, .. } | Self::ParamValue { time, .. } => {
				*time = at as u32;
			}
		}
		this
	}
}

impl events::EventImpl for Event {
	fn to_clap(self, dialect: NoteDialect) -> ClapEvent {
		match self {
			Self::On {
				time,
				key,
				velocity,
				note_id,
				flags,
			} => match dialect {
				NoteDialect::Clap => ClapEvent::NoteOn(
					NoteOnEvent::new(time, Pckn::new(0u16, 0u16, key, note_id), velocity.into())
						.with_flags(flags),
				),
				NoteDialect::Midi | NoteDialect::MidiMpe => ClapEvent::Midi(
					MidiEvent::new(
						time,
						0u16,
						[0x90, key, (velocity * 127.0).round().max(1.0) as u8],
					)
					.with_flags(flags),
				),
				NoteDialect::Midi2 => ClapEvent::Midi2(
					Midi2Event::new(
						time,
						0u16,
						[
							(0x4090 << 16) | (u32::from(key) << 8),
							(u32::from((velocity * 65535.0).round() as u16) << 16),
							0,
							0,
						],
					)
					.with_flags(flags),
				),
			},
			Self::Off {
				time,
				key,
				velocity,
				note_id,
				flags,
			} => match dialect {
				NoteDialect::Clap => ClapEvent::NoteOff(
					NoteOffEvent::new(time, Pckn::new(0u16, 0u16, key, note_id), velocity.into())
						.with_flags(flags),
				),
				NoteDialect::Midi | NoteDialect::MidiMpe => ClapEvent::Midi(
					MidiEvent::new(time, 0u16, [0x80, key, (velocity * 127.0).round() as u8])
						.with_flags(flags),
				),
				NoteDialect::Midi2 => ClapEvent::Midi2(
					Midi2Event::new(
						time,
						0u16,
						[
							(0x4080 << 16) | (u32::from(key) << 8),
							(u32::from((velocity * 65535.0).round() as u16) << 16),
							0,
							0,
						],
					)
					.with_flags(flags),
				),
			},
			Self::ParamValue {
				time,
				param_id,
				value,
				flags,
			} => ClapEvent::ParamValue(
				ParamValueEvent::new(
					time,
					param_id,
					Pckn::new(0u16, 0u16, Match::All, Match::All),
					value.into(),
					Cookie::empty(),
				)
				.with_flags(flags),
			),
		}
	}

	fn try_from_unknown(value: &UnknownEvent) -> Option<Self> {
		match value.as_core_event()? {
			CoreEventSpace::NoteOn(event) => Some(Self::On {
				time: event.time(),
				key: *event.key().as_specific()? as u8,
				velocity: event.velocity() as f32,
				note_id: event.note_id(),
				flags: event.flags(),
			}),
			CoreEventSpace::NoteOff(event) => Some(Self::Off {
				time: event.time(),
				key: *event.key().as_specific()? as u8,
				velocity: event.velocity() as f32,
				note_id: event.note_id(),
				flags: event.flags(),
			}),
			CoreEventSpace::ParamValue(event) => Some(Self::ParamValue {
				time: event.time(),
				param_id: event.param_id()?,
				value: event.value() as f32,
				flags: event.flags(),
			}),
			CoreEventSpace::Midi(event) => {
				let data = event.data();
				match data[0] & 0xf0 {
					0x90 if data[2] != 0 => Some(Self::On {
						time: event.time(),
						key: data[1],
						velocity: f32::from(data[2]) / 127.0,
						note_id: Match::All,
						flags: event.flags(),
					}),
					0x90 | 0x80 => Some(Self::Off {
						time: event.time(),
						key: data[1],
						velocity: f32::from(data[2]) / 127.0,
						note_id: Match::All,
						flags: event.flags(),
					}),
					_ => None,
				}
			}
			CoreEventSpace::Midi2(event) => {
				let data = event.data();
				match data[0] >> 16 {
					0x4090 => Some(Self::On {
						time: event.time(),
						key: (data[0] >> 8) as u8,
						velocity: f32::from((data[1] >> 16) as u16) / 65535.0,
						note_id: Match::All,
						flags: event.flags(),
					}),
					0x4080 => Some(Self::Off {
						time: event.time(),
						key: (data[0] >> 8) as u8,
						velocity: f32::from((data[1] >> 16) as u16) / 65535.0,
						note_id: Match::All,
						flags: event.flags(),
					}),
					_ => None,
				}
			}
			_ => None,
		}
	}
}
