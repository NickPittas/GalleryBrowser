use gallerybrowser_core::FilterScope;
use gallerybrowser_library::{NamedCount, RatingFilter};

use crate::state::{Action, AppState};
use crate::theme;

/// Scope switch (Folder/Library), rating filter, tags (with counts, click toggles filter, context:
/// delete) and collections. Pushes SetQuery/SetScope/...
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let (tags, collections) = cached_lists(ui, state);
    egui::ScrollArea::vertical()
        .id_salt("sidebar_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            scope_section(ui, state);
            ui.add_space(6.0);
            rating_section(ui, state);
            ui.add_space(6.0);
            tags_section(ui, state, &tags);
            ui.add_space(6.0);
            collections_section(ui, state, &collections);
        });
}

type Lists = (Vec<NamedCount>, Vec<NamedCount>);

/// Tag/collection lists are re-read only when the library changed (`library_generation`).
fn cached_lists(ui: &egui::Ui, state: &AppState) -> Lists {
    let id = egui::Id::new("sidebar_lists");
    let generation = state.library_generation;
    let cached: Option<(u64, Lists)> = ui.data(|d| d.get_temp(id));
    if let Some((g, lists)) = cached
        && g == generation
    {
        return lists;
    }
    let mut tags = state.library.tags();
    let mut collections = state.library.collections();
    tags.sort_by_key(|t| t.name.to_lowercase());
    collections.sort_by_key(|c| c.name.to_lowercase());
    let lists = (tags, collections);
    ui.data_mut(|d| d.insert_temp(id, (generation, lists.clone())));
    lists
}

fn scope_section(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("Scope").strong());
    let mut scope = state.pane().filters.scope;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut scope, FilterScope::Folder, "Folder");
        ui.selectable_value(&mut scope, FilterScope::Library, "Library");
    });
    if scope != state.pane().filters.scope {
        state.push(Action::SetScope(scope));
    }
}

fn rating_label(r: Option<RatingFilter>) -> String {
    match r {
        None => "Any rating".into(),
        Some(RatingFilter::Unrated) => "Unrated".into(),
        Some(RatingFilter::Exactly(n)) => format!("{} (exactly)", stars(n)),
        Some(RatingFilter::AtLeast(n)) => format!("{} & up", stars(n)),
    }
}

fn stars(n: u8) -> String {
    format!("{n}★")
}

fn rating_section(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("Rating").strong());
    let current = state.pane().filters.query.rating;
    let mut chosen = current;
    egui::ComboBox::from_id_salt("rating_filter")
        .width(ui.available_width() - 8.0)
        .selected_text(rating_label(current))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut chosen, None, "Any rating");
            ui.selectable_value(&mut chosen, Some(RatingFilter::Unrated), "Unrated");
            for n in 1..=5u8 {
                ui.selectable_value(
                    &mut chosen,
                    Some(RatingFilter::Exactly(n)),
                    rating_label(Some(RatingFilter::Exactly(n))),
                );
            }
            for n in 1..=4u8 {
                ui.selectable_value(
                    &mut chosen,
                    Some(RatingFilter::AtLeast(n)),
                    rating_label(Some(RatingFilter::AtLeast(n))),
                );
            }
        });
    if chosen != current {
        let mut q = state.pane().filters.query.clone();
        q.rating = chosen;
        state.push(Action::SetQuery(q));
    }
}

fn tags_section(ui: &mut egui::Ui, state: &mut AppState, tags: &[NamedCount]) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Tags").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("＋").on_hover_text("New tag").clicked() {
                state.dialogs.new_tag = Some(String::new());
            }
            if ui
                .add_enabled(
                    !state.pane().filters.query.tags.is_empty(),
                    egui::Button::new("All").small(),
                )
                .on_hover_text("Clear tag filter")
                .clicked()
            {
                let mut q = state.pane().filters.query.clone();
                q.tags.clear();
                state.push(Action::SetQuery(q));
            }
        });
    });
    if tags.is_empty() {
        ui.label(egui::RichText::new("No tags yet").color(theme::TEXT_MUTED));
    }
    for t in tags {
        let active = state.pane().filters.query.tags.contains(&t.name);
        let resp = ui.selectable_label(active, format!("{} ({})", t.name, t.count));
        if resp.clicked() {
            let mut q = state.pane().filters.query.clone();
            if active {
                q.tags.retain(|x| *x != t.name);
            } else {
                q.tags.push(t.name.clone());
            }
            state.push(Action::SetQuery(q));
        }
        resp.context_menu(|ui| {
            if ui.button("Delete Tag").clicked() {
                state.push(Action::DeleteTag(t.name.clone()));
                ui.close();
            }
        });
    }
}

fn collections_section(ui: &mut egui::Ui, state: &mut AppState, collections: &[NamedCount]) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Collections").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("＋")
                .on_hover_text("New collection")
                .clicked()
            {
                state.dialogs.new_collection = Some(String::new());
            }
            if ui
                .add_enabled(
                    state.pane().filters.query.collection.is_some(),
                    egui::Button::new("All").small(),
                )
                .on_hover_text("Clear collection filter")
                .clicked()
            {
                let mut q = state.pane().filters.query.clone();
                q.collection = None;
                state.push(Action::SetQuery(q));
            }
        });
    });
    if collections.is_empty() {
        ui.label(egui::RichText::new("No collections yet").color(theme::TEXT_MUTED));
    }
    for c in collections {
        let active = state.pane().filters.query.collection.as_deref() == Some(c.name.as_str());
        let resp = ui.selectable_label(active, format!("{} ({})", c.name, c.count));
        if resp.clicked() {
            let mut q = state.pane().filters.query.clone();
            q.collection = if active { None } else { Some(c.name.clone()) };
            state.push(Action::SetQuery(q));
        }
        resp.context_menu(|ui| {
            if ui.button("Delete Collection").clicked() {
                state.push(Action::DeleteCollection(c.name.clone()));
                ui.close();
            }
        });
    }
}
