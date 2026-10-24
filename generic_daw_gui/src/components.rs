use crate::{
	components::drag_handle::drag_handle,
	icons::{Icon, LUCIDE_FONT, check, chevron_down, chevron_right, chevron_up, rotate_ccw},
	stylefns::{button_with_radius, container_with_radius, weaker_bordered_box},
	widget::{LINE_HEIGHT, TEXT_HEIGHT},
};
use generic_daw_widget::{context_menu::ContextMenu, stateful::Stateful, submenu::Submenu};
use iced::{
	Element, Fill, Font, Theme, border,
	widget::{
		Button, button, column, container, pick_list, right, row, rule, sensor, space, text,
		text_input,
	},
};
use std::{cell::Cell, ops::RangeInclusive};

mod drag_handle;

/// An item of a menu.
#[derive(Clone, Debug)]
pub enum MenuItem<Message> {
	/// Something the user can do.
	Action {
		/// The icon of the item.
		icon: Option<Icon>,
		/// What the item is called.
		label: String,
		/// The shortcut of the item, if there is one.
		shortcut: String,
		/// What happens when the user picks the item.
		message: Message,
	},
	/// Something the user can turn on and off.
	Toggle {
		/// The icon of the item.
		icon: Option<Icon>,
		/// What the item is called.
		label: String,
		/// The shortcut of the item, if there is one.
		shortcut: String,
		/// Whether the thing is on.
		is_on: bool,
		/// What happens when the user picks the item.
		message: Message,
	},
	/// A line between the items.
	Separator,
	/// A menu inside the menu.
	Submenu {
		/// The icon of the item.
		icon: Option<Icon>,
		/// What the item is called.
		label: String,
		/// The items of the menu.
		items: Vec<Self>,
	},
}

pub fn file_tree_entry<'a, Message: 'a>(
	i: Icon,
	l: impl text::IntoFragment<'a>,
	style: impl Fn(&Theme, button::Status) -> button::Style + 'a,
) -> Button<'a, Message> {
	button(
		row![
			i.size(LINE_HEIGHT),
			text(l)
				.wrapping(text::Wrapping::None)
				.ellipsis(text::Ellipsis::End)
		]
		.padding(1)
		.spacing(2)
		.width(Fill),
	)
	.padding(0)
	.height(LINE_HEIGHT + 2.0)
	.style(style)
}

pub fn icon_button<'a, Message: 'a>(
	i: Icon,
	style: impl Fn(&Theme, button::Status) -> button::Style + 'a,
) -> Button<'a, Message> {
	button(i.size(13.0))
		.style(button_with_radius(style, 0))
		.padding(1)
}

/// A row of a menu, with an icon, a label, and something on the right.
pub fn menu_row<'a, Message: 'a>(
	icon: Option<Icon>,
	label: impl text::IntoFragment<'a>,
	right: impl Into<Element<'a, Message>>,
) -> Button<'a, Message> {
	button(
		row![
			icon.map_or_else(
				|| -> Element<'a, Message> { space().width(TEXT_HEIGHT).into() },
				|icon| -> Element<'a, Message> { icon.size(TEXT_HEIGHT).into() },
			),
			text(label).line_height(1.0),
			right.into(),
		]
		.spacing(5),
	)
	.style(button::text)
	.padding(5)
}

pub fn menu_entry<'a, Message: 'a>(
	i: impl Into<Option<Icon>>,
	l: impl text::IntoFragment<'a>,
	r: impl text::IntoFragment<'a>,
) -> Button<'a, Message> {
	menu_row(
		i.into(),
		l,
		right(text(r).line_height(1.0).style(text::secondary)),
	)
}

/// A line between the items of a menu.
pub fn menu_separator<'a, Message: 'a>() -> Element<'a, Message> {
	container(rule::horizontal(1))
		.padding(iced::padding::horizontal(5))
		.into()
}

/// A menu, with the items the user can pick.
pub fn menu<'a, Message: 'a + Clone>(items: Vec<MenuItem<Message>>) -> Element<'a, Message> {
	menu_width(items, 180.0)
}

/// A menu that is as wide as the user wants it to be.
pub fn menu_width<'a, Message: 'a + Clone>(
	items: Vec<MenuItem<Message>>,
	width: f32,
) -> Element<'a, Message> {
	container(column(
		items.into_iter().map(menu_item).collect::<Vec<_>>(),
	))
	.width(width)
	.style(container_with_radius(weaker_bordered_box, 5))
	.into()
}

/// One item of a menu.
pub fn menu_item<'a, Message: 'a + Clone>(item: MenuItem<Message>) -> Element<'a, Message> {
	match item {
		MenuItem::Action {
			icon,
			label,
			shortcut,
			message,
		} => menu_entry(icon, label, shortcut).on_press(message).into(),
		MenuItem::Toggle {
			icon,
			label,
			shortcut,
			is_on,
			message,
		} => menu_entry(
			if is_on { Some(check()) } else { icon },
			label,
			shortcut,
		)
		.on_press(message)
		.into(),
		MenuItem::Separator => menu_separator(),
		MenuItem::Submenu { icon, label, items } => Submenu::new(
			menu_row(
				icon,
				label,
				right(chevron_right().size(TEXT_HEIGHT)),
			),
			move || menu(items.clone()),
		)
		.into(),
	}
}

pub fn number_input<'a>(
	range: RangeInclusive<usize>,
	value: usize,
	default: usize,
	radius: impl Into<border::Radius>,
) -> Element<'a, Option<usize>> {
	let radius = radius.into();
	let max_digits = (range.end() + 1).ilog10();
	row![
		ContextMenu::new(
			drag_handle(range, value, default, radius.right(0)).map(Some),
			move || container(
				menu_entry(rotate_ccw(), "Reset", "Ctrl-Click").on_press(Some(default)),
			)
			.width(160)
			.style(container_with_radius(weaker_bordered_box, 5))
			.into()
		),
		text_input("", value)
			.style(move |t, s| {
				let mut style = text_input::default(t, s);
				style.border.radius = radius.left(0);
				style
			})
			.font(Font::MONOSPACE)
			.width(max_digits * 10 + 14)
			.on_input(|text| text.parse().ok())
	]
	.into()
}

pub fn pick_list_handle() -> pick_list::Handle {
	pick_list::Handle::Dynamic {
		closed: pick_list::Icon {
			font: LUCIDE_FONT,
			code_point: chevron_down().glyph(),
			size: None,
			line_height: None,
			shaping: text::Shaping::Basic,
		},
		open: pick_list::Icon {
			font: LUCIDE_FONT,
			code_point: chevron_up().glyph(),
			size: None,
			line_height: None,
			shaping: text::Shaping::Basic,
		},
	}
}

pub fn text_icon_button<'a, Message: 'a>(
	i: impl text::IntoFragment<'a>,
	style: impl Fn(&Theme, button::Status) -> button::Style + 'a,
) -> Button<'a, Message> {
	button(
		text(i)
			.size(13)
			.line_height(1.0)
			.width(13)
			.height(13)
			.center(),
	)
	.style(button_with_radius(style, 0))
	.padding(1)
}

pub fn virtualized<'a, Message: 'static>(
	f: impl Fn() -> Element<'a, Message> + 'a,
) -> Element<'a, Message> {
	let cache = f();
	let size = cache.as_widget().size();
	let cache = Cell::new(Some(cache));
	Stateful::new(
		|state, event: Option<_>| {
			*state ^= event.is_none();
			event
		},
		move |state| {
			sensor(if *state {
				cache.take().unwrap_or_else(&f).map(Some)
			} else {
				space().width(size.width).height(size.height).into()
			})
			.on_show(|_| None)
			.on_hide(None)
			.into()
		},
	)
	.into()
}
