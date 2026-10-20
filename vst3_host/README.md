# vst3_host

A safe-ish Rust host for VST3 plugins, modeled on `clap_host`. It wraps the raw
[`vst3`](https://crates.io/crates/vst3) bindings, and is meant to plug into Generic DAW next to
`clap_host`.

## What it does

| Area | API |
|---|---|
| Finding plugins | `find_plugin_paths`, `DEFAULT_VST3_PATHS`, `descriptors` |
| Creating a plugin | `Plugin::new` (component, controller, host context, connection points) |
| Parameters | `Plugin::params`, `param_normalized`, `set_param_normalized`, `param_display`, `param_from_display` |
| Audio | `Plugin::activate` → `AudioThread::process`, `Plugin::deactivate` |
| Notes and automation | `AudioThread::push_event`, `drain_output_events` |
| State | `Plugin::save_state`, `Plugin::load_state` |
| Restart requests | `MainThreadMessage::Restart` → `Plugin::handle_restart` → `Plugin::restart`; the audio thread can look at `AudioThread::restart_flags` without waiting for the main thread |

```rust
let (mut plugin, messages) = Plugin::new(&descriptor)?;               // main thread
let mut audio = plugin.activate(AudioConfig { sample_rate: 48000.0, max_frames: 512, offline: false })?;

audio.push_event(Event::NoteOn { time: 0, channel: 0, pitch: 60, velocity: 0.8, note_id: None });
audio.process(&mut stereo_frames, Some(&transport), 1.0);              // audio thread

plugin.deactivate(audio);                                              // main thread
```

Threading: everything on `Plugin` is main thread. `AudioThread` is `Send` and is the only thing
that touches the processor while active. `MainThreadMessage`s (parameter edits made in the
plugin's own UI, restart requests) arrive on the channel returned by `Plugin::new`.

## Where it is used

- `generic_daw_core` runs a VST3 plugin in a plugin slot next to CLAP ones (`AudioBackend::Vst3`),
  converting its events and transport. Parameter values for VST3 plugins are normalized
  (`0.0..=1.0`), and parameter ids are passed on as they are.
- `generic_daw_project` stores a VST3 plugin as format `Vst3`, with the class id as hex digits,
  and the blob from `Plugin::save_state` as its state. Older versions of the app that open such a
  project say they can't find the plugin.

## Not done yet

- Plugin editor windows (`IPlugView`), including `IPlugFrame` and `IRunLoop` on Linux, and the
  plugin picker and parameter UI in `generic_daw_gui`.
- MIDI CC, pitch bend and aftertouch (these go through `IMidiMapping`).
- Skipping silent blocks (the plugin is processed whenever it is active).
- macOS bundle loading (`bundleEntry` needs a `CFBundle`).
- Presets and program lists, `IComponentHandler2::setDirty`.
- Out-of-process scanning, so a plugin that crashes while loading takes the host down.

## Testing

```
cargo test -p vst3_host
```

The tests build `test_plugin/` (a small VST3 plugin with several classes, derived from the `gain`
example of the `vst3` crate) and wrap it in a bundle. Linux and Windows layouts are supported, but
only Linux has been run by the author, **please run the tests on Windows**. They exercise the Windows
loader, which has been type-checked by hand but never executed.
