use iced::{
    color, Background,
    widget::button::{Appearance as ButAppearance, StyleSheet as ButStyle},
};

struct ButTheme;
impl ButStyle for ButTheme {
    type Style = iced::Theme;

    fn active(&self, _style: &Self::Style) -> ButAppearance {
        let mut appearance = ButAppearance::default();
        appearance.background = Some(Background::Color(color!(0x403830)));
        appearance.text_color = color!(0xdddddd);
        appearance
    }

    fn hovered(&self, _style: &Self::Style) -> ButAppearance {
        let mut appearance = ButAppearance::default();
        appearance.background = Some(Background::Color(color!(0x555040)));
        appearance.text_color = color!(0xdddddd);
        appearance
    }

    fn pressed(&self, style: &Self::Style) -> ButAppearance {
        self.hovered(style)
    }
}
pub fn get_but_theme() -> iced::theme::Button {
    iced::theme::Button::Custom(
        Box::new(ButTheme) as Box<dyn ButStyle<Style = iced::Theme>>
    )
}
