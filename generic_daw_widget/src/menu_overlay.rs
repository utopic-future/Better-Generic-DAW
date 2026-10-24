use iced_widget::{
	Renderer,
	core::{
		Element, Event, Layout, Point, Rectangle, Shell, Size, Theme, Vector, keyboard,
		layout::{Limits, Node},
		mouse::{self, Cursor, Interaction},
		overlay,
		renderer::Style,
		widget::{Operation, tree::Tree},
	},
};

/// Where a menu is attached to.
#[derive(Clone, Copy, Debug)]
pub enum Anchor {
	/// The top left corner of the menu is at this point.
	TopLeft(Point),
	/// The top right corner of the menu is at this point.
	///
	/// The menu opens to the left of the point when there isn't room to the right.
	TopRight(Point),
}

impl Anchor {
	/// The point the menu is attached to.
	fn point(self) -> Point {
		match self {
			Self::TopLeft(point) | Self::TopRight(point) => point,
		}
	}
}

/// A menu that floats above the rest of the interface.
pub struct MenuOverlay<'a, 'b, Message> {
	/// The contents of the menu.
	pub content: &'b mut Element<'a, Message, Theme, Renderer>,
	/// The tree of the contents of the menu.
	pub tree: &'b mut Tree,
	/// Where the menu is, if it is open.
	pub state: &'b mut Option<Point>,
	/// Where the menu is attached to.
	pub anchor: Anchor,
	/// The bounds of the thing that opened the menu.
	///
	/// The menu closes when the pointer leaves both the menu and this, which is
	/// how a submenu gets out of the way when the pointer moves to another item.
	pub parent: Option<Rectangle>,
}

impl<Message> overlay::Overlay<Message, Theme, Renderer> for MenuOverlay<'_, '_, Message> {
	fn layout(&mut self, renderer: &Renderer, bounds: Size) -> Node {
		let anchor = self.anchor.point();

		// the menu can be as big as the biggest of the two sides of the anchor
		let max_width = anchor.x.max(bounds.width - anchor.x);
		let max_height = anchor.y.max(bounds.height - anchor.y);

		let mut layout = self.content.as_widget_mut().layout(
			self.tree,
			renderer,
			&Limits::new(Size::ZERO, Size::new(max_width, max_height)),
		);

		let width = layout.bounds().width;

		let x = match self.anchor {
			Anchor::TopLeft(point) => point.x,
			Anchor::TopRight(point) => {
				// open to the left of the point when there isn't room to the right
				if point.x + width > bounds.width && point.x - width >= 0.0 {
					point.x - width
				} else {
					point.x
				}
			}
		};

		layout = layout.move_to(Point::new(x, anchor.y));

		// keep the menu on the screen
		if bounds.width < layout.bounds().x + layout.bounds().width {
			layout.translate_mut(Vector::new(
				bounds.width - (layout.bounds().x + layout.bounds().width),
				0.0,
			));
		}

		if bounds.height < layout.bounds().y + layout.bounds().height {
			layout.translate_mut(Vector::new(
				0.0,
				bounds.height - (layout.bounds().y + layout.bounds().height),
			));
		}

		layout
	}

	fn draw(
		&self,
		renderer: &mut Renderer,
		theme: &Theme,
		style: &Style,
		layout: Layout<'_>,
		cursor: Cursor,
	) {
		self.content.as_widget().draw(
			self.tree,
			renderer,
			theme,
			style,
			layout,
			cursor,
			&layout.bounds(),
		);
	}

	fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
		self.content
			.as_widget_mut()
			.operate(self.tree, layout, renderer, operation);
	}

	fn update(
		&mut self,
		event: &Event,
		layout: Layout<'_>,
		cursor: Cursor,
		renderer: &Renderer,
		shell: &mut Shell<'_, Message>,
	) {
		let was_event_captured = shell.is_event_captured();

		self.content.as_widget_mut().update(
			self.tree,
			event,
			layout,
			cursor,
			renderer,
			shell,
			&layout.bounds(),
		);

		if was_event_captured {
			return;
		}

		match event {
			Event::Mouse(mouse::Event::ButtonPressed { .. }) => {
				if cursor.is_over(layout.bounds()) {
					shell.capture_event();
				} else {
					*self.state = None;
					shell.request_redraw();
				}
			}
			Event::Mouse(mouse::Event::ButtonReleased { .. })
				if shell.is_event_captured() && cursor.is_over(layout.bounds()) =>
			{
				*self.state = None;
				shell.request_redraw();
			}
			Event::Mouse(mouse::Event::CursorMoved { .. }) => {
				// a submenu gets out of the way when the pointer moves to another item
				if let Some(parent) = self.parent
					&& let Some(position) = cursor.position()
					&& !layout.bounds().contains(position)
					&& !parent.contains(position)
				{
					*self.state = None;
					shell.request_redraw();
				}
			}
			Event::Mouse(mouse::Event::WheelScrolled { .. }) => {
				shell.capture_event();
			}
			Event::Keyboard(keyboard::Event::KeyPressed {
				key: keyboard::Key::Named(keyboard::key::Named::Escape),
				..
			}) => {
				*self.state = None;
				shell.capture_event();
				shell.request_redraw();
			}
			_ => {}
		}
	}

	fn mouse_interaction(
		&self,
		layout: Layout<'_>,
		cursor: Cursor,
		renderer: &Renderer,
	) -> Interaction {
		let interaction = self.content.as_widget().mouse_interaction(
			self.tree,
			layout,
			cursor,
			&layout.bounds(),
			renderer,
		);

		if interaction == Interaction::default() && cursor.is_over(layout.bounds()) {
			Interaction::Idle
		} else {
			interaction
		}
	}

	fn overlay<'a>(
		&'a mut self,
		layout: Layout<'a>,
		renderer: &Renderer,
	) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
		self.content.as_widget_mut().overlay(
			self.tree,
			layout,
			renderer,
			&layout.bounds(),
			Vector::ZERO,
		)
	}
}
