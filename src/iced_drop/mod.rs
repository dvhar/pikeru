pub mod widget;

use iced::{
    advanced::{graphics::futures::MaybeSend, renderer, widget::Id},
    Command, Element, Point, Rectangle,
};

use widget::droppable::*;
use widget::operation::drop;

pub fn droppable<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Droppable<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    Droppable::new(content)
}

pub fn zones_on_point<Message, MF>(
    msg: MF,
    point: Point,
    options: Option<Vec<Id>>,
    depth: Option<usize>,
) -> Command<Message>
where
    Message: 'static,
    MF: Fn(Vec<(Id, Rectangle)>) -> Message + MaybeSend + Sync + Clone + 'static,
{
    Command::widget(drop::find_zones(
        move |bounds| bounds.contains(point),
        options,
        depth,
    ))
    .map(move |id| msg(id))
}

pub fn find_zones<Message, MF, F>(
    msg: MF,
    filter: F,
    options: Option<Vec<Id>>,
    depth: Option<usize>,
) -> Command<Message>
where
    Message: 'static,
    MF: Fn(Vec<(Id, Rectangle)>) -> Message + MaybeSend + Sync + Clone + 'static,
    F: Fn(&Rectangle) -> bool + 'static,
{
    Command::widget(drop::find_zones(filter, options, depth)).map(move |id| msg(id))
}
