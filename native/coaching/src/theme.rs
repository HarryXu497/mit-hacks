//! The look of the flat screens, so they belong to the same game as the world behind them.
//!
//! The reference for the *shape* of these controls is a console sports menu: bold angled slabs,
//! heavy dark outlines, a bright leading edge on whatever is selected. The colours are not from
//! that reference — they are the jungle's, read off what the world already uses: `jungle.rs`'s
//! turf and timber, and the ink outline `INK` draws around every character.
//!
//! Only the shape is borrowed. There is no character strip, no "Game Guide" row, no online-play
//! entry; this app has a lobby, an easel, a table and a match, and the menu says exactly that.
//!
//! The angle is drawn rather than faked with padding: egui has no skew, so a slab is a
//! `convex_polygon` with its top edge pushed along, laid over a slightly larger polygon in ink.
//! That is the same trick the world uses for its outlines — an enlarged copy of the same form,
//! grown by a fixed amount so the line keeps its weight at any size.

use bevy_egui::egui;

// --- The jungle's palette ----------------------------------------------------------------------

/// Ink. The outline around everything, and the darkest thing on screen.
pub const INK: egui::Color32 = egui::Color32::from_rgb(20, 26, 22);
/// Deep canopy shadow: the ground a panel sits on.
pub const CANOPY: egui::Color32 = egui::Color32::from_rgb(31, 46, 36);
/// Sunlit turf, for a slab at rest.
pub const TURF: egui::Color32 = egui::Color32::from_rgb(58, 92, 60);
/// Turf with the sun on it, for the one under the cursor.
pub const TURF_LIT: egui::Color32 = egui::Color32::from_rgb(82, 126, 78);
/// Carved timber, for a panel's frame and the face of a flat wood panel.
pub const TIMBER: egui::Color32 = egui::Color32::from_rgb(74, 52, 36);
/// Timber in shadow, for the bars that band the screen's top and bottom so they
/// sit behind the lighter panel between them.
pub const TIMBER_DARK: egui::Color32 = egui::Color32::from_rgb(54, 38, 26);
/// A sunlit plank, for the face of a flat panel. This is the lightest a warm
/// wood can go while cloth text over it still clears the contrast the tests
/// enforce; going lighter would mean switching the text to dark ink instead.
pub const PLANK: egui::Color32 = egui::Color32::from_rgb(132, 98, 63);
/// The same plank, a shade cooler, for the top and bottom bars.
pub const PLANK_DARK: egui::Color32 = egui::Color32::from_rgb(112, 83, 54);
/// Tribal gold: the leading edge, and anything chosen.
pub const GOLD: egui::Color32 = egui::Color32::from_rgb(226, 170, 64);
/// Bleached cloth, for text on dark.
pub const CLOTH: egui::Color32 = egui::Color32::from_rgb(240, 234, 214);
/// Quieter cloth, for the line under a heading.
pub const CLOTH_DIM: egui::Color32 = egui::Color32::from_rgb(176, 184, 166);
/// The orange team's clay, used only where something is destructive.
pub const CLAY: egui::Color32 = egui::Color32::from_rgb(176, 74, 44);

/// Height of a slab, and how far its top edge leans along.
const SLAB_HEIGHT: f32 = 46.0;
const SLAB_LEAN: f32 = 0.26;
/// How far the ink shell is grown beyond the slab. Fixed, so the line keeps its weight.
const SLAB_INK: f32 = 3.0;
/// Width of the bright leading edge.
const SLAB_EDGE: f32 = 7.0;
/// The widest a slab gets, so a maximised window does not stretch one across the screen.
pub const SLAB_MAX_WIDTH: f32 = 340.0;

/// What a slab is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    /// The thing you probably came here to do.
    Primary,
    /// Everything else.
    Plain,
    /// Something that throws work away.
    Danger,
}

impl Tone {
    fn face(self, hovered: bool) -> egui::Color32 {
        match (self, hovered) {
            (Tone::Primary, false) => TURF,
            (Tone::Primary, true) => TURF_LIT,
            (Tone::Plain, false) => CANOPY,
            (Tone::Plain, true) => TURF,
            (Tone::Danger, false) => egui::Color32::from_rgb(96, 44, 30),
            (Tone::Danger, true) => CLAY,
        }
    }

    fn edge(self) -> egui::Color32 {
        match self {
            Tone::Primary => GOLD,
            Tone::Plain => CLOTH_DIM,
            Tone::Danger => CLAY,
        }
    }
}

/// The four corners of a slab, leaning along its top edge.
///
/// Returned as a plain array so the geometry is testable without a painter: the lean has to stay
/// inside the space the widget was given, or slabs overlap their neighbours.
fn slab_corners(rect: egui::Rect, grow: f32) -> [egui::Pos2; 4] {
    let rect = rect.expand(grow);
    let lean = rect.height() * SLAB_LEAN;
    [
        egui::pos2(rect.left() + lean, rect.top()),
        egui::pos2(rect.right(), rect.top()),
        egui::pos2(rect.right() - lean, rect.bottom()),
        egui::pos2(rect.left(), rect.bottom()),
    ]
}

/// A slab: the app's one button shape.
///
/// `chosen` marks the row you are currently on — the reference shows the selected entry pushed
/// forward and brightened, which is worth keeping because it is the only thing telling you where
/// you are in a list of identical shapes.
pub fn slab(ui: &mut egui::Ui, label: &str, tone: Tone, chosen: bool) -> egui::Response {
    slab_sized(ui, label, tone, chosen, SLAB_HEIGHT, 19.0)
}

/// The same slab at HUD scale, for a control that sits over live play rather than on a screen of
/// its own. Same form and palette, so it still reads as one of these controls.
pub fn slab_compact(ui: &mut egui::Ui, label: &str, tone: Tone, chosen: bool) -> egui::Response {
    slab_sized(ui, label, tone, chosen, 30.0, 12.5)
}

fn slab_sized(
    ui: &mut egui::Ui,
    label: &str,
    tone: Tone,
    chosen: bool,
    height: f32,
    font: f32,
) -> egui::Response {
    let width = ui.available_width().min(SLAB_MAX_WIDTH);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() || chosen;
        let painter = ui.painter();

        // Ink first, as an enlarged copy of the same form.
        painter.add(egui::Shape::convex_polygon(
            slab_corners(rect, SLAB_INK).to_vec(),
            INK,
            egui::Stroke::NONE,
        ));
        painter.add(egui::Shape::convex_polygon(
            slab_corners(rect, 0.0).to_vec(),
            tone.face(hovered),
            egui::Stroke::NONE,
        ));

        // The bright leading edge, along the slab's left side and leaning with it.
        let corners = slab_corners(rect, 0.0);
        painter.add(egui::Shape::convex_polygon(
            vec![
                corners[0],
                corners[0] + egui::vec2(SLAB_EDGE, 0.0),
                corners[3] + egui::vec2(SLAB_EDGE, 0.0),
                corners[3],
            ],
            tone.edge(),
            egui::Stroke::NONE,
        ));

        painter.text(
            egui::pos2(
                rect.left() + height * SLAB_LEAN + height * 0.39,
                rect.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            label.to_uppercase(),
            egui::FontId::proportional(font),
            if hovered { CLOTH } else { CLOTH_DIM },
        );
    }

    response
}

/// A slab that fills the width it is given, for a row of two or three.
pub fn slab_inline(ui: &mut egui::Ui, label: &str, tone: Tone) -> egui::Response {
    slab(ui, label, tone, false)
}

/// Overlays a plank-and-grain pattern on a wood panel's face.
///
/// Painted over the flat base fill and behind the panel's content, so the wood
/// looks planked rather than poured. It is drawn, not sampled from a photo, to
/// stay with the flat-shaded world — horizontal seams like a timber wall, then a
/// scatter of faint grain streaks. Every offset comes from the line's index, so
/// the pattern is identical every frame and never shimmers.
pub fn wood_grain(painter: &egui::Painter, rect: egui::Rect) {
    if rect.width() < 4.0 || rect.height() < 4.0 {
        return;
    }
    // Translucent, so one pair of colours works on any plank shade.
    let groove = egui::Color32::from_rgba_unmultiplied(20, 14, 8, 96);
    let lit = egui::Color32::from_rgba_unmultiplied(255, 236, 200, 30);
    let grain = egui::Color32::from_rgba_unmultiplied(26, 16, 9, 34);

    // Horizontal plank seams: a dark groove with a lit edge just beneath it.
    let plank_h = 58.0_f32;
    let mut y = rect.top() + plank_h;
    while y < rect.bottom() - 2.0 {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.8, groove),
        );
        painter.line_segment(
            [egui::pos2(rect.left(), y + 1.8), egui::pos2(rect.right(), y + 1.8)],
            egui::Stroke::new(1.0, lit),
        );
        y += plank_h;
    }

    // Grain: faint streaks running along the planks, at index-derived heights so
    // they stay put between frames.
    let count = (rect.height() / 13.0) as i32;
    for i in 0..count {
        let t = i as f32;
        let gy = rect.top() + t * 13.0 + (t * 1.9).sin() * 4.0 + 6.0;
        if gy <= rect.top() || gy >= rect.bottom() {
            continue;
        }
        // Streaks stop short of the edges by a little, varied by index.
        let inset = 10.0 + (t * 2.3).cos().abs() * 40.0;
        let (x0, x1) = (rect.left() + inset, rect.right() - inset * 0.6);
        if x1 > x0 {
            painter.line_segment(
                [egui::pos2(x0, gy), egui::pos2(x1, gy)],
                egui::Stroke::new(1.0, grain),
            );
        }
    }
}

/// The frame a panel of controls sits in: carved timber on canopy shadow.
pub fn panel_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(CANOPY)
        .stroke(egui::Stroke::new(2.0_f32, TIMBER))
        .rounding(egui::Rounding::same(3.0_f32))
        .inner_margin(egui::Margin::symmetric(18.0, 14.0))
}

/// A panel's heading: the display face in gold, so the coaching panels wear the
/// same type as the app's other screens rather than egui's default heading font.
pub fn panel_title(ui: &mut egui::Ui, text: &str, size: f32) -> egui::Response {
    ui.label(egui::RichText::new(text).font(display_font(size)).color(GOLD))
}

/// A section label within a panel: dim-cloth on the app's other screens is gold
/// and shouted, so match that.
pub fn section_label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(display_font(13.0))
            .color(GOLD),
    )
}

/// A screen's title, with the rule under it the world's signs use.
pub fn title(ui: &mut egui::Ui, text: &str, subtitle: Option<&str>) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(34.0)
            .strong()
            .color(CLOTH),
    );
    if let Some(subtitle) = subtitle {
        ui.label(egui::RichText::new(subtitle).size(14.0).color(CLOTH_DIM));
    }
    ui.add_space(6.0);
    let width = ui.available_width().min(SLAB_MAX_WIDTH);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 3.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, GOLD);
    ui.add_space(18.0);
}

/// A dark wash over part of the screen, fading out across it.
///
/// The menu is drawn over the live world rather than over a filled panel — the jungle is the
/// backdrop, because it is already standing there and a flat colour in front of it is a worse
/// picture than the one it is hiding. But cloth text over sunlit turf is unreadable, so a wash
/// goes underneath it: opaque enough on the left to hold a heading, gone by the middle of the
/// screen so the stadium is still the thing you are looking at.
///
/// Built as one gradient mesh rather than a stack of rectangles, so there are no visible bands.
pub fn scrim(painter: &egui::Painter, rect: egui::Rect, near: u8, far: u8) {
    let mut mesh = egui::Mesh::default();
    let dark = |alpha: u8| egui::Color32::from_rgba_unmultiplied(12, 18, 14, alpha);
    mesh.colored_vertex(rect.left_top(), dark(near));
    mesh.colored_vertex(rect.right_top(), dark(far));
    mesh.colored_vertex(rect.right_bottom(), dark(far));
    mesh.colored_vertex(rect.left_bottom(), dark(near));
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// How wide the menu's wash is, as a fraction of the window.
///
/// Matches where the column of slabs ends. Past that the scenery is unobscured, which is the
/// whole reason for having it behind the menu at all.
pub const SCRIM_FRACTION: f32 = 0.46;

/// Dress the whole context: the jungle's colours, and no rounded corners anywhere.
///
/// Called once at startup. Everything not drawn by [`slab`] -- text fields, scroll bars, the
/// existing coaching panel -- inherits this, so nothing has to be restyled widget by widget for
/// the app to stop looking like two different programs.
pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);

    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;

    v.dark_mode = true;
    v.panel_fill = CANOPY;
    v.window_fill = CANOPY;
    v.extreme_bg_color = INK;
    v.faint_bg_color = egui::Color32::from_rgb(40, 58, 46);
    v.override_text_color = Some(CLOTH);
    v.hyperlink_color = GOLD;
    v.selection.bg_fill = TURF;
    v.selection.stroke = egui::Stroke::new(1.0_f32, GOLD);
    v.window_stroke = egui::Stroke::new(2.0_f32, TIMBER);

    // Square, like carved timber. The reference's slabs have no rounding at all.
    for corner in [
        &mut v.widgets.noninteractive.rounding,
        &mut v.widgets.inactive.rounding,
        &mut v.widgets.hovered.rounding,
        &mut v.widgets.active.rounding,
        &mut v.widgets.open.rounding,
        &mut v.window_rounding,
        &mut v.menu_rounding,
    ] {
        *corner = egui::Rounding::same(2.0_f32);
    }

    v.widgets.noninteractive.bg_fill = CANOPY;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, CLOTH_DIM);
    v.widgets.inactive.bg_fill = egui::Color32::from_rgb(44, 64, 50);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, CLOTH);
    v.widgets.hovered.bg_fill = TURF;
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, CLOTH);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.5_f32, GOLD);
    v.widgets.active.bg_fill = TURF_LIT;
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, CLOTH);

    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    ctx.set_style(style);
}


    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 200.0), egui::vec2(320.0, SLAB_HEIGHT))
    }

    #[test]
    fn a_slab_leans_without_leaving_the_space_it_was_given() {
        let r = rect();
        for corner in slab_corners(r, 0.0) {
            assert!(corner.x >= r.left() - 0.01, "{corner:?} is left of the widget");
            assert!(corner.x <= r.right() + 0.01, "{corner:?} is right of the widget");
            assert!(corner.y >= r.top() - 0.01 && corner.y <= r.bottom() + 0.01);
        }
    }

    #[test]
    fn the_lean_is_visible_but_not_a_wedge() {
        let corners = slab_corners(rect(), 0.0);
        let lean = corners[0].x - corners[3].x;
        assert!(lean > 6.0, "the angle should read at a glance, got {lean}");
        // A lean approaching the slab's own width would make it a triangle rather than a slab.
        assert!(lean < rect().width() * 0.25, "too steep to hold a label: {lean}");
    }

    #[test]
    fn the_ink_shell_surrounds_the_face_on_every_side() {
        let face = slab_corners(rect(), 0.0);
        let ink = slab_corners(rect(), SLAB_INK);
        // Top-right and bottom-left are the two corners that are not shifted by the lean, so
        // they are where an off-by-one in `expand` would show up first.
        assert!(ink[1].x > face[1].x && ink[1].y < face[1].y);
        assert!(ink[3].x < face[3].x && ink[3].y > face[3].y);
    }

    #[test]
    fn a_hovered_slab_is_lighter_than_one_at_rest() {
        // The only thing distinguishing the row you are on, so it has to actually differ.
        for tone in [Tone::Primary, Tone::Plain, Tone::Danger] {
            let (rest, lit) = (tone.face(false), tone.face(true));
            assert_ne!(rest, lit, "{tone:?} does not react to the cursor");
            let brightness = |c: egui::Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
            assert!(brightness(lit) > brightness(rest), "{tone:?} darkens on hover");
        }
    }

    #[test]
    fn text_on_the_jungle_palette_stays_readable() {
        // Rough relative-luminance contrast. Not a full WCAG check, but enough to catch cloth
        // text being put on a colour it disappears into.
        fn luminance(c: egui::Color32) -> f32 {
            let channel = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
        }
        fn ratio(a: egui::Color32, b: egui::Color32) -> f32 {
            let (x, y) = (luminance(a), luminance(b));
            let (hi, lo) = if x > y { (x, y) } else { (y, x) };
            (hi + 0.05) / (lo + 0.05)
        }

        for background in [CANOPY, TURF, TIMBER, PLANK, PLANK_DARK, INK] {
            assert!(
                ratio(CLOTH, background) >= 4.5,
                "cloth on {background:?} is only {:.1}:1",
                ratio(CLOTH, background)
            );
        }
        assert!(ratio(INK, GOLD) >= 4.5, "ink on gold must stay legible");
    }

// ---------------------------------------------------------------------------
// Type
// ---------------------------------------------------------------------------

/// The wordmark face and the UI face, in that order.
///
/// M PLUS Rounded 1c: the open rounded gothic that stands in for the Fontworks
/// Rodin that Animal Crossing sets its wordmark in. Subset to Latin, which is
/// why two weights cost 135 KB rather than 7 MB. See `assets/fonts/README.md`.
const DISPLAY_TTF: &[u8] = include_bytes!("../assets/fonts/MPLUSRounded1c-Black.ttf");
const BODY_TTF: &[u8] = include_bytes!("../assets/fonts/MPLUSRounded1c-Bold.ttf");

pub const DISPLAY: &str = "canopy-display";
pub const BODY: &str = "canopy-body";

/// Registers both faces and makes the bold one egui's default.
///
/// Called once, before anything draws. egui keeps its own default stack as the
/// fallback tail so anything outside the Latin subset still renders rather than
/// coming out as blank boxes.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        DISPLAY.to_owned(),
        egui::FontData::from_static(DISPLAY_TTF),
    );
    fonts
        .font_data
        .insert(BODY.to_owned(), egui::FontData::from_static(BODY_TTF));

    fonts
        .families
        .entry(egui::FontFamily::Name(DISPLAY.into()))
        .or_default()
        .insert(0, DISPLAY.to_owned());

    // Proportional is what every unstyled label in the app already asks for, so
    // putting the body face at the front of it restyles the whole UI at once
    // rather than one call site at a time.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, BODY.to_owned());

    ctx.set_fonts(fonts);
}

fn display_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(DISPLAY.into()))
}

/// Offsets for the faked text outline.
///
/// egui has no text stroke, so the string is painted eight times in ink behind
/// one copy in the fill colour. Eight directions rather than four because at
/// this weight a four-way outline leaves visible notches on the diagonals of
/// letters like A and Y -- the same reason the game's own UI does it this way.
const OUTLINE: [(f32, f32); 8] = [
    (-1., -1.),
    (0., -1.),
    (1., -1.),
    (-1., 0.),
    (1., 0.),
    (-1., 1.),
    (0., 1.),
    (1., 1.),
];

/// Paints `text` with an ink contour, and returns the box it filled.
fn paint_outlined(
    painter: &egui::Painter,
    at: egui::Pos2,
    text: &str,
    font: egui::FontId,
    fill: egui::Color32,
    weight: f32,
) -> egui::Rect {
    for (dx, dy) in OUTLINE {
        painter.text(
            at + egui::vec2(dx * weight, dy * weight),
            egui::Align2::LEFT_TOP,
            text,
            font.clone(),
            INK,
        );
    }
    painter.text(at, egui::Align2::LEFT_TOP, text, font, fill)
}

/// The wordmark, for the top of the lobby.
pub fn wordmark(ui: &mut egui::Ui, text: &str) {
    const SIZE: f32 = 40.0;
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(text.to_owned(), display_font(SIZE), GOLD)
    });
    let (rect, _) = ui.allocate_exact_size(
        galley.size() + egui::vec2(8.0, 8.0),
        egui::Sense::hover(),
    );
    paint_outlined(
        ui.painter(),
        rect.left_top() + egui::vec2(4.0, 4.0),
        text,
        display_font(SIZE),
        GOLD,
        3.0,
    );
}

/// The line under the wordmark.
///
/// Outlined like everything else on this screen. Dim cloth on its own was fine
/// over the wash and vanished the moment the wash came off -- it was sitting on
/// open sky at almost its own value.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    const SIZE: f32 = 17.0;
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(text.to_owned(), display_font(SIZE), CLOTH)
    });
    let (rect, _) = ui.allocate_exact_size(
        galley.size() + egui::vec2(6.0, 6.0),
        egui::Sense::hover(),
    );
    paint_outlined(
        ui.painter(),
        rect.left_top() + egui::vec2(3.0, 3.0),
        text,
        display_font(SIZE),
        CLOTH,
        2.0,
    );
}

/// A menu entry: outlined type over the scenery, with no slab behind it.
///
/// The slab was carrying the contrast before the type had an outline of its
/// own. Now that it does, a filled shape behind every row is one layer too
/// many -- it hides the stadium the lobby is deliberately standing in front of.
/// The row that is hovered or chosen goes gold and grows; the rest stay cloth.
pub fn menu_row(ui: &mut egui::Ui, label: &str, chosen: bool) -> egui::Response {
    const SIZE: f32 = 40.0;
    const GROWN: f32 = 46.0;

    let hovered_size = if chosen { GROWN } else { SIZE };
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(label.to_owned(), display_font(GROWN), CLOTH)
    });
    // Sized to the grown text either way, so the column does not reflow as the
    // cursor moves down it.
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 12.0, GROWN + 10.0),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let lit = chosen || response.hovered();
        let size = if response.hovered() { GROWN } else { hovered_size };
        paint_outlined(
            ui.painter(),
            rect.left_top() + egui::vec2(4.0, (rect.height() - size) * 0.5),
            label,
            display_font(size),
            if lit { GOLD } else { CLOTH },
            4.0,
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 200.0), egui::vec2(320.0, SLAB_HEIGHT))
    }

    #[test]
    fn a_slab_leans_without_leaving_the_space_it_was_given() {
        let r = rect();
        for corner in slab_corners(r, 0.0) {
            assert!(corner.x >= r.left() - 0.01, "{corner:?} is left of the widget");
            assert!(corner.x <= r.right() + 0.01, "{corner:?} is right of the widget");
            assert!(corner.y >= r.top() - 0.01 && corner.y <= r.bottom() + 0.01);
        }
    }

    #[test]
    fn the_lean_is_visible_but_not_a_wedge() {
        let corners = slab_corners(rect(), 0.0);
        let lean = corners[0].x - corners[3].x;
        assert!(lean > 6.0, "the angle should read at a glance, got {lean}");
        // A lean approaching the slab's own width would make it a triangle rather than a slab.
        assert!(lean < rect().width() * 0.25, "too steep to hold a label: {lean}");
    }

    #[test]
    fn the_ink_shell_surrounds_the_face_on_every_side() {
        let face = slab_corners(rect(), 0.0);
        let ink = slab_corners(rect(), SLAB_INK);
        // Top-right and bottom-left are the two corners that are not shifted by the lean, so
        // they are where an off-by-one in `expand` would show up first.
        assert!(ink[1].x > face[1].x && ink[1].y < face[1].y);
        assert!(ink[3].x < face[3].x && ink[3].y > face[3].y);
    }

    #[test]
    fn a_hovered_slab_is_lighter_than_one_at_rest() {
        // The only thing distinguishing the row you are on, so it has to actually differ.
        for tone in [Tone::Primary, Tone::Plain, Tone::Danger] {
            let (rest, lit) = (tone.face(false), tone.face(true));
            assert_ne!(rest, lit, "{tone:?} does not react to the cursor");
            let brightness = |c: egui::Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
            assert!(brightness(lit) > brightness(rest), "{tone:?} darkens on hover");
        }
    }

    #[test]
    fn text_on_the_jungle_palette_stays_readable() {
        // Rough relative-luminance contrast. Not a full WCAG check, but enough to catch cloth
        // text being put on a colour it disappears into.
        fn luminance(c: egui::Color32) -> f32 {
            let channel = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
        }
        fn ratio(a: egui::Color32, b: egui::Color32) -> f32 {
            let (x, y) = (luminance(a), luminance(b));
            let (hi, lo) = if x > y { (x, y) } else { (y, x) };
            (hi + 0.05) / (lo + 0.05)
        }

        for background in [CANOPY, TURF, TIMBER, INK] {
            assert!(
                ratio(CLOTH, background) >= 4.5,
                "cloth on {background:?} is only {:.1}:1",
                ratio(CLOTH, background)
            );
        }
        assert!(ratio(INK, GOLD) >= 4.5, "ink on gold must stay legible");
    }
}
