use iced::{
    color, Border, border::Radius,
    Color,
    gradient, Radians,
    widget::button,
    widget::container,
    Background,
};

fn gradbut(c1: Color, c2: Color, txt: Color, rad: Radians) -> button::Style {
    let mut appearance = button::Style::default();
    let gradient = gradient::Linear::new(rad)
        .add_stop(0.0, c1)
        .add_stop(1.0, c2)
        .into();
    appearance.background = Some(iced::Background::Gradient(gradient));
    appearance.text_color = txt;
    appearance.border = border(color!(0x383838));
    appearance
}

fn border(color: Color) -> Border {
    Border {
        color,
        width: 1.0,
        radius: Radius::from(4.0),
    }
}

/// Gradient button style: dark gradient background with light text.
pub fn top_but_style() -> Box<dyn Fn(&iced::Theme, button::Status) -> button::Style + Send + Sync> {
    Box::new(move |_theme: &iced::Theme, status: button::Status| {
        match status {
            button::Status::Hovered | button::Status::Pressed => {
                gradbut(color!(0x563656), color!(0x404060), color!(0xeeeeee), Radians(0.0))
            }
            _ => gradbut(color!(0x232323), color!(0x303030), color!(0xdddddd), Radians(0.0)),
        }
    })
}

/// Gradient button style: purple-tinted on hover, with angle rotation.
pub fn side_but_style() -> Box<dyn Fn(&iced::Theme, button::Status) -> button::Style + Send + Sync> {
    Box::new(move |_theme: &iced::Theme, status: button::Status| {
        match status {
            button::Status::Hovered | button::Status::Pressed => {
                gradbut(color!(0x563656), color!(0x404068), color!(0xeeeeee), Radians(280.0))
            }
            _ => gradbut(color!(0x262626), color!(0x2c2c28), color!(0xdddddd), Radians(280.0)),
        }
    })
}

/// Flat button style: solid dark background with no gradient.
pub fn flat_but_style() -> Box<dyn Fn(&iced::Theme, button::Status) -> button::Style + Send + Sync> {
    Box::new(move |_theme: &iced::Theme, _status: button::Status| {
        let mut appearance = button::Style::default();
        appearance.background = Some(iced::Background::Color(color!(0x262626)));
        appearance.text_color = color!(0xdddddd);
        appearance
    })
}

/// Red close button: same gradient style as Open/Cancel buttons but with red text.
pub fn red_close_style() -> Box<dyn Fn(&iced::Theme, button::Status) -> button::Style + Send + Sync> {
    Box::new(move |_theme: &iced::Theme, status: button::Status| {
        match status {
            button::Status::Hovered | button::Status::Pressed => {
                gradbut(color!(0x563656), color!(0x404060), color!(0xff5555), Radians(0.0))
            }
            _ => gradbut(color!(0x232323), color!(0x303030), color!(0xff3333), Radians(0.0)),
        }
    })
}

/// Selected item container: red background.
pub fn selected_style() -> impl Fn(&iced::Theme) -> container::Style {
    |_theme: &iced::Theme| {
        let mut appearance = container::Style::default();
        appearance.background = Some(Background::Color(color!(0x990000)));
        appearance
    }
}
