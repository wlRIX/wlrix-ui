// SPDX-License-Identifier: GPL-3.0-or-later
//! Motif's raised and sunken edges, painted into a [`Canvas`].
//!
//! The geometry lives in [`crate::bevel`]; this is the client-side sink for it, which
//! resolves each [`Shade`](crate::bevel::Shade) to a color and fills. The compositor has its
//! own sink that emits GPU quads from the same geometry.

use crate::bevel::{self, Bevel, Run, Shade};

use crate::canvas::{Canvas, Rect};
use crate::color::Rgb;

/// Draw a beveled edge just inside `rect`, leaving the interior untouched.
pub fn draw(canvas: &mut Canvas, rect: Rect, bevel: Bevel) {
    let mut quads = Vec::new();
    // `raised` is already folded into the `Bevel`'s two colors -- `sunken()` swaps them when
    // it is built -- so the geometry is always asked for the raised arrangement and the swap
    // happens once, at construction, rather than twice.
    bevel::quads(&mut quads, rect, bevel.thickness, true, Run::Rings);
    for (r, shade) in quads {
        let color = match shade {
            Shade::Light => bevel.top_left,
            Shade::Dark => bevel.bottom_right,
            // `Run::Rings` emits the edge only; the interior is `panel`'s fill.
            Shade::Face => continue,
        };
        canvas.fill_rect(r, color);
    }
}

/// A beveled panel: fill the interior, then edge it.
pub fn panel(canvas: &mut Canvas, rect: Rect, fill: Rgb, bevel: Bevel) {
    canvas.fill_rect(rect, fill);
    draw(canvas, rect, bevel);
}

/// A single-pixel outline in `color`, drawn *around* `rect`'s outermost ring.
///
/// The dialog's black keyline sits outside the raised bevel, so it is drawn separately
/// from [`draw`].
pub fn outline(canvas: &mut Canvas, rect: Rect, color: Rgb) {
    canvas.stroke_rect(rect, color);
}

/// Motif's etched separator: a dark line with a light one under it, centred in `row`.
///
/// The two-line etch reads as a groove cut into the surface rather than a drawn rule, which
/// is why it is a pair and not one line. Used between menu sections, by both the compositor's
/// window menu and the desktop's right-click menu -- they are the same object as far as the
/// user is concerned, so they are the same code here.
pub fn groove(canvas: &mut Canvas, row: Rect, top_shadow: Rgb, bottom_shadow: Rgb) {
    let y = row.y + row.h / 2 - 1;
    canvas.fill_rect(Rect::new(row.x, y, row.w, 1), bottom_shadow);
    canvas.fill_rect(Rect::new(row.x, y + 1, row.w, 1), top_shadow);
}

#[cfg(test)]
mod tests {
    use super::*;

    const FACE: Rgb = Rgb(0xff_999999);
    const LIGHT: Rgb = Rgb(0xff_e0e0e0);
    const DARK: Rgb = Rgb(0xff_606060);

    fn canvas(buf: &mut Vec<u8>, size: i32) -> Canvas<'_> {
        *buf = vec![0u8; (size * size * 4) as usize];
        Canvas::new(buf, size, size)
    }

    #[test]
    fn raised_puts_light_top_left_dark_bottom_right() {
        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 8);
        let rect = Rect::new(0, 0, 8, 8);
        panel(&mut c, rect, FACE, Bevel::raised(LIGHT, DARK, 2));
        // A pixel on the top edge is light, one on the bottom edge is dark.
        assert_eq!(c.get(3, 0), LIGHT);
        assert_eq!(c.get(3, 7), DARK);
        assert_eq!(c.get(0, 3), LIGHT);
        assert_eq!(c.get(7, 3), DARK);
        // The interior kept the fill.
        assert_eq!(c.get(4, 4), FACE);
    }

    #[test]
    fn sunken_is_the_reverse() {
        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 8);
        panel(
            &mut c,
            Rect::new(0, 0, 8, 8),
            FACE,
            Bevel::sunken(LIGHT, DARK, 2),
        );
        // Now the top is dark and the bottom light -- pressed in.
        assert_eq!(c.get(3, 0), DARK);
        assert_eq!(c.get(3, 7), LIGHT);
    }

    #[test]
    fn thickness_draws_that_many_rings() {
        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 10);
        draw(
            &mut c,
            Rect::new(0, 0, 10, 10),
            Bevel::raised(LIGHT, DARK, 3),
        );
        // Three light rings deep on the top edge, then the untouched interior.
        assert_eq!(c.get(5, 0), LIGHT);
        assert_eq!(c.get(5, 1), LIGHT);
        assert_eq!(c.get(5, 2), LIGHT);
        assert_eq!(c.get(5, 3), Rgb(0));
    }

    #[test]
    fn the_disputed_corners_go_to_the_light_side() {
        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 8);
        draw(&mut c, Rect::new(0, 0, 8, 8), Bevel::raised(LIGHT, DARK, 1));
        assert_eq!(c.get(7, 0), LIGHT);
        assert_eq!(c.get(0, 7), LIGHT);
    }

    #[test]
    fn a_groove_is_dark_over_light() {
        let mut buf = Vec::new();
        let mut c = canvas(&mut buf, 8);
        groove(&mut c, Rect::new(0, 0, 8, 8), LIGHT, DARK);
        assert_eq!(c.get(4, 3), DARK);
        assert_eq!(c.get(4, 4), LIGHT);
    }
}
