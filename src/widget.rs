// SPDX-License-Identifier: GPL-3.0-or-later
//! Motif widgets: the pieces that are a fill, a bevel and a label.
//!
//! Only the drawing. Layout and hit-testing stay with whoever owns the screen -- they are
//! what a component *is*, and a shared widget that owned them would have to know about three
//! different event loops.
//!
//! # A note on the baselines
//!
//! [`button`] and [`text_field`] center their text with `(h + ascent) / 2 - 2` rather than
//! [`Fonts::centered_baseline`](crate::text::Fonts::centered_baseline). That is the
//! arithmetic they were drawn with in `wlrix-greeter`, magic `- 2` and all, and it is kept
//! here so lifting them into this crate moved no pixel. Reconciling the two is a change with
//! a screenshot behind it, not a tidy-up to slip into a move.

use crate::bevel::Bevel;
use crate::canvas::{Canvas, Rect};
use crate::motif;
use crate::palette::Palette;
use crate::text::{Face, Fonts, Run};

/// Label size on a push button.
pub const BUTTON_PX: f32 = 14.0;
/// Text size in a single-line field.
pub const FIELD_PX: f32 = 15.0;

/// A Motif push button: a raised face with a centered label, sunken and nudged when pressed.
pub fn button(
    canvas: &mut Canvas,
    palette: &Palette,
    fonts: &mut Fonts,
    rect: Rect,
    label: &str,
    pressed: bool,
) {
    let thickness = palette.metrics.shadow_thickness_default;
    let (fill, bevel) = if pressed {
        (
            palette.armed,
            Bevel::sunken(
                palette.face_top_shadow,
                palette.face_bottom_shadow,
                thickness,
            ),
        )
    } else {
        (
            palette.face,
            Bevel::raised(
                palette.face_top_shadow,
                palette.face_bottom_shadow,
                thickness,
            ),
        )
    };
    motif::panel(canvas, rect, fill, bevel);

    // The label moves with the face when the button goes in, which is most of what sells the
    // press -- a label that stayed put would read as a button that had merely changed color.
    let nudge = if pressed { 1 } else { 0 };
    let text_w = fonts.width(Face::Regular, BUTTON_PX, label);
    let x = rect.left() + (rect.w - text_w) / 2 + nudge;
    let baseline = rect.top() + (rect.h + fonts.ascent(Face::Regular, BUTTON_PX)) / 2 - 2 + nudge;
    fonts.draw(
        canvas,
        Run {
            face: Face::Regular,
            px: BUTTON_PX,
            x,
            baseline,
            color: palette.foreground,
        },
        label,
    );
}

/// A single-line text field: a sunken well, its contents, and a caret when focused.
///
/// `caret_on` is the visible half of the blink, which the caller owns -- the blink is a timer
/// on somebody's event loop and this function draws one frame.
pub fn text_field(
    canvas: &mut Canvas,
    palette: &Palette,
    fonts: &mut Fonts,
    rect: Rect,
    text: &str,
    focused: bool,
    caret_on: bool,
) {
    let thickness = palette.metrics.shadow_thickness_text;
    motif::panel(
        canvas,
        rect,
        palette.text_field_background,
        Bevel::sunken(
            palette.text_top_shadow,
            palette.text_bottom_shadow,
            thickness,
        ),
    );
    // A focused field takes a thin keyline just inside the bevel.
    if focused {
        canvas.stroke_rect(rect.inset(thickness), palette.outer_line);
    }

    let x = rect.left() + palette.metrics.text_margin_width + thickness;
    let baseline = rect.top() + (rect.h + fonts.ascent(Face::Regular, FIELD_PX)) / 2 - 2;
    if !text.is_empty() {
        fonts.draw(
            canvas,
            Run {
                face: Face::Regular,
                px: FIELD_PX,
                x,
                baseline,
                color: palette.foreground,
            },
            text,
        );
    }

    // The caret, in the visible half of its blink, just past the last glyph.
    if focused && caret_on {
        let caret_x = x + fonts.width(Face::Regular, FIELD_PX, text);
        let top = rect.top() + thickness + 3;
        let height = rect.h - 2 * thickness - 6;
        canvas.fill_rect(
            Rect::new(caret_x, top, 1, height.max(1)),
            palette.foreground,
        );
    }
}

/// The triangular glyph on a scrollbar's stepper, centered in `button`.
pub fn arrow(canvas: &mut Canvas, palette: &Palette, button: Rect, up: bool) {
    let cx = button.x + button.w / 2;
    let cy = button.y + button.h / 2;
    let size: i32 = 4;
    // The apex is one pixel wide and the base `2*size+1`; `up` flips which end is which.
    for step in 0..=size {
        let half = if up { step } else { size - step };
        let row_y = cy - size / 2 + step;
        for dx in -half..=half {
            canvas.put(cx + dx, row_y, palette.foreground);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;
    use crate::palette::DEFAULT;

    fn canvas(buf: &mut Vec<u8>, w: i32, h: i32) -> Canvas<'_> {
        *buf = vec![0u8; (w * h * 4) as usize];
        Canvas::new(buf, w, h)
    }

    #[test]
    fn a_button_is_raised_and_a_pressed_one_is_sunken() {
        let p = DEFAULT;
        let rect = Rect::new(0, 0, 60, 24);

        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 60, 24);
        let mut fonts = Fonts::load().expect("system fonts");
        button(&mut c, p, &mut fonts, rect, "Login", false);
        assert_eq!(c.get(30, 0), p.face_top_shadow, "raised: light on top");
        assert_eq!(c.get(30, 23), p.face_bottom_shadow);

        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 60, 24);
        button(&mut c, p, &mut fonts, rect, "Login", true);
        assert_eq!(c.get(30, 0), p.face_bottom_shadow, "pressed: it inverts");
        assert_eq!(c.get(30, 23), p.face_top_shadow);
    }

    #[test]
    fn a_field_takes_a_keyline_only_when_focused() {
        let p = DEFAULT;
        let rect = Rect::new(0, 0, 80, 26);
        let inset = p.metrics.shadow_thickness_text;

        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 80, 26);
        let mut fonts = Fonts::load().expect("system fonts");
        text_field(&mut c, p, &mut fonts, rect, "", false, false);
        assert_ne!(c.get(40, inset), p.outer_line);

        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 80, 26);
        text_field(&mut c, p, &mut fonts, rect, "", true, false);
        assert_eq!(c.get(40, inset), p.outer_line);
    }

    #[test]
    fn the_caret_shows_only_in_the_lit_half_of_the_blink() {
        let p = DEFAULT;
        let rect = Rect::new(0, 0, 80, 26);
        let mut fonts = Fonts::load().expect("system fonts");

        let mut lit = |caret_on: bool| {
            let mut buf = vec![0u8; 80 * 26 * 4];
            let mut c = Canvas::new(&mut buf, 80, 26);
            text_field(&mut c, p, &mut fonts, rect, "", true, caret_on);
            let thickness = p.metrics.shadow_thickness_text;
            let x = rect.left() + p.metrics.text_margin_width + thickness;
            // Strictly inside the focus keyline, which is drawn in the same black and would
            // otherwise answer for the caret.
            ((thickness + 3)..(rect.h - thickness - 3)).any(|y| c.get(x, y) == p.foreground)
        };
        assert!(lit(true));
        assert!(!lit(false));
    }

    #[test]
    fn an_arrow_points_the_way_it_is_asked_to() {
        let p = DEFAULT;
        let rect = Rect::new(0, 0, 18, 18);

        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 18, 18);
        c.clear(Rgb(0xff_ffffff));
        arrow(&mut c, p, rect, true);
        // Apex at the top, base at the bottom: the top row of the glyph is narrower.
        let width = |y: i32| (0..18).filter(|x| c.get(*x, y) == p.foreground).count();
        assert!(width(7) < width(11), "up: {} vs {}", width(7), width(11));

        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 18, 18);
        c.clear(Rgb(0xff_ffffff));
        arrow(&mut c, p, rect, false);
        let width = |y: i32| (0..18).filter(|x| c.get(*x, y) == p.foreground).count();
        assert!(width(7) > width(11), "down: {} vs {}", width(7), width(11));
    }
}
