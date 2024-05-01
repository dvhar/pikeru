use iced::{
    color, Background, Border, border::Radius,
    widget::button::{Appearance as ButAppearance, StyleSheet as ButStyle},
    Color,
    gradient, Radians,
    Shadow,
    Vector,
};

struct TopButTheme;
impl ButStyle for TopButTheme {
    type Style = iced::Theme;

    fn active(&self, _style: &Self::Style) -> ButAppearance {
        let mut appearance = ButAppearance::default();
        let gradient = gradient::Linear::new(Radians(0.0))
            .add_stop(0.0, color!(0x232323))
            .add_stop(1.0, color!(0x303030))
            .into();
        appearance.background = Some(iced::Background::Gradient(gradient));
        appearance.text_color = color!(0xdddddd);
        appearance.border = border(color!(0x383838));
        appearance.shadow = Shadow {
            color: color!(0xa02020),
            offset: Vector::new(8.0, 8.0),
            blur_radius: 18.0,
        };
        appearance
    }

    fn hovered(&self, _style: &Self::Style) -> ButAppearance {
        let mut appearance = ButAppearance::default();
        appearance.background = Some(Background::Color(color!(0x454035)));
        appearance.text_color = color!(0xdddddd);
        appearance.border = border(color!(0x383838));
        appearance
    }

}

struct SideButTheme;
impl ButStyle for SideButTheme {
    type Style = iced::Theme;

    fn active(&self, _style: &Self::Style) -> ButAppearance {
        let mut appearance = ButAppearance::default();
        let gradient = gradient::Linear::new(Radians(280.0))
            .add_stop(0.0, color!(0x262626))
            .add_stop(1.0, color!(0x282828))
            .into();
        appearance.background = Some(iced::Background::Gradient(gradient));
        appearance.text_color = color!(0xdddddd);
        appearance.border = border(color!(0x383838));
        appearance.shadow = Shadow {
            color: color!(0xa02020),
            offset: Vector::new(8.0, 8.0),
            blur_radius: 18.0,
        };
        appearance
    }

    fn hovered(&self, _style: &Self::Style) -> ButAppearance {
        let mut appearance = ButAppearance::default();
        appearance.background = Some(Background::Color(color!(0x454035)));
        appearance.text_color = color!(0xdddddd);
        appearance.border = border(color!(0x383838));
        appearance
    }

}

fn border(color: Color) -> Border {
    Border {
        color,
        width: 1.0,
        radius: Radius::from(4.0),
    }
}

pub fn top_but_theme() -> iced::theme::Button {
    iced::theme::Button::Custom(
        Box::new(TopButTheme) as Box<dyn ButStyle<Style = iced::Theme>>
    )
}
pub fn side_but_theme() -> iced::theme::Button {
    iced::theme::Button::Custom(
        Box::new(SideButTheme) as Box<dyn ButStyle<Style = iced::Theme>>
    )
}
