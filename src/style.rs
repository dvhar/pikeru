use iced::{
    color, Background, Border, border::Radius,
    widget::button::{Appearance as ButAppearance, StyleSheet as ButStyle},
    Color,
};

struct ButTheme;
impl ButStyle for ButTheme {
    type Style = iced::Theme;

    fn active(&self, _style: &Self::Style) -> ButAppearance {
        let mut appearance = ButAppearance::default();
        appearance.background = Some(Background::Color(color!(0x302820)));
        appearance.text_color = color!(0xdddddd);
        appearance.border = border(color!(0x383838));
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
pub fn get_but_theme() -> iced::theme::Button {
    iced::theme::Button::Custom(
        Box::new(ButTheme) as Box<dyn ButStyle<Style = iced::Theme>>
    )
}

fn border(color: Color) -> Border {
    Border {
        color,
        width: 1.0,
        radius: Radius::from(4.0),
    }
}
