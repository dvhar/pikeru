use iced::{
    color, Border, border::Radius,
    widget::button::{Appearance as ButAppearance, StyleSheet as ButStyle},
    Color,
    gradient, Radians,
};

struct TopButTheme;
impl ButStyle for TopButTheme {
    type Style = iced::Theme;

    fn active(&self, _style: &Self::Style) -> ButAppearance {
        gradbut(color!(0x232323), color!(0x303030), color!(0xdddddd), Radians(0.0))
    }

    fn hovered(&self, _style: &Self::Style) -> ButAppearance {
        gradbut(color!(0x563656), color!(0x404060), color!(0xeeeeee), Radians(0.0))
    }

}

struct SideButTheme;
impl ButStyle for SideButTheme {
    type Style = iced::Theme;

    fn active(&self, _style: &Self::Style) -> ButAppearance {
        gradbut(color!(0x262626), color!(0x2c2c28), color!(0xdddddd), Radians(280.0))
    }

    fn hovered(&self, _style: &Self::Style) -> ButAppearance {
        gradbut(color!(0x563656), color!(0x404068), color!(0xeeeeee), Radians(280.0))
    }

}

fn gradbut(c1: Color, c2: Color, txt: Color, rad: Radians) -> ButAppearance {
    let mut appearance = ButAppearance::default();
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
