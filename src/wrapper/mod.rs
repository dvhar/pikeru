use std::fmt::Debug;
use std::vec;

use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{self, layout, mouse, overlay, renderer, Layout};
use iced::event::Status;
use iced::{Element, Point, Rectangle, Size, Vector};

pub struct Wrapper<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    content: Element<'a, Message, Theme, Renderer>,
    id: Option<iced::advanced::widget::Id>,
    on_info: Option<Box<dyn Fn(Rectangle, Rectangle) -> Message + 'a>>,
}

impl<'a, Message, Theme, Renderer> Wrapper<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    /// Creates a new [`Wrapper`].
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
            id: None,
            on_info: None,
        }
    }

    /// Sets the unique identifier of the [`Wrapper`].
    pub fn id(mut self, id: iced::advanced::widget::Id) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the message that will be produced when the [`Wrapper`] is clicked.
    pub fn on_info<F>(mut self, message: F) -> Self
    where
        F: Fn(Rectangle, Rectangle) -> Message + 'a,
    {
        self.on_info = Some(Box::new(message));
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Wrapper<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn state(&self) -> iced::advanced::widget::tree::State {
        advanced::widget::tree::State::new(State::default())
    }

    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        advanced::widget::tree::Tag::of::<State>()
    }

    fn children(&self) -> Vec<iced::advanced::widget::Tree> {
        vec![advanced::widget::Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut iced::advanced::widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.content))
    }

    fn size(&self) -> iced::Size<iced::Length> {
        self.content.as_widget().size()
    }

    fn on_event(
        &mut self,
        tree: &mut iced::advanced::widget::Tree,
        event: iced::Event,
        layout: iced::advanced::Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        _viewport: &iced::Rectangle,
    ) -> iced::advanced::graphics::core::event::Status {
        // handle the on event of the content first, in case that the wrapper is nested
        let status = self.content.as_widget_mut().on_event(
            &mut tree.children[0],
            event.clone(),
            layout,
            cursor,
            _renderer,
            _clipboard,
            shell,
            _viewport,
        );
        // this should really only be captured if the wrapper is nested or it contains some other
        // widget that captures the event
        if status == Status::Captured {
            return status;
        };
        if let Some(on_info) = self.on_info.as_deref() {
            let message = (on_info)(layout.bounds(), *_viewport);
            shell.publish(message);
        }
        status
    }

    fn layout(
        &self,
        tree: &mut iced::advanced::widget::Tree,
        renderer: &Renderer,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        let content_node = self
            .content
            .as_widget()
            .layout(&mut tree.children[0], renderer, limits);
        content_node
    }

    fn operate(
        &self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation<Message>,
    ) {
        let state = tree.state.downcast_mut::<State>();
        operation.custom(state, self.id.as_ref());
        operation.container(self.id.as_ref(), layout.bounds(), &mut |operation| {
            self.content
                .as_widget()
                .operate(&mut tree.children[0], layout, renderer, operation);
        });
    }

    fn draw(
        &self,
        tree: &iced::advanced::widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: iced::advanced::Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            &viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        _translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let mut children = tree.children.iter_mut();
        self.content.as_widget_mut().overlay(
            children.next().unwrap(),
            layout,
            renderer,
            _translation,
        )
    }

    fn mouse_interaction(
        &self,
        tree: &iced::advanced::widget::Tree,
        layout: iced::advanced::Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        _viewport: &iced::Rectangle,
        _renderer: &Renderer,
    ) -> iced::advanced::mouse::Interaction {
        let child_interact = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            _viewport,
            _renderer,
        );
        if child_interact != mouse::Interaction::default() {
            return child_interact;
        }
        mouse::Interaction::default()
    }
}

impl<'a, Message, Theme, Renderer> From<Wrapper<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(
        wrapper: Wrapper<'a, Message, Theme, Renderer>,
    ) -> Element<'a, Message, Theme, Renderer> {
        Element::new(wrapper)
    }
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct State {
    widget_pos: Point,
    overlay_bounds: Rectangle,
    action: Action,
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub enum Action {
    #[default]
    None,
}

#[allow(dead_code)]
struct Overlay<'a, 'b, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    content: &'b Element<'a, Message, Theme, Renderer>,
    tree: &'b mut advanced::widget::Tree,
    overlay_bounds: Rectangle,
}

impl<'a, 'b, Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for Overlay<'a, 'b, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, _bounds: Size) -> layout::Node {
        Widget::<Message, Theme, Renderer>::layout(
            self.content.as_widget(),
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, self.overlay_bounds.size()),
        )
        .move_to(self.overlay_bounds.position())
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        inherited_style: &renderer::Style,
        layout: Layout<'_>,
        cursor_position: mouse::Cursor,
    ) {
        Widget::<Message, Theme, Renderer>::draw(
            self.content.as_widget(),
            self.tree,
            renderer,
            theme,
            inherited_style,
            layout,
            cursor_position,
            &Rectangle::with_size(Size::INFINITY),
        );
    }

    fn is_over(&self, _layout: Layout<'_>, _renderer: &Renderer, _cursor_position: Point) -> bool {
        false
    }
}

// use like this to get position info about a widget:
// wrapper::wrapper(some_widget).on_info(Message::PositionInfo)
// where the message is PositionInfo(Point, Rectangle),
pub fn locator<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Wrapper<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    Wrapper::new(content)
}

