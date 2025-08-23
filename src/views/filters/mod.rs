pub mod render;
pub mod items;
pub use render::draw_filters_panel;

pub trait EnumWithAlternativeNames {
    fn alternative_name(&self) -> &'static str;
}