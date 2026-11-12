//! Touch gestures on the photo in the overlay Develop layout, on top of the
//! pinch, pan, double-tap fit and press-and-hold original that the preview
//! already handles:
//!
//! - a quick horizontal swipe on the fitted photo opens the previous or next
//!   photo, as ← and → do;
//! - a two-finger tap undoes the last edit, a three-finger tap redoes it;
//! - a single tap hides or shows every control over the photo. It waits out
//!   the double-tap window first, so the double-tap fit never toggles it.
//!
//! Gestures only start on the photo's own layer, so a touch that lands on a
//! glass surface never counts. Tabs whose canvas tools take single-finger
//! drags (masks, inpainting, crop, the export carousel) and active pickers
//! keep the swipe for themselves.

use crate::app::{CalibRawApp, SidebarTab};
use crate::ui::develop::open_adjacent_photo;
use eframe::egui::{self, Pos2, Rect, Ui};

/// Shortest horizontal travel of a swipe, in points.
const SWIPE_MIN_DISTANCE: f32 = 72.0;
/// Longest a swipe may take, in seconds.
const SWIPE_MAX_SECONDS: f64 = 0.6;
/// How much more horizontal than vertical a swipe must be.
const SWIPE_AXIS_RATIO: f32 = 2.0;
/// Longest a multi-finger tap may last, in seconds.
const TAP_MAX_SECONDS: f64 = 0.35;
/// Most the fingers of a tap may travel together, in points.
const TAP_MAX_TRAVEL: f32 = 14.0;
/// Most the fingers of a tap may land apart in time, in seconds: a tap is
/// not a second finger joining a stroke already under way.
const TAP_LANDING_WINDOW: f64 = 0.15;
/// Longest a single tap may last, in seconds; longer presses are holds.
const SINGLE_TAP_MAX_SECONDS: f64 = 0.25;
/// Most a single tap may travel, in points.
const SINGLE_TAP_MAX_TRAVEL: f32 = 10.0;
/// How long a single tap waits for a second one, in seconds. Matches the
/// double-tap window of the preview's fit gesture.
const DOUBLE_TAP_WINDOW: f64 = 0.3;

#[derive(Clone, Copy)]
struct Swipe {
    origin: Pos2,
    started: f64,
    /// A second finger touched during the press: it was a pinch, not a swipe.
    multi_touch: bool,
}

#[derive(Clone, Copy)]
struct Tap {
    started: f64,
    travel: f32,
    fingers: usize,
    valid: bool,
}

#[derive(Clone, Copy, Default)]
struct GestureState {
    swipe: Option<Swipe>,
    tap: Option<Tap>,
    /// Release time of a single tap still waiting out the double-tap window.
    pending_tap: Option<f64>,
    /// The current press is the second tap of a double tap.
    second_tap: bool,
}

fn state_id() -> egui::Id {
    egui::Id::new("develop-photo-gestures")
}

fn immersive_id() -> egui::Id {
    egui::Id::new("develop-immersive")
}

/// Whether a tap on the photo hid the controls.
pub(crate) fn immersive(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(immersive_id()))
        .unwrap_or(false)
}

/// Whether the open tab leaves single taps on the photo free.
fn tap_allowed(app: &CalibRawApp) -> bool {
    let tool_owns_tap = matches!(
        app.ui.sidebar_tab,
        SidebarTab::Masks | SidebarTab::Inpainting | SidebarTab::Crop
    );
    let picker_active = app.develop_ui.area_picker.is_some()
        || app.develop_ui.point_color.picker_active
        || app.develop_ui.mask_point_color.picker_active;
    !tool_owns_tap && !picker_active && !app.preview.original_hold_owns_touch()
}

/// Whether the open tab leaves single-finger drags on the photo free.
fn swipe_allowed(app: &CalibRawApp) -> bool {
    let tool_owns_drag = matches!(
        app.ui.sidebar_tab,
        SidebarTab::Masks | SidebarTab::Inpainting | SidebarTab::Crop | SidebarTab::Export
    );
    let picker_active = app.develop_ui.area_picker.is_some()
        || app.develop_ui.point_color.picker_active
        || app.develop_ui.mask_point_color.picker_active;
    let fitted = (app.preview.zoom - 1.0).abs() <= 0.0005;
    fitted && !tool_owns_drag && !picker_active && !app.preview.original_hold_owns_touch()
}

/// Recognises the gestures that start on the photo inside `area`. Call after
/// the preview, from the Ui the preview was painted in.
pub(crate) fn handle(ui: &Ui, app: &mut CalibRawApp, frame: &eframe::Frame, area: Rect) {
    if !app.caps().is_mobile() {
        return;
    }
    let ctx = ui.ctx().clone();
    let layer = ui.layer_id();
    let on_photo =
        |pos: Pos2| area.contains(pos) && ctx.layer_id_at(pos).is_none_or(|hit| hit == layer);
    let mut state = ctx
        .data(|data| data.get_temp::<GestureState>(state_id()))
        .unwrap_or_default();

    let (now, pressed, released, press_origin, latest, multi) = ui.input(|input| {
        (
            input.time,
            input.pointer.primary_pressed(),
            input.pointer.primary_released(),
            input.pointer.press_origin(),
            input.pointer.interact_pos(),
            input.multi_touch(),
        )
    });

    if pressed {
        state.second_tap = state
            .pending_tap
            .take()
            .is_some_and(|released| now - released <= DOUBLE_TAP_WINDOW);
        state.swipe = press_origin
            .filter(|origin| on_photo(*origin))
            .map(|origin| Swipe {
                origin,
                started: now,
                multi_touch: false,
            });
    }

    match multi {
        Some(touch) => {
            if let Some(swipe) = &mut state.swipe {
                swipe.multi_touch = true;
            }
            let tap = state.tap.get_or_insert_with(|| Tap {
                started: now,
                travel: 0.0,
                fingers: touch.num_touches,
                // Both fingers must land on the photo at about the same time.
                valid: on_photo(touch.start_pos)
                    && state
                        .swipe
                        .is_none_or(|swipe| now - swipe.started <= TAP_LANDING_WINDOW),
            });
            tap.fingers = tap.fingers.max(touch.num_touches);
            tap.travel += touch.translation_delta.length() + (touch.zoom_delta - 1.0).abs() * 200.0;
        }
        None => {
            if let Some(tap) = state.tap.take() {
                let quick = now - tap.started <= TAP_MAX_SECONDS;
                if tap.valid && quick && tap.travel <= TAP_MAX_TRAVEL {
                    match tap.fingers {
                        2 if app.can_undo_edit() => app.undo_edit(),
                        3 if app.can_redo_edit() => app.redo_edit(),
                        _ => {}
                    }
                }
            }
        }
    }

    if released {
        if let (Some(swipe), Some(end)) = (state.swipe.take(), latest) {
            let delta = end - swipe.origin;
            let is_swipe = !swipe.multi_touch
                && now - swipe.started <= SWIPE_MAX_SECONDS
                && delta.x.abs() >= SWIPE_MIN_DISTANCE
                && delta.x.abs() >= delta.y.abs() * SWIPE_AXIS_RATIO;
            if is_swipe && swipe_allowed(app) {
                // Finger moving left reveals the next photo, as in a gallery.
                open_adjacent_photo(app, delta.x < 0.0, frame);
            }
            let is_tap = !swipe.multi_touch
                && now - swipe.started <= SINGLE_TAP_MAX_SECONDS
                && delta.length() <= SINGLE_TAP_MAX_TRAVEL;
            if is_tap && !state.second_tap && tap_allowed(app) {
                state.pending_tap = Some(now);
            }
        }
        state.second_tap = false;
    }

    if let Some(released) = state.pending_tap {
        let waited = now - released;
        if waited > DOUBLE_TAP_WINDOW {
            state.pending_tap = None;
            let hidden = immersive(&ctx);
            ctx.data_mut(|data| data.insert_temp(immersive_id(), !hidden));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                DOUBLE_TAP_WINDOW - waited + 0.01,
            ));
        }
    }

    ctx.data_mut(|data| data.insert_temp(state_id(), state));
}
