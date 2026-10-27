#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use daw::{CRASHES_DIR, Daw, format_now};
use iced::{Result, daemon};
use icons::LUCIDE_BYTES;
use log::LevelFilter;
use state::State;
use std::{backtrace::Backtrace, fs::File, io::Write as _, sync::Arc};

mod action;
mod app_icon;
mod arrangement_view;
mod clap_host;
mod components;
mod config;
mod config_view;
mod daw;
mod file_tree;
mod icons;
mod lod;
mod mesh_budget;
mod operation;
mod plugin_descriptor;
mod plugin_memory;
mod plugin_picker;
mod shortcuts;
mod state;
mod stylefns;
mod theme;
mod vst2_host;
mod vst3_host;
mod widget;

fn main() -> Result {
	// The program can be told to look for VST2 plugins instead of starting. That's how
	// the plugins are found: some of them crash when they're made, and a crash takes the
	// whole program with it.
	if let Some(paths) =
		generic_daw_core::vst2_host::scan::paths_from_args(std::env::args().skip(1))
	{
		generic_daw_core::vst2_host::scan::print_plugins(&paths);
		return Ok(());
	}

	install_panic_hook();

	env_logger::builder()
		.filter_level(LevelFilter::Warn)
		.parse_default_env()
		.init();

	daemon(Daw::create, Daw::update, Daw::view)
		.title(Daw::title)
		.theme(Daw::theme)
		.scale_factor(Daw::scale_factor)
		.subscription(Daw::subscription)
		.font(LUCIDE_BYTES)
		.run()
}

fn install_panic_hook() {
	let default_hook = std::panic::take_hook();
	std::panic::set_hook(Box::new(move |info| {
		default_hook(info);
		let crash_log = Arc::from(CRASHES_DIR.join(format!("{}.log", format_now())));
		if let Ok(mut file) = File::create(&crash_log) {
			let current_thread = std::thread::current();
			let current_thread_name = current_thread.name().unwrap_or("<unnamed>");
			let backtrace = Backtrace::force_capture();
			write!(
				file,
				"thread '{current_thread_name}' {info}\nstack backtrace:\n{backtrace}",
			)
			.unwrap();
			let mut state = State::read();
			state.crash_log = Some(crash_log);
			state.write();
		}
	}));
}
