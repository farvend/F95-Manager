use eframe::egui::{TextEdit, Ui};

/// Простой поиск: только поле ввода без верхней панели/режимов.
/// Возвращает true, если текст изменился в этом кадре (для дебаунса).
pub fn search_with_mode(ui: &mut Ui, text: &mut String) -> bool {
    let w = ui.available_width();
    let resp = ui.add_sized(
        [w, 0.0],
        TextEdit::singleline(text)
            .hint_text(crate::localization::translate("filters-search-placeholder")),
    );
    resp.changed()
}
