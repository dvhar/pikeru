pub mod widget;

use iced::{
    advanced::{graphics::futures::MaybeSend, renderer, widget::Id},
    Element, Point, Rectangle,
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
) -> iced::Task<Message>
where
    Message: 'static + Send,
    MF: Fn(Vec<(Id, Rectangle)>) -> Message + MaybeSend + Sync + Clone + 'static,
{
    iced::advanced::widget::operate(drop::find_zones(
        move |bounds| bounds.contains(point),
        options,
        depth,
    ))
    .map(move |id| msg(id))
}

#[allow(dead_code)]
pub fn find_zones<Message, MF, F>(
    msg: MF,
    filter: F,
    options: Option<Vec<Id>>,
    depth: Option<usize>,
) -> iced::Task<Message>
where
    Message: 'static + Send,
    MF: Fn(Vec<(Id, Rectangle)>) -> Message + MaybeSend + Sync + Clone + 'static,
    F: Fn(&Rectangle) -> bool + 'static + Send,
{
    iced::advanced::widget::operate(drop::find_zones(filter, options, depth)).map(move |id| msg(id))
}
