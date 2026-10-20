//! Tests for the VST3 host, with the small plugins in `vst3_host/test_plugin`.
//!
//! These can't open real windows, so they look at what the host decides to do.

#[path = "../../../vst3_host/tests/common/mod.rs"]
mod common;

use super::{Message, Vst3Host};
use crate::{clap_host::PluginId, daw};
use generic_daw_core::{
	AudioBackend, AudioProcessor, AudioThreadMessage, Transport,
	clap_host::ClapId,
	vst3_host::{self as host, EditorSize, MainThreadMessage, ParentWindow},
};
use fragile::Fragile;
use iced::{Task, window};
use std::{num::NonZero, path::Path, sync::Arc};
use utils::{NoClone, NoDebug};

const GAIN: u32 = 0;
const METER: u32 = 3;

fn transport() -> Transport {
	Transport::new(
		0,
		NonZero::new(2).unwrap(),
		NonZero::new(48000).unwrap(),
		NonZero::new(256).unwrap(),
	)
}

/// Runs a message, and returns the instructions it came with.
fn run(host: &mut Vst3Host, message: Message) -> Vec<daw::Instruction> {
	let mut instructions = Vec::new();

	let _task = host
		.update(message, &transport())
		.handle(|message| message, |instruction| {
			instructions.push(instruction);
			Task::none()
		});

	instructions
}

fn audio_thread(id: PluginId, message: AudioThreadMessage) -> Message {
	Message::AudioThread(id, NoClone(Box::new(message)))
}

fn add(host: &mut Vst3Host, name: &str) -> PluginId {
	let path: Arc<Path> = common::bundle().into();
	let descriptor = host::descriptors(&path)
		.unwrap()
		.into_iter()
		.find(|d| d.name.starts_with(name))
		.unwrap();

	let (plugin, receiver) = host::Plugin::new(&descriptor).unwrap();
	let id = PluginId::unique();
	let _task = host.plugin_add(id, plugin, receiver);
	id
}

fn host() -> Vst3Host {
	Vst3Host::new()
}

/// Activates the plugin, and hands back the audio thread the way the arrangement would.
fn activate(host: &mut Vst3Host, id: PluginId) -> (AudioProcessor, usize) {
	let mut instructions = run(host, Message::Activate(id));
	assert_eq!(instructions.len(), 1);

	let daw::Instruction::Vst3PluginActivate(activated, audio, params) = instructions.remove(0)
	else {
		panic!("expected the plugin to be activated");
	};
	assert_eq!(activated, id);

	(AudioProcessor::create_vst3(audio, params).0, params)
}

fn param(id: u32) -> ClapId {
	ClapId::new(id)
}

#[test]
fn activating_gives_the_audio_thread() {
	let mut host = host();
	let id = add(&mut host, "Gain");

	assert!(!host.plugins[&id].active);
	let (processor, params) = activate(&mut host, id);
	assert!(host.plugins[&id].active);
	assert_eq!(params, 5);

	// activating twice does nothing
	assert!(run(&mut host, Message::Activate(id)).is_empty());

	// the audio thread hands the processor back, and the plugin is inactive again
	let instructions = run(&mut host, audio_thread(id, AudioThreadMessage::Deactivate(processor)));
	assert!(instructions.is_empty());
	assert!(!host.plugins[&id].active);

	// it can be activated again
	let (processor, _) = activate(&mut host, id);
	run(&mut host, audio_thread(id, AudioThreadMessage::Deactivate(processor)));
}

#[test]
fn parameters_from_the_list_go_to_the_plugin_and_the_processor() {
	let mut host = host();
	let id = add(&mut host, "Gain");

	// inactive plugins only have the value set
	let instructions = run(&mut host, Message::HostParamValueChange(id, param(GAIN), 0.5));
	assert!(instructions.is_empty());
	assert_eq!(host.plugins[&id].plugin.param_normalized(GAIN), Some(0.5));

	let (processor, _) = activate(&mut host, id);

	let instructions = run(&mut host, Message::HostParamValueChange(id, param(GAIN), 0.25));
	assert!(matches!(
		instructions[..],
		[daw::Instruction::PluginParamChanged(changed, changed_param, value)]
			if changed == id && changed_param == param(GAIN) && (value - 0.25).abs() < 1e-6
	));
	assert_eq!(host.plugins[&id].plugin.param_normalized(GAIN), Some(0.25));

	// parameters that are only shown aren't set
	let instructions = run(&mut host, Message::HostParamValueChange(id, param(METER), 0.9));
	assert!(instructions.is_empty());

	run(&mut host, audio_thread(id, AudioThreadMessage::Deactivate(processor)));
}

#[test]
fn values_the_plugin_changes_reach_the_controller() {
	let mut host = host();
	let id = add(&mut host, "Gain");

	let instructions = run(&mut host, Message::PluginParamValueChange(id, param(GAIN), 0.75));
	assert!(instructions.is_empty());
	assert_eq!(host.plugins[&id].plugin.param_normalized(GAIN), Some(0.75));
}

#[test]
fn edits_in_the_plugins_editor_reach_the_processor() {
	let mut host = host();
	let id = add(&mut host, "Gain");

	let edit = Message::MainThread(id, MainThreadMessage::PerformEdit(GAIN, 0.4));

	// not while nothing is listening
	assert!(run(&mut host, edit.clone()).is_empty());

	let (processor, _) = activate(&mut host, id);

	let instructions = run(&mut host, edit);
	assert!(matches!(
		instructions[..],
		[daw::Instruction::PluginParamChanged(changed, changed_param, value)]
			if changed == id && changed_param == param(GAIN) && (value - 0.4).abs() < 1e-6
	));

	// beginning and ending an edit has nothing to do
	assert!(run(&mut host, Message::MainThread(id, MainThreadMessage::BeginEdit(GAIN))).is_empty());
	assert!(run(&mut host, Message::MainThread(id, MainThreadMessage::EndEdit(GAIN))).is_empty());

	run(&mut host, audio_thread(id, AudioThreadMessage::Deactivate(processor)));
}

#[test]
fn restarts_set_up_the_audio_again() {
	let mut host = host();
	let id = add(&mut host, "Restarter");
	let (processor, _) = activate(&mut host, id);

	// the plugin turns mono, and the audio thread hands the processor back
	host.plugins[&id].plugin.set_param_normalized(0, 0.2).unwrap();
	let mut instructions = run(&mut host, audio_thread(id, AudioThreadMessage::Restart(processor)));

	let daw::Instruction::Vst3PluginRestart(restarted, Some((audio, params))) = instructions.remove(0)
	else {
		panic!("expected the plugin to be restarted");
	};
	assert_eq!(restarted, id);
	assert_eq!(audio.layout().main_input, Some((0, 1)));
	assert!(host.plugins[&id].active);

	let (processor, _) = AudioProcessor::create_vst3(audio, params);
	run(&mut host, audio_thread(id, AudioThreadMessage::Deactivate(processor)));
	assert!(!host.plugins[&id].active);
}

#[test]
fn destroying_removes_the_plugin() {
	let mut host = host();
	let id = add(&mut host, "Gain");
	let (processor, _) = activate(&mut host, id);

	run(&mut host, audio_thread(id, AudioThreadMessage::Destroy(processor)));
	assert!(host.plugins.is_empty());

	// plugins that aren't active can go right away
	let id = add(&mut host, "Gain");
	run(&mut host, Message::DestroyInactive(id));
	assert!(host.plugins.is_empty());
}

#[test]
fn plugins_without_an_editor_get_a_window_with_their_parameters() {
	let mut host = host();
	let id = add(&mut host, "Gain");

	assert!(run(&mut host, Message::GuiOpen(id)).is_empty());

	let window = host.window_of_plugin[&id];
	assert_eq!(host.plugin_of_window[&window], id);
	assert!(!host.editor_windows.contains(&window));

	// the plugin stays here, and the window shows its parameters
	assert!(host.plugins.contains_key(&id));
	assert!(host.view(window).is_some());
	assert_eq!(host.title(window).as_deref(), Some("Gain (vst3-rs example plugin)"));
	assert_eq!(host.scale_factor(window), None);

	// opening it again doesn't make another window
	run(&mut host, Message::GuiOpen(id));
	assert_eq!(host.plugin_of_window.len(), 1);

	run(&mut host, Message::WindowClosed(window));
	assert!(host.window_of_plugin.is_empty());
	assert!(host.plugin_of_window.is_empty());
}

#[test]
fn editors_get_a_window_where_a_point_is_a_pixel() {
	let mut host = host();
	let id = add(&mut host, "Restarter");

	run(&mut host, Message::GuiOpen(id));

	let window = host.window_of_plugin[&id];
	assert!(host.editor_windows.contains(&window));
	// the plugin is on its way to the window, where it's put in
	assert!(!host.plugins.contains_key(&id));

	// the scale of the screen isn't known yet
	assert_eq!(host.scale_factor(window), None);

	run(&mut host, Message::WindowRescaled(window, 2.0));
	assert_eq!(host.scale_factor(window), Some(0.5));

	run(&mut host, Message::WindowClosed(window));
	assert!(host.window_of_plugin.is_empty());
	assert!(host.editor_windows.is_empty());
	assert_eq!(host.scale_factor(window), None);
}

#[test]
fn windows_of_other_hosts_are_nothing_to_this_one() {
	let mut host = host();
	let id = add(&mut host, "Gain");
	let stranger = window::Id::unique();

	run(&mut host, Message::WindowClosed(stranger));
	run(&mut host, Message::WindowCloseRequested(stranger));
	run(&mut host, Message::WindowRescaled(stranger, 1.5));
	run(&mut host, Message::WindowResized(stranger, iced::Size::new(10.0, 10.0)));

	assert!(host.view(stranger).is_none());
	assert!(host.title(stranger).is_none());
	assert!(host.plugins.contains_key(&id));
}

#[test]
fn messages_for_plugins_that_arent_there_yet_are_tried_again() {
	let mut host = host();
	let id = PluginId::unique();

	// nothing happens right now, and nothing panics
	assert!(run(&mut host, Message::Activate(id)).is_empty());
	assert!(run(&mut host, Message::HostParamValueChange(id, param(GAIN), 0.5)).is_empty());
	assert!(run(&mut host, Message::MainThread(id, MainThreadMessage::PerformEdit(GAIN, 0.5))).is_empty());
	assert!(host.plugins.is_empty());
}

#[test]
fn state_goes_from_one_plugin_to_another() {
	let mut host = host();
	let from = add(&mut host, "Gain");
	let to = add(&mut host, "Gain");

	// the mode parameter is part of what the controller saves
	host.plugins[&from].plugin.set_param_normalized(2, 2.0 / 3.0).unwrap();
	let state = host.get_state(from).unwrap();

	assert!(run(&mut host, Message::SetState(to, NoDebug(state.into_boxed_slice()))).is_empty());
	let mode = host.plugins[&to].plugin.param_normalized(2).unwrap();
	assert!((mode - 2.0 / 3.0).abs() < 1e-9);

	assert!(host.get_state(PluginId::unique()).is_none());
}

#[test]
fn damaged_states_are_ignored() {
	let mut host = host();
	let id = add(&mut host, "Gain");

	assert!(run(&mut host, Message::SetState(id, NoDebug(Box::from(&b"garbage"[..])))).is_empty());
	assert_eq!(host.plugins[&id].plugin.param_normalized(GAIN), Some(1.0));
}

#[test]
fn the_render_mode_is_used_for_the_next_activation() {
	let mut host = host();
	let id = add(&mut host, "Probe");

	host.set_render_mode(generic_daw_core::clap_host::RenderMode::Offline);
	let (processor, _) = activate(&mut host, id);

	// the probe tells what mode it was set up for, in its right channel
	let AudioBackend::Vst3(mut audio) = processor.into_backend() else {
		panic!("should be a VST3 plugin");
	};
	let mut buf = vec![[0.0, 0.0]; 64];
	audio.process(&mut buf, None, 1.0);
	// processing mode 2 is offline
	assert_eq!(buf[3][1], 2.0);

	host.plugins.get_mut(&id).unwrap().plugin.deactivate(audio);
}

/// Puts a plugin in a window the way `GuiOpen` and the window do, with a pretend window.
fn open_editor(host: &mut Vst3Host, id: PluginId) -> window::Id {
	let window = window::Id::unique();
	host.window_of_plugin.insert(id, window);
	host.plugin_of_window.insert(window, id);
	host.editor_windows.insert(window);

	let mut hosted = host.plugins.remove(&id).unwrap();
	hosted.plugin.create_editor().unwrap();
	hosted
		.plugin
		.attach_editor(ParentWindow::native(std::ptr::without_provenance_mut(0x1000)))
		.unwrap();

	run(host, Message::GuiOpened(id, NoClone(NoDebug(Box::new(Fragile::new(hosted))))));
	assert!(host.plugins.contains_key(&id));

	window
}

fn editor_size(host: &Vst3Host, id: PluginId) -> Option<EditorSize> {
	host.plugins[&id].plugin.editor_size()
}

fn size(width: i32, height: i32) -> EditorSize {
	EditorSize { width, height }
}

#[test]
fn editors_follow_their_window() {
	let mut host = host();
	let id = add(&mut host, "Restarter");
	let window = open_editor(&mut host, id);

	// the window sizes before the scale is known aren't the editor's
	run(&mut host, Message::WindowResized(window, iced::Size::new(500.0, 400.0)));
	assert_eq!(editor_size(&host, id), Some(size(400, 300)));

	// the editor is told how many pixels the screen has per point
	run(&mut host, Message::WindowRescaled(window, 2.0));
	assert_eq!(host.plugins[&id].plugin.param_normalized(6), Some(0.5));

	// after that the window decides, within what the editor allows
	run(&mut host, Message::WindowResized(window, iced::Size::new(433.0, 90.0)));
	assert_eq!(editor_size(&host, id), Some(size(430, 100)));

	run(&mut host, Message::WindowResized(window, iced::Size::new(5000.0, 5000.0)));
	assert_eq!(editor_size(&host, id), Some(size(800, 600)));
}

#[test]
fn editors_can_ask_for_a_size() {
	let mut host = host();
	let id = add(&mut host, "Restarter");
	let window = open_editor(&mut host, id);

	run(&mut host, Message::MainThread(id, MainThreadMessage::ResizeEditor { width: 640, height: 480 }));
	assert_eq!(editor_size(&host, id), Some(size(640, 480)));

	// without a window there's nothing to resize, and the editor stays as it is
	run(&mut host, Message::WindowClosed(window));
	run(&mut host, Message::MainThread(id, MainThreadMessage::ResizeEditor { width: 300, height: 300 }));
	assert_eq!(editor_size(&host, id), Some(size(640, 480)));
}

#[test]
fn closing_the_window_takes_the_editor_out_of_it() {
	let mut host = host();
	let id = add(&mut host, "Restarter");
	let window = open_editor(&mut host, id);
	assert!(host.plugins[&id].plugin.has_editor_open());
	assert_eq!(host.plugins[&id].plugin.param_normalized(3), Some(1.0));

	run(&mut host, Message::WindowCloseRequested(window));

	// the editor has to be gone before the window is
	assert!(!host.plugins[&id].plugin.has_editor_open());
	assert_eq!(host.plugins[&id].plugin.param_normalized(3), Some(0.0));

	run(&mut host, Message::WindowClosed(window));
	assert!(host.window_of_plugin.is_empty());

	// and the editor can be opened again
	let window = open_editor(&mut host, id);
	assert_eq!(host.window_of_plugin[&id], window);
}

#[test]
fn an_editor_that_didnt_attach_is_replaced_by_the_parameters() {
	let mut host = host();
	let id = add(&mut host, "Restarter");

	let window = window::Id::unique();
	host.window_of_plugin.insert(id, window);
	host.plugin_of_window.insert(window, id);
	host.editor_windows.insert(window);

	// the plugin comes back from the window without having an editor
	let hosted = host.plugins.remove(&id).unwrap();
	run(&mut host, Message::GuiOpened(id, NoClone(NoDebug(Box::new(Fragile::new(hosted))))));

	assert!(!host.editor_windows.contains(&window));
	let new_window = host.window_of_plugin[&id];
	assert_ne!(new_window, window);
	assert!(host.view(new_window).is_some());

	// the old window going away doesn't take the new one with it
	run(&mut host, Message::WindowClosed(window));
	assert_eq!(host.window_of_plugin[&id], new_window);
}

#[test]
fn removing_a_plugin_closes_its_editor() {
	let mut host = host();
	let id = add(&mut host, "Restarter");
	open_editor(&mut host, id);

	run(&mut host, Message::DestroyInactive(id));
	assert!(host.plugins.is_empty());
}
