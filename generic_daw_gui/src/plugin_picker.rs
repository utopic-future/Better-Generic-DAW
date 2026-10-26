use crate::plugin_descriptor::PluginDescriptor;
use crate::{
	action::Action,
	components::{menu_entry, virtualized},
	daw,
	icons::star,
	plugin_memory::{PluginEntry, PluginKey, PluginMemory},
	shortcuts::Action as ShortcutAction,
	stylefns::{
		container_with_radius, scrollable_style, scrollable_with_container, selectable_box,
		text_input_with_radius, weaker_bordered_box, weakest_bordered_box,
	},
	widget::LINE_HEIGHT,
};
use generic_daw_core::clap_host::{Category, Subcategory};
use generic_daw_widget::context_menu::ContextMenu;
use iced::{
	Element, Fill, FillPortion, Font, Shrink, Task,
	font::Weight,
	padding,
	widget::{
		Space, checkbox, column, container, mouse_area, operation::focus, row, scrollable, text,
		text_input,
	},
};
use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
};
use unicode_segmentation::UnicodeSegmentation as _;
use utils::natural_cmp;

#[derive(Clone, Debug)]
pub enum Message {
	Query(Arc<str>),
	Category(Category),
	Subcategory(Subcategory),
	Hover(usize),
	HoverNext,
	HoverPrev,
	Select,
	/// Loads the plugin with this index.
	Load(usize),
	/// Adds the plugin to the favorites, or takes it out of them.
	ToggleFavorite(usize),
	/// Shows only the plugins that are favorites.
	FavoritesOnly(bool),
}

#[derive(Debug, Default)]
pub struct State {
	options: Vec<PluginDescriptor>,
	/// The keys of the plugins in `options`.
	keys: Vec<PluginKey>,
	matchers: Vec<(Box<str>, Option<Box<str>>)>,
	/// The plugins that the scan that's going on found.
	found: HashSet<PluginKey>,
	/// What we remember about the plugins on this computer.
	memory: PluginMemory,
}

impl State {
	pub fn descriptors(&self) -> &[PluginDescriptor] {
		&self.options
	}

	/// What we remember about the plugins on this computer.
	pub fn memory(&self) -> &PluginMemory {
		&self.memory
	}
}

impl State {
	/// Reads what we remember about the plugins, and the plugins themselves.
	#[must_use]
	pub fn read() -> Self {
		let mut this = Self {
			memory: PluginMemory::read(),
			..Self::default()
		};

		this.load_from_memory();

		this
	}

	/// Adds the plugins we remember, so that they're there right away, before the scan
	/// had a chance to find them again.
	pub fn load_from_memory(&mut self) {
		let descriptors = self
			.memory
			.plugins
			.iter()
			.filter_map(PluginEntry::descriptor)
			.collect::<Vec<_>>();

		for descriptor in descriptors {
			// the same plugin in different formats are different options
			if let Err(i) = self.options.binary_search_by(|option| {
				natural_cmp(option.name().as_bytes(), descriptor.name().as_bytes())
					.then(option.format_rank().cmp(&descriptor.format_rank()))
			}) {
				self.insert(i, descriptor);
			}
		}
	}

	/// Adds a plugin that a scan found.
	pub fn add(&mut self, descriptor: PluginDescriptor, plugin_picker: Option<&mut PluginPicker>) {
		let key = PluginKey::from_descriptor(&descriptor);
		self.found.insert(key.clone());

		// the same plugin in different formats are different options
		match self.options.binary_search_by(|option| {
			natural_cmp(option.name().as_bytes(), descriptor.name().as_bytes())
				.then(option.format_rank().cmp(&descriptor.format_rank()))
		}) {
			Ok(i) => {
				if self.keys[i] != key && self.found.contains(&self.keys[i]) {
					// there are two plugins with the same name and format, and we
					// keep the one we found first
					return;
				}

				// the plugin that's there is either the same one, or one we remember
				// but that isn't where it used to be
				self.set(i, descriptor);
			}
			Err(i) => self.insert(i, descriptor),
		}

		if let Some(plugin_picker) = plugin_picker {
			plugin_picker.search(self);
		}
	}

	/// Records that a scan is going on.
	pub fn start_scan(&mut self) {
		self.found.clear();
	}

	/// Takes the plugins that the scan didn't find out of the list.
	pub fn finish_scan(&mut self, plugin_picker: Option<&mut PluginPicker>) {
		let mut i = 0;

		while i < self.options.len() {
			if self.found.contains(&self.keys[i]) {
				i += 1;
				continue;
			}

			self.options.remove(i);
			self.keys.remove(i);
			self.matchers.remove(i);
		}

		self.memory.finish_scan(&self.found);

		if let Some(plugin_picker) = plugin_picker {
			plugin_picker.search(self);
		}
	}

	/// Records that a scan found a plugin.
	pub fn record_scan(&mut self, descriptor: &PluginDescriptor) {
		self.memory.record_scan(descriptor);
	}

	/// Records that a plugin was loaded.
	pub fn mark_used(&mut self, descriptor: &PluginDescriptor) {
		self.memory
			.mark_used(&PluginKey::from_descriptor(descriptor));
	}

	/// Adds the plugin to the favorites, or takes it out of them. Tells whether the
	/// plugin is a favorite afterwards.
	pub fn toggle_favorite(&mut self, i: usize) -> bool {
		let Some(key) = self.keys.get(i).cloned() else {
			return false;
		};

		self.memory.toggle_favorite(&key)
	}

	fn insert(&mut self, i: usize, descriptor: PluginDescriptor) {
		let key = PluginKey::from_descriptor(&descriptor);
		let matcher = (
			build_matcher(descriptor.name()),
			descriptor.vendor().map(build_matcher),
		);

		self.options.insert(i, descriptor);
		self.keys.insert(i, key);
		self.matchers.insert(i, matcher);
	}

	fn set(&mut self, i: usize, descriptor: PluginDescriptor) {
		let key = PluginKey::from_descriptor(&descriptor);
		let matcher = (
			build_matcher(descriptor.name()),
			descriptor.vendor().map(build_matcher),
		);

		self.options[i] = descriptor;
		self.keys[i] = key;
		self.matchers[i] = matcher;
	}
}

#[derive(Debug)]
pub struct PluginPicker {
	query: Arc<str>,
	categories: HashSet<Category>,
	subcategories: HashSet<Subcategory>,
	favorites_only: bool,
	filtered_options: Vec<usize>,
	hovered_option: usize,
}

impl PluginPicker {
	pub fn create(state: &State) -> (Self, Task<Message>) {
		let mut this = Self {
			query: Arc::default(),
			categories: HashSet::with_capacity(Category::VARIANTS.len()),
			subcategories: HashSet::with_capacity(Subcategory::VARIANTS.len()),
			favorites_only: false,
			filtered_options: Vec::new(),
			hovered_option: 0,
		};

		this.search(state);

		(this, focus("plugin picker input"))
	}

	pub fn update(
		&mut self,
		message: Message,
		state: &mut State,
	) -> Action<daw::Instruction, Message> {
		match message {
			Message::Query(query) => {
				self.query = query;
				self.search(state);
			}
			Message::Category(category) => {
				if !self.categories.remove(&category) {
					self.categories.insert(category);
				}
				self.search(state);
			}
			Message::Subcategory(subcategory) => {
				if !self.subcategories.remove(&subcategory) {
					self.subcategories.insert(subcategory);
				}
				self.search(state);
			}
			Message::FavoritesOnly(favorites_only) => {
				self.favorites_only = favorites_only;
				self.search(state);
			}
			Message::ToggleFavorite(i) => {
				state.toggle_favorite(i);
				self.search(state);
			}
			Message::Hover(i) => self.hovered_option = i,
			Message::HoverNext => self.hovered_option += 1,
			Message::HoverPrev => self.hovered_option = self.hovered_option().saturating_sub(1),
			Message::Load(i) => {
				if let Some(descriptor) = state.options.get(i) {
					return Action::batch([
						Action::instruction(daw::Instruction::PluginLoad(descriptor.clone())),
						Action::instruction(daw::Instruction::Message(
							daw::Message::TogglePluginPicker,
						)),
					]);
				}
			}
			Message::Select => {
				if !self.filtered_options.is_empty() {
					return Action::batch([
						Action::instruction(daw::Instruction::PluginLoad(
							state.options[self.filtered_options[self.hovered_option()]].clone(),
						)),
						Action::instruction(daw::Instruction::Message(
							daw::Message::TogglePluginPicker,
						)),
					]);
				}
			}
		}

		Action::none()
	}

	pub fn view<'a>(&'a self, state: &'a State) -> Element<'a, Message> {
		scrollable(
			column![
				text_input("Add Plugin", &*self.query)
					.id("plugin picker input")
					.on_input(|query| Message::Query(query.into()))
					.style(text_input_with_radius(text_input::default, 5)),
				row![
					column![
						column![
							text("Category").font(Font::DEFAULT.weight(Weight::Bold)),
							container(column(Category::VARIANTS.iter().map(|category| {
								checkbox(
									self.categories.is_empty()
										|| self.categories.contains(category),
								)
								.label(category.to_string())
								.on_toggle(|_| Message::Category(*category))
								.width(Fill)
								.into()
							})))
							.padding(padding::horizontal(5).vertical(2.6))
							.style(container_with_radius(weakest_bordered_box, 5))
						]
						.spacing(5),
						column![
							text("Subcategory").font(Font::DEFAULT.weight(Weight::Bold)),
							container(column(Subcategory::VARIANTS.iter().map(|subcategory| {
								checkbox(
									self.subcategories.is_empty()
										|| self.subcategories.contains(subcategory),
								)
								.label(subcategory.to_string())
								.on_toggle(|_| Message::Subcategory(*subcategory))
								.width(Fill)
								.into()
							})))
							.padding(padding::horizontal(5).vertical(2.6))
							.style(container_with_radius(weakest_bordered_box, 5))
						]
						.spacing(5),
						column![
							text("Favorites").font(Font::DEFAULT.weight(Weight::Bold)),
							container(
								column(state.memory.favorite_entries().filter_map(|entry| {
									let i = state.keys.iter().position(|key| key == &entry.key)?;

									Some(
										menu_entry(star(), entry.name.as_ref(), "")
											.on_press(Message::Load(i))
											.into(),
									)
								}))
								.width(Fill),
							)
							.padding(padding::horizontal(5).vertical(2.6))
							.style(container_with_radius(weakest_bordered_box, 5)),
							container(
								checkbox(self.favorites_only)
									.label("Only favorites")
									.on_toggle(Message::FavoritesOnly)
									.width(Fill),
							)
							.padding(padding::horizontal(5).vertical(2.6))
							.style(container_with_radius(weakest_bordered_box, 5))
						]
						.spacing(5),
						column![
							text("Recent").font(Font::DEFAULT.weight(Weight::Bold)),
							container(
								column(state.memory.recent_entries().take(5).filter_map(|entry| {
									let i = state.keys.iter().position(|key| key == &entry.key)?;

									Some(
										menu_entry(None, entry.name.as_ref(), "")
											.on_press(Message::Load(i))
											.into(),
									)
								}))
								.width(Fill),
							)
							.padding(padding::horizontal(5).vertical(2.6))
							.style(container_with_radius(weakest_bordered_box, 5))
						]
						.spacing(5),
					]
					.spacing(10),
					scrollable(
						column(
							self.filtered_options
								.iter()
								.map(|&option| (
									option,
									&state.options[option],
									&state.keys[option]
								))
								.enumerate()
								.map(|(i, (option, descriptor, key))| {
									let favorite = state.memory.is_favorite(key);

									virtualized(move || {
										let favorite_icon: Element<'_, Message> = if favorite {
											star().size(13.0).into()
										} else {
											Space::new().width(13.0).into()
										};

										ContextMenu::new(
											mouse_area(
												container(
													row![
														favorite_icon,
														text(descriptor.name())
															.wrapping(text::Wrapping::None)
															.ellipsis(text::Ellipsis::End),
														text(
															descriptor
																.version()
																.unwrap_or_default()
														)
														.wrapping(text::Wrapping::None)
														.ellipsis(text::Ellipsis::End)
														.style(text::secondary)
														.width(Fill),
														text(
															descriptor.vendor().unwrap_or_default()
														)
														.wrapping(text::Wrapping::None)
														.ellipsis(text::Ellipsis::Start)
														.style(text::secondary),
														text(descriptor.format())
															.wrapping(text::Wrapping::None)
															.style(text::secondary),
													]
													.spacing(5)
													.padding(padding::horizontal(3)),
												)
												.padding(1)
												.height(LINE_HEIGHT + 2.0)
												.style(container_with_radius(
													selectable_box(
														container::transparent,
														i == self.hovered_option(),
													),
													5,
												)),
											)
											.on_enter(Message::Hover(i))
											.on_press(Message::Select),
											move || {
												container(
													menu_entry(
														star(),
														if favorite {
															"Remove from favorites"
														} else {
															"Add to favorites"
														},
														"",
													)
													.on_press(Message::ToggleFavorite(option)),
												)
												.width(200)
												.style(container_with_radius(
													weaker_bordered_box,
													5,
												))
												.into()
											},
										)
										.into()
									})
								})
						)
						.width(Fill)
						.padding(5)
					)
					.width(FillPortion(3))
					.height(Fill)
					.direction(scrollable::Direction::Vertical(
						scrollable::Scrollbar::hidden(),
					))
					.style(scrollable_with_container(
						scrollable::default,
						container_with_radius(weaker_bordered_box, 5)
					))
				]
				.height(Shrink)
				.spacing(10)
			]
			.spacing(10)
			.padding(10),
		)
		.width(720)
		.spacing(5)
		.style(scrollable_with_container(
			scrollable_style,
			container_with_radius(weakest_bordered_box, 10),
		))
		.into()
	}

	/// The message of a shortcut that applies to the plugin picker.
	pub fn keybinds(action: ShortcutAction) -> Option<daw::Message> {
		match action {
			ShortcutAction::ClosePluginPicker => Some(daw::Message::TogglePluginPicker),
			ShortcutAction::SelectPlugin => Some(daw::Message::PluginPicker(Message::Select)),
			ShortcutAction::HoverPrevious => Some(daw::Message::PluginPicker(Message::HoverPrev)),
			ShortcutAction::HoverNext => Some(daw::Message::PluginPicker(Message::HoverNext)),
			_ => None,
		}
	}

	fn search(&mut self, state: &State) {
		let query = self
			.query
			.unicode_words()
			.map(str::to_lowercase)
			.collect::<Vec<_>>();

		self.filtered_options.clear();
		self.filtered_options.extend(
			state
				.options
				.iter()
				.zip(&state.matchers)
				.enumerate()
				.filter(|(_, (descriptor, _))| {
					self.categories.is_empty()
						|| self
							.categories
							.iter()
							.any(|&category| descriptor.in_category(category))
				})
				.filter(|(_, (descriptor, _))| {
					self.subcategories.is_empty()
						|| self
							.subcategories
							.iter()
							.any(|&subcategory| descriptor.in_subcategory(subcategory))
				})
				.filter(|(i, _)| !self.favorites_only || state.memory.is_favorite(&state.keys[*i]))
				.filter(|(_, (_, (name, vendor)))| {
					query.iter().all(|word| {
						name.contains(word)
							|| vendor
								.as_deref()
								.is_some_and(|vendor| vendor.contains(word))
					})
				})
				.map(|(i, _)| i),
		);

		// where every plugin was loaded last, the more recently used one first
		let ranks: HashMap<&PluginKey, usize> = state
			.memory
			.recent
			.iter()
			.enumerate()
			.map(|(i, key)| (key, i))
			.collect();

		let rank = |key: &PluginKey| ranks.get(key).copied().unwrap_or(usize::MAX);

		// the favorites come first, then the plugins that were used last, and then the
		// rest, by name
		self.filtered_options.sort_by(|&a, &b| {
			let a_key = &state.keys[a];
			let b_key = &state.keys[b];

			state
				.memory
				.is_favorite(b_key)
				.cmp(&state.memory.is_favorite(a_key))
				.then_with(|| rank(a_key).cmp(&rank(b_key)))
				.then_with(|| {
					natural_cmp(
						state.options[a].name().as_bytes(),
						state.options[b].name().as_bytes(),
					)
				})
				.then_with(|| {
					state.options[a]
						.format_rank()
						.cmp(&state.options[b].format_rank())
				})
		});
	}

	fn hovered_option(&self) -> usize {
		self.hovered_option
			.min(self.filtered_options.len().saturating_sub(1))
	}
}

fn build_matcher(keyword: &str) -> Box<str> {
	keyword
		.unicode_words()
		.flat_map(str::chars)
		.flat_map(char::to_lowercase)
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use generic_daw_core::clap_host;
	use std::{ffi::CStr, path::Path};

	fn clap(id: &str, name: &str, path: &str) -> PluginDescriptor {
		let mut id = id.as_bytes().to_vec();
		id.push(0);

		PluginDescriptor::Clap(clap_host::PluginDescriptor {
			id: Arc::from(CStr::from_bytes_with_nul(&id).unwrap()),
			name: name.into(),
			vendor: None,
			version: None,
			features: Arc::from([clap_host::Feature::Category(Category::AudioEffect)]),
			path: Arc::from(Path::new(path)),
		})
	}

	fn state_with(memory: PluginMemory) -> State {
		let mut state = State {
			memory,
			..State::default()
		};

		state.load_from_memory();

		state
	}

	#[test]
	fn plugins_that_are_remembered_are_there_right_away() {
		let descriptor = clap("com.example.plugin", "plugin", "plugin.clap");

		let mut memory = PluginMemory::default();
		memory.record_scan(&descriptor);

		let state = state_with(memory);

		assert_eq!(state.options.len(), 1);
		assert_eq!(state.keys, vec![PluginKey::from_descriptor(&descriptor)]);
	}

	#[test]
	fn plugins_that_moved_are_found_again() {
		let old = clap("com.example.plugin", "plugin", "old/plugin.clap");
		let new = clap("com.example.plugin", "plugin", "new/plugin.clap");

		let mut memory = PluginMemory::default();
		memory.record_scan(&old);

		let mut state = state_with(memory);
		state.start_scan();
		state.add(new.clone(), None);

		assert_eq!(state.options.len(), 1);
		assert_eq!(state.keys[0], PluginKey::from_descriptor(&new));
	}

	#[test]
	fn plugins_that_are_gone_are_taken_out() {
		let gone = clap("com.example.gone", "gone", "gone.clap");
		let here = clap("com.example.here", "here", "here.clap");

		let mut memory = PluginMemory::default();
		memory.record_scan(&gone);
		memory.record_scan(&here);

		let mut state = state_with(memory);
		state.start_scan();
		state.add(here.clone(), None);
		state.finish_scan(None);

		assert_eq!(state.options.len(), 1);
		assert_eq!(state.options[0].name(), "here");
	}

	#[test]
	fn plugins_of_different_formats_are_different_options() {
		let clap = clap("com.example.plugin", "plugin", "plugin.clap");
		let vst3 = PluginDescriptor::Vst3(generic_daw_core::vst3_host::PluginDescriptor {
			id: [0; 16],
			name: "plugin".into(),
			vendor: None,
			version: None,
			subcategories: Arc::from([]),
			path: Arc::from(Path::new("plugin.vst3")),
		});

		let mut state = State::default();
		state.add(clap, None);
		state.add(vst3, None);

		assert_eq!(state.options.len(), 2);
	}

	#[test]
	fn favorites_are_remembered() {
		let descriptor = clap("com.example.plugin", "plugin", "plugin.clap");

		let mut state = State::default();
		state.add(descriptor.clone(), None);

		assert!(state.toggle_favorite(0));
		assert!(
			state
				.memory
				.is_favorite(&PluginKey::from_descriptor(&descriptor))
		);
		assert!(!state.toggle_favorite(0));
		assert!(
			!state
				.memory
				.is_favorite(&PluginKey::from_descriptor(&descriptor))
		);
	}

	#[test]
	fn the_favorites_come_first_then_the_plugins_used_last() {
		let favorite = clap("com.example.favorite", "a favorite", "favorite.clap");
		let used = clap("com.example.used", "b used", "used.clap");
		let rest = clap("com.example.rest", "c rest", "rest.clap");

		let mut state = State::default();
		state.add(favorite.clone(), None);
		state.add(used.clone(), None);
		state.add(rest.clone(), None);

		state.toggle_favorite(0);
		state.mark_used(&used);

		let (mut picker, _) = PluginPicker::create(&state);
		picker.search(&state);

		let names = picker
			.filtered_options
			.iter()
			.map(|&i| state.options[i].name())
			.collect::<Vec<_>>();

		assert_eq!(names, vec!["a favorite", "b used", "c rest"]);
	}
}
