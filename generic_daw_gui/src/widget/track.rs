use crate::widget::clip::Clip;
use iced::{
	Event, Fill, Length, Rectangle, Renderer, Size, Theme, Vector,
	advanced::{
		Layout, Shell, Widget,
		layout::{Limits, Node},
		mouse::{Cursor, Interaction},
		overlay,
		renderer::Style,
		widget::{Operation, Tree},
	},
};
use std::borrow::{Borrow, BorrowMut};

#[derive(Debug)]
pub struct Track<'a, Message> {
	pub(super) clips: Box<[Clip<'a, Message>]>,
}

impl<'a, Message: 'a> Widget<Message, Theme, Renderer> for Track<'a, Message> {
	fn diff(&mut self, tree: &mut Tree) {
		tree.diff_children(&mut self.clips);
	}

	fn size(&self) -> Size<Length> {
		Size::new(Fill, Fill)
	}

	fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
		let _viewport_x = 0.0; // Will be set properly by parent
		let _viewport_width = limits.max().width;
		
		Node::with_children(
			limits.max(),
			self.clips
				.iter_mut()
				.zip(&mut tree.children)
				.map(|(child, tree)| {
					// Clip layout is fast - just computes position from time
					child.layout(tree, renderer, limits)
				})
				.collect(),
		)
	}

	fn update(
		&mut self,
		tree: &mut Tree,
		event: &Event,
		layout: Layout<'_>,
		mut cursor: Cursor,
		renderer: &Renderer,
		shell: &mut Shell<'_, Message>,
		viewport: &Rectangle,
	) {
		// Compute visible clip range based on viewport
		let viewport_left = viewport.x - layout.position().x;
		let viewport_right = viewport_left + viewport.width;
		
		for (i, ((child, tree), clip_layout)) in self
			.clips
			.iter_mut()
			.zip(&mut tree.children)
			.zip(layout.children())
			.enumerate()
			.rev()
		{
			// Skip clips outside horizontal viewport
			let clip_bounds = clip_layout.bounds();
			if clip_bounds.x + clip_bounds.width < viewport_left || clip_bounds.x > viewport_right {
				continue;
			}
			
			child.update(tree, event, clip_layout, cursor, renderer, shell, viewport);

			if i != 0
				&& !cursor.is_levitating()
				&& child.mouse_interaction(tree, clip_layout, cursor, viewport, renderer)
					!= Interaction::default()
			{
				cursor = cursor.levitate();
			}
		}
	}

	fn draw(
		&self,
		_tree: &Tree,
		_renderer: &mut Renderer,
		_theme: &Theme,
		_style: &Style,
		_layout: Layout<'_>,
		_cursor: Cursor,
		_viewport: &Rectangle,
	) {
		panic!();
	}

	fn mouse_interaction(
		&self,
		tree: &Tree,
		layout: Layout<'_>,
		cursor: Cursor,
		viewport: &Rectangle,
		renderer: &Renderer,
	) -> Interaction {
		let viewport_left = viewport.x - layout.position().x;
		let viewport_right = viewport_left + viewport.width;
		
		self.clips
			.iter()
			.zip(&tree.children)
			.zip(layout.children())
			.filter_map(|((child, tree), clip_layout)| {
				// Skip clips outside horizontal viewport
				let clip_bounds = clip_layout.bounds();
				if clip_bounds.x + clip_bounds.width < viewport_left || clip_bounds.x > viewport_right {
					return None;
				}
				Some(child.mouse_interaction(tree, clip_layout, cursor, viewport, renderer))
			})
			.rfind(|&i| i != Interaction::default())
			.unwrap_or_default()
	}

	fn overlay<'b>(
		&'b mut self,
		tree: &'b mut Tree,
		layout: Layout<'b>,
		renderer: &Renderer,
		viewport: &Rectangle,
		translation: Vector,
	) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
		let viewport_left = viewport.x - layout.position().x;
		let viewport_right = viewport_left + viewport.width;
		
		let children = self
			.clips
			.iter_mut()
			.zip(&mut tree.children)
			.zip(layout.children())
			.filter_map(|((child, tree), clip_layout)| {
				// Skip clips outside horizontal viewport
				let clip_bounds = clip_layout.bounds();
				if clip_bounds.x + clip_bounds.width < viewport_left || clip_bounds.x > viewport_right {
					return None;
				}
				child.overlay(tree, clip_layout, renderer, viewport, translation)
			})
			.collect::<Vec<_>>();

		(!children.is_empty()).then(|| overlay::Group::with_children(children).overlay())
	}

	fn operate(
		&mut self,
		tree: &mut Tree,
		layout: Layout<'_>,
		renderer: &Renderer,
		operation: &mut dyn Operation,
	) {
		operation.container(None, layout.bounds());
		operation.traverse(&mut |operation| {
			self.clips
				.iter_mut()
				.zip(&mut tree.children)
				.zip(layout.children())
				.for_each(|((child, tree), clip_layout)| {
					// Skip clips with zero width (invalid layout)
					if clip_layout.bounds().width <= 0.0 {
						return;
					}
					child.operate(tree, clip_layout, renderer, operation);
				});
		});
	}
}

impl<'a, Message> Track<'a, Message> {
	pub fn new(children: impl IntoIterator<Item = Clip<'a, Message>>) -> Self {
		Self {
			clips: children.into_iter().collect(),
		}
	}
}

impl<'a, Message: 'a> Borrow<dyn Widget<Message, Theme, Renderer> + 'a> for Track<'a, Message> {
	fn borrow(&self) -> &(dyn Widget<Message, Theme, Renderer> + 'a) {
		self
	}
}

impl<'a, Message: 'a> BorrowMut<dyn Widget<Message, Theme, Renderer> + 'a> for Track<'a, Message> {
	fn borrow_mut(&mut self) -> &mut (dyn Widget<Message, Theme, Renderer> + 'a) {
		self
	}
}
