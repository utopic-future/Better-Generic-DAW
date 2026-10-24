use crate::{
	LazyElement,
	menu_overlay::{Anchor, MenuOverlay},
};
use iced_widget::{
	Renderer,
	core::{
		Element, Event, Layout, Length, Point, Rectangle, Shell, Size, Theme, Vector, Widget,
		layout::{Limits, Node},
		mouse::{self, Cursor, Interaction},
		overlay,
		renderer::Style,
		widget::{Operation, Tree, tree},
	},
};

/// A menu that opens when the pointer moves over it.
pub struct Submenu<'a, Message> {
	content: Element<'a, Message, Theme, Renderer>,
	menu: LazyElement<'a, Message, Theme, Renderer>,
}

struct State {
	/// Where the menu is, if it is open.
	position: Option<Point>,
}

impl<'a, Message> Submenu<'a, Message> {
	/// Creates a submenu.
	///
	/// The `content` is the item that opens the menu, and the `menu` is what the
	/// menu is made of.
	pub fn new(
		content: impl Into<Element<'a, Message, Theme, Renderer>>,
		menu: impl Fn() -> Element<'a, Message, Theme, Renderer> + 'a,
	) -> Self {
		Self {
			content: content.into(),
			menu: LazyElement::new(Box::new(menu)),
		}
	}
}

impl<Message> Widget<Message, Theme, Renderer> for Submenu<'_, Message> {
	fn tag(&self) -> tree::Tag {
		tree::Tag::of::<State>()
	}

	fn state(&self) -> tree::State {
		tree::State::new(State { position: None })
	}

	fn size(&self) -> Size<Length> {
		self.content.as_widget().size()
	}

	fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
		self.content
			.as_widget_mut()
			.layout(&mut tree.children[0], renderer, limits)
	}

	fn draw(
		&self,
		tree: &Tree,
		renderer: &mut Renderer,
		theme: &Theme,
		style: &Style,
		layout: Layout<'_>,
		cursor: Cursor,
		viewport: &Rectangle,
	) {
		self.content.as_widget().draw(
			&tree.children[0],
			renderer,
			theme,
			style,
			layout,
			cursor,
			viewport,
		);
	}

	fn diff(&mut self, tree: &mut Tree) {
		if tree.state.downcast_ref::<State>().position.is_some() {
			tree.diff_children(&mut [&mut self.content, &mut self.menu]);
		} else {
			tree.diff_children(&mut [&mut self.content]);
		}
	}

	fn operate(
		&mut self,
		tree: &mut Tree,
		layout: Layout<'_>,
		renderer: &Renderer,
		operation: &mut dyn Operation,
	) {
		self.content
			.as_widget_mut()
			.operate(&mut tree.children[0], layout, renderer, operation);
	}

	fn update(
		&mut self,
		tree: &mut Tree,
		event: &Event,
		layout: Layout<'_>,
		cursor: Cursor,
		renderer: &Renderer,
		shell: &mut Shell<'_, Message>,
		viewport: &Rectangle,
	) {
		self.content.as_widget_mut().update(
			&mut tree.children[0],
			event,
			layout,
			cursor,
			renderer,
			shell,
			viewport,
		);

		if shell.is_event_captured() {
			return;
		}

		// the menu opens when the pointer moves over the item
		if let Event::Mouse(mouse::Event::CursorMoved { .. }) = event
			&& let Some(position) = cursor.position()
			&& layout.bounds().contains(position)
			&& tree.state.downcast_ref::<State>().position.is_none()
		{
			tree.state.downcast_mut::<State>().position = Some(position);

			if tree.children.len() == 1 {
				tree.children.push(Tree::new(&*self.menu));
			}

			self.menu.as_widget_mut().diff(&mut tree.children[1]);

			shell.request_redraw();
		}
	}

	fn mouse_interaction(
		&self,
		tree: &Tree,
		layout: Layout<'_>,
		cursor: Cursor,
		viewport: &Rectangle,
		renderer: &Renderer,
	) -> Interaction {
		self.content.as_widget().mouse_interaction(
			&tree.children[0],
			layout,
			cursor,
			viewport,
			renderer,
		)
	}

	fn overlay<'b>(
		&'b mut self,
		tree: &'b mut Tree,
		layout: Layout<'b>,
		renderer: &Renderer,
		viewport: &Rectangle,
		translation: Vector,
	) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
		let state = tree.state.downcast_mut::<State>();

		if state.position.is_none() {
			return self.content.as_widget_mut().overlay(
				&mut tree.children[0],
				layout,
				renderer,
				viewport,
				translation,
			);
		}

		let [first, second] = &mut *tree.children else {
			unreachable!();
		};

		// the menu opens to the right of the item
		let anchor = Anchor::TopRight(Point::new(
			layout.bounds().x + layout.bounds().width,
			layout.bounds().y,
		));

		let menu = overlay::Element::new(Box::new(MenuOverlay {
			content: &mut self.menu,
			tree: second,
			state: &mut state.position,
			anchor,
			parent: Some(layout.bounds()),
		}));

		let Some(content_overlay) =
			self.content
				.as_widget_mut()
				.overlay(first, layout, renderer, viewport, translation)
		else {
			return Some(menu);
		};

		Some(overlay::Group::with_children(vec![content_overlay, menu]).overlay())
	}
}

impl<'a, Message: 'a> From<Submenu<'a, Message>> for Element<'a, Message, Theme, Renderer> {
	fn from(value: Submenu<'a, Message>) -> Self {
		Self::new(value)
	}
}
