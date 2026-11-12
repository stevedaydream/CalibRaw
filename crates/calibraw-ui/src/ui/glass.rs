//! Translucent "glass" surfaces for the touch Develop overlay: the photo stays
//! full-bleed and the tool surfaces float over it as tinted, rounded cards.
//!
//! egui has no backdrop blur, so a surface is a tinted fill with a soft
//! shadow, a hairline outline and a brighter top edge that reads as a lit
//! glass rim. Widgets inside a surface get translucent visuals through
//! [`translucent_contents`], so panel and card fills do not repaint an opaque
//! layer over the tint.

use eframe::egui::{self, Color32, CornerRadius, Rect, Stroke, StrokeKind, Ui};

/// Corner radius of floating surfaces, in points.
pub(crate) const CORNER: u8 = 22;
/// Gap between floating surfaces and the screen edges, in points.
pub(crate) const MARGIN: f32 = 10.0;

/// Tint of the surface body. Dark mode keeps enough density for white text
/// over bright skies and sand; light mode keeps dark text readable over
/// shadows.
fn tint(dark_mode: bool) -> Color32 {
    if dark_mode {
        Color32::from_rgba_unmultiplied(16, 20, 26, 150)
    } else {
        Color32::from_rgba_unmultiplied(246, 247, 250, 168)
    }
}

/// Fill of cards and sections nested in a surface: a faint lift, not a layer.
fn nested_fill(dark_mode: bool) -> Color32 {
    if dark_mode {
        Color32::from_white_alpha(14)
    } else {
        Color32::from_black_alpha(10)
    }
}

/// Paints one floating glass surface at `rect`, below its contents.
pub(crate) fn paint(ui: &Ui, rect: Rect, corner: CornerRadius) {
    let dark = ui.visuals().dark_mode;
    let painter = ui.ctx().layer_painter(ui.layer_id());
    let shadow = egui::Shadow {
        offset: [0, 6],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 90 } else { 40 }),
    };
    painter.add(shadow.as_shape(rect, corner));
    painter.rect_filled(rect, corner, tint(dark));
    // Hairline outline, then a brighter rim along the top edge.
    painter.rect_stroke(
        rect,
        corner,
        Stroke::new(1.0, Color32::from_white_alpha(if dark { 26 } else { 90 })),
        StrokeKind::Inside,
    );
    let rim_inset = f32::from(corner.nw.max(corner.ne)).max(8.0);
    let rim_y = rect.top() + 0.75;
    if rect.width() > rim_inset * 2.0 {
        painter.hline(
            (rect.left() + rim_inset)..=(rect.right() - rim_inset),
            rim_y,
            Stroke::new(1.0, Color32::from_white_alpha(if dark { 60 } else { 160 })),
        );
    }
}

/// Makes the panel, window and card fills used by the contents of `ui`
/// translucent, so nested frames tint rather than cover the glass.
pub(crate) fn translucent_contents(ui: &mut Ui) {
    let dark = ui.visuals().dark_mode;
    let visuals = ui.visuals_mut();
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.window_fill = Color32::TRANSPARENT;
    visuals.faint_bg_color = nested_fill(dark);
    visuals.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
    // Slider tracks and text fields stay legible but let the photo through.
    visuals.extreme_bg_color = visuals.extreme_bg_color.gamma_multiply(0.55);
}

/// Corners for a surface docked to no edge.
pub(crate) fn floating_corners() -> CornerRadius {
    CornerRadius::same(CORNER)
}
