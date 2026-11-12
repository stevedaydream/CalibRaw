use crate::app::CalibRawApp;
use crate::ui::{glass, photo_gestures, sidebar::Sidebar, top_bar::TopBar};
use eframe::egui::{self, CornerRadius, Rect, Sense, Ui};
use moduwu_design::ScreenLayout;

const HANDLE_HEIGHT: f32 = 28.0;
const COLLAPSED_HEIGHT: f32 = 144.0;
/// Release speed, in points per second, from which a fling moves the sheet
/// to the next detent in its direction instead of the nearest one.
const FLING_SPEED: f32 = 650.0;
/// Seconds the sheet takes to settle on a detent.
const SETTLE_SECONDS: f32 = 0.22;
/// Gap between the top controls and the top of the safe area, in points.
const TOP_GAP: f32 = 4.0;

fn max_sheet_height(viewport_height: f32) -> f32 {
    (viewport_height * 0.65).max(1.0)
}

fn sheet_height(viewport_height: f32, requested: f32) -> f32 {
    let maximum = max_sheet_height(viewport_height);
    requested.clamp(COLLAPSED_HEIGHT.min(maximum), maximum)
}

/// Collapsed, half and full sheet heights, ascending.
fn detents(viewport_height: f32) -> [f32; 3] {
    let full = max_sheet_height(viewport_height);
    let half = sheet_height(viewport_height, (viewport_height * 0.45).max(360.0)).min(full);
    [COLLAPSED_HEIGHT.min(full), half, full]
}

/// The detent a released drag settles on: a fling moves one detent in its
/// direction, a slow release snaps to the nearest one.
fn settle(viewport_height: f32, height: f32, velocity_down: f32) -> f32 {
    let stops = detents(viewport_height);
    if velocity_down > FLING_SPEED {
        return stops
            .iter()
            .rev()
            .copied()
            .find(|stop| *stop < height - 1.0)
            .unwrap_or(stops[0]);
    }
    if velocity_down < -FLING_SPEED {
        return stops
            .iter()
            .copied()
            .find(|stop| *stop > height + 1.0)
            .unwrap_or(stops[2]);
    }
    stops
        .iter()
        .copied()
        .min_by(|a, b| (a - height).abs().total_cmp(&(b - height).abs()))
        .unwrap_or(height)
}

/// The photo fills the safe area and the tools float over it on glass.
/// The photo is laid out in a region that does not depend on the sheet's
/// height, so dragging the sheet never moves the photo, and gestures only
/// start on the photo where no surface covers it.
pub(crate) fn show(ui: &mut Ui, app: &mut CalibRawApp, frame: &eframe::Frame) {
    // A phone held sideways puts the tools in a rail beside the photo.
    if app.caps().is_mobile() && ui.available_width() >= ui.available_height() {
        show_landscape(ui, app, frame);
        return;
    }
    let canvas = ui.available_rect_before_wrap();
    let context = ui.ctx().clone();
    let top = top_controls_rect(canvas);
    let height_id = egui::Id::new("portrait-tool-sheet-height");
    let drag_id = egui::Id::new("portrait-tool-sheet-drag");
    let animation_id = egui::Id::new("portrait-tool-sheet-animation");
    let default_height = detents(canvas.height())[1];
    // The stored height is the user's expanded size. A closed sidebar collapses
    // the sheet to its tab bar without forgetting that size.
    let requested = context
        .data(|data| data.get_temp::<f32>(height_id))
        .filter(|height| *height > COLLAPSED_HEIGHT + 1.0)
        .unwrap_or(default_height);
    let dragged_height = context.data(|data| data.get_temp::<f32>(drag_id));
    let target = if app.develop_ui.sidebar_open {
        sheet_height(canvas.height(), requested)
    } else {
        sheet_height(canvas.height(), COLLAPSED_HEIGHT)
    };
    let mut height = match dragged_height {
        // Follow the finger exactly, and start the settle from here.
        Some(height) => context.animate_value_with_time(animation_id, height, 0.0),
        None => context.animate_value_with_time(animation_id, target, SETTLE_SECONDS),
    };

    show_top_controls(&context, top, app, frame, TopBar::show_portrait);

    let inner = canvas.shrink2(egui::vec2(glass::MARGIN, 0.0));
    let sheet = Rect::from_min_max(
        egui::pos2(inner.left(), inner.bottom() - glass::MARGIN - height),
        egui::pos2(inner.right(), inner.bottom() - glass::MARGIN),
    );
    egui::Area::new(egui::Id::new("portrait-tool-sheet"))
        .order(egui::Order::Middle)
        .fixed_pos(sheet.min)
        .movable(false)
        .constrain(false)
        .show(&context, |ui| {
            ui.set_min_size(sheet.size());
            ui.set_max_size(sheet.size());
            glass::paint(ui, sheet, glass::floating_corners());
            glass::translucent_contents(ui);
            ui.set_clip_rect(sheet);
            let handle = Rect::from_min_size(sheet.min, egui::vec2(sheet.width(), HANDLE_HEIGHT));
            let response = ui
                .interact(handle, height_id, Sense::click_and_drag())
                .on_hover_cursor(egui::CursorIcon::ResizeVertical);
            ui.painter().rect_filled(
                Rect::from_center_size(handle.center(), egui::vec2(40.0, 5.0)),
                3.0,
                ui.visuals().text_color().gamma_multiply(0.45),
            );
            if response.dragged() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    height = sheet_height(
                        canvas.height(),
                        sheet.bottom() - pointer.y + HANDLE_HEIGHT * 0.5,
                    );
                    context.data_mut(|data| data.insert_temp(drag_id, height));
                }
            } else if response.drag_stopped() {
                let velocity = ui.input(|input| input.pointer.velocity().y);
                let settled = settle(canvas.height(), height, velocity);
                app.develop_ui.sidebar_open = settled > COLLAPSED_HEIGHT + 1.0;
                context.data_mut(|data| {
                    data.remove::<f32>(drag_id);
                    if settled > COLLAPSED_HEIGHT + 1.0 {
                        data.insert_temp(height_id, settled);
                    }
                });
            } else if response.clicked() {
                app.develop_ui.sidebar_open = !app.develop_ui.sidebar_open;
            }
            let content = Rect::from_min_max(
                egui::pos2(sheet.left() + 4.0, handle.bottom()),
                egui::pos2(
                    sheet.right() - 4.0,
                    sheet.bottom() - f32::from(glass::CORNER) * 0.4,
                ),
            );
            let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(content));
            content_ui.set_clip_rect(content);
            if Sidebar::shows_mask_strip(app) && height > COLLAPSED_HEIGHT + 100.0 {
                egui::Panel::top("portrait-mask-strip")
                    .resizable(false)
                    .exact_size(Sidebar::VERTICAL_MASK_STRIP_HEIGHT)
                    .frame(egui::Frame::NONE)
                    .show(&mut content_ui, |ui| {
                        Sidebar::show_vertical_mask_strip(ui, app, frame)
                    });
            }
            Sidebar::show(&mut content_ui, app, ScreenLayout::Vertical, frame);
        });

    // The photo is centred above the collapsed sheet whatever the sheet's
    // height, and continues full-bleed behind every glass surface.
    let photo = Rect::from_min_max(
        canvas.min,
        egui::pos2(
            canvas.right(),
            (canvas.bottom() - glass::MARGIN - COLLAPSED_HEIGHT).max(canvas.top() + 1.0),
        ),
    );
    app.show_preview(ui, frame, Some(photo));
    photo_gestures::handle(ui, app, frame, canvas);
}

fn top_controls_rect(canvas: Rect) -> Rect {
    Rect::from_min_size(
        canvas.min + egui::vec2(glass::MARGIN, TOP_GAP),
        egui::vec2(
            (canvas.width() - glass::MARGIN * 2.0).max(1.0),
            moduwu_design::TOOLBAR_HEIGHT + 12.0,
        ),
    )
}

fn show_top_controls(
    context: &egui::Context,
    top: Rect,
    app: &mut CalibRawApp,
    frame: &eframe::Frame,
    contents: fn(&mut Ui, &mut CalibRawApp, &eframe::Frame),
) {
    egui::Area::new(egui::Id::new("portrait-top-controls"))
        .order(egui::Order::Middle)
        .fixed_pos(top.min)
        .movable(false)
        .constrain(false)
        .show(context, |ui| {
            ui.set_min_size(top.size());
            ui.set_max_size(top.size());
            glass::paint(ui, top, CornerRadius::same((top.height() * 0.5) as u8));
            glass::translucent_contents(ui);
            ui.set_clip_rect(top);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(
                    moduwu_design::SPACE_SM as i8 + 4,
                    6,
                ))
                .show(ui, |ui| contents(ui, app, frame));
        });
}

fn show_landscape(ui: &mut Ui, app: &mut CalibRawApp, frame: &eframe::Frame) {
    let canvas = ui.available_rect_before_wrap();
    let context = ui.ctx().clone();
    let top = top_controls_rect(canvas);
    show_top_controls(&context, top, app, frame, TopBar::show_portrait);
    let rail_width = Sidebar::ANDROID_LANDSCAPE_TOOL_RAIL_WIDTH;
    let below_top = top.bottom() + glass::MARGIN * 0.6;
    let rail = Rect::from_min_max(
        egui::pos2(canvas.right() - glass::MARGIN - rail_width, below_top),
        egui::pos2(
            canvas.right() - glass::MARGIN,
            canvas.bottom() - glass::MARGIN,
        ),
    );
    let tools_width = if app.develop_ui.sidebar_open {
        ScreenLayout::Horizontal
            .sidebar_default_size(canvas.size())
            .min(canvas.width() * 0.48)
    } else {
        0.0
    };
    let tools = Rect::from_min_max(
        egui::pos2(rail.left() - glass::MARGIN * 0.6 - tools_width, below_top),
        egui::pos2(rail.left() - glass::MARGIN * 0.6, rail.bottom()),
    );
    egui::Area::new(egui::Id::new("landscape-overlay-tool-rail"))
        .order(egui::Order::Middle)
        .fixed_pos(rail.min)
        .movable(false)
        .constrain(false)
        .show(&context, |ui| {
            ui.set_min_size(rail.size());
            ui.set_max_size(rail.size());
            glass::paint(ui, rail, glass::floating_corners());
            glass::translucent_contents(ui);
            ui.set_clip_rect(rail);
            Sidebar::show_android_landscape_primary_tabs(ui, app);
        });
    if app.develop_ui.sidebar_open {
        egui::Area::new(egui::Id::new("landscape-overlay-tools"))
            .order(egui::Order::Middle)
            .fixed_pos(tools.min)
            .movable(false)
            .constrain(false)
            .show(&context, |ui| {
                ui.set_min_size(tools.size());
                ui.set_max_size(tools.size());
                glass::paint(ui, tools, glass::floating_corners());
                glass::translucent_contents(ui);
                ui.set_clip_rect(tools);
                let mut content = ui.new_child(egui::UiBuilder::new().max_rect(tools.shrink(10.0)));
                if Sidebar::shows_mask_strip(app) {
                    egui::Panel::top("landscape-overlay-masks")
                        .resizable(false)
                        .exact_size(Sidebar::VERTICAL_MASK_STRIP_HEIGHT)
                        .frame(egui::Frame::NONE)
                        .show(&mut content, |ui| {
                            Sidebar::show_vertical_mask_strip(ui, app, frame)
                        });
                }
                Sidebar::show(&mut content, app, ScreenLayout::Horizontal, frame);
            });
    }
    // The photo fills everything left of the rail; the tools panel floats
    // over it, so opening the panel does not move or rescale the photo.
    let photo = Rect::from_min_max(canvas.min, egui::pos2(rail.left(), canvas.bottom()));
    app.show_preview(ui, frame, Some(photo));
    photo_gestures::handle(ui, app, frame, canvas);
}
