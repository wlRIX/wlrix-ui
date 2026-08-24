// SPDX-License-Identifier: GPL-3.0-or-later
//! Motif bevel geometry, with no opinion about where the pixels go.
//!
//! Every panel, button and field in the Indigo Magic look is a rectangle with a beveled
//! edge: two light sides and two dark, so it reads as standing out of the screen or
//! pressed into it. Which pair goes where is the only difference between raised and
//! sunken, and each surface carries its *own* light/dark pair -- which is why the palette
//! has a shadow pair per surface rather than one global one.
//!
//! wlRIX draws that edge into two very different sinks. The clients ([`wlrix-greeter`],
//! [`wlrix-desktop`]) fill pixels in a `wl_shm` buffer; the compositor emits solid-color
//! quads for the GPU. Both were written separately and neither is wrong, so this module is
//! the geometry alone -- a run of `(Rect, Shade)` pairs -- and each sink decides what a
//! `Shade` looks like.
//!
//! **Quads are emitted in painter's order: a later quad covers an earlier one.** That is
//! what a [`Canvas`](crate::canvas::Canvas) sink does by filling them in turn. A sink
//! building a scene front-to-back -- smithay's render-element lists are ordered topmost
//! first -- reverses the run. One convention, stated once, because the two sinks disagree
//! and the disagreement is invisible until a bevel comes out as a flat slab.
//!
//! [`wlrix-greeter`]: https://github.com/wlRIX/wlrix-greeter
//! [`wlrix-desktop`]: https://github.com/wlRIX/wlrix-desktop

use crate::canvas::Rect;

/// Which of a surface's three tones a quad takes.
///
/// Named rather than colored, because the caller holds the colurs: the compositor's are
/// `Color32F` and a client's are [`Rgb`](crate::color::Rgb), and neither should have to
/// convert to speak to this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shade {
    /// The interior.
    Face,
    /// Top and left when raised, bottom and right when sunken.
    Light,
    /// The other pair.
    Dark,
}

/// How a bevel's four sides are cut, and so which of its shadows run the full span.
///
/// This exists because wlRIX has two bevels that are both correct and do not agree at the
/// corners, and unifying them by fiat would change pixels on one side or the other. Pick the
/// one that matches the sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// `thickness` nested rings, each drawn top, left, bottom, right, then the two corner
    /// pixels the edges disagree on going to the light side. Motif's own convention, and what
    /// every client-drawn widget uses. Quads overlap at the corners, so a sink must paint them
    /// in the order they are emitted.
    Rings,
    /// Pieces butt left to right: the left and right shadows run the full height.
    Horizontal,
    /// Pieces butt top to bottom: the top and bottom shadows run the full width.
    Vertical,
}

/// A bevel drawn light-top-left, dark-bottom-right (raised) or the reverse (sunken).
///
/// Carries the two colors as well as the thickness, for the client-side drawing in
/// [`crate::motif`]. The compositor never builds one: it holds `Color32F` and calls
/// [`quads`] directly.
#[derive(Clone, Copy)]
pub struct Bevel {
    pub top_left: crate::color::Rgb,
    pub bottom_right: crate::color::Rgb,
    pub thickness: i32,
}

impl Bevel {
    /// Standing out of the screen: light above and left.
    pub fn raised(
        top_shadow: crate::color::Rgb,
        bottom_shadow: crate::color::Rgb,
        thickness: i32,
    ) -> Self {
        Self {
            top_left: top_shadow,
            bottom_right: bottom_shadow,
            thickness,
        }
    }

    /// Pressed into the screen: the light and dark swap, so the highlight is below.
    pub fn sunken(
        top_shadow: crate::color::Rgb,
        bottom_shadow: crate::color::Rgb,
        thickness: i32,
    ) -> Self {
        Self {
            top_left: bottom_shadow,
            bottom_right: top_shadow,
            thickness,
        }
    }
}

/// Emit one beveled piece into `out`, in paint order.
///
/// [`Run::Rings`] emits the edge only -- the caller has already filled the interior, which is
/// what [`crate::motif::panel`] does. [`Run::Horizontal`] and [`Run::Vertical`] emit the
/// interior too, as a final [`Shade::Face`] quad, because the compositor builds a scene rather
/// than painting over one.
pub fn quads(out: &mut Vec<(Rect, Shade)>, rect: Rect, thickness: i32, raised: bool, run: Run) {
    let (tl, br) = if raised {
        (Shade::Light, Shade::Dark)
    } else {
        (Shade::Dark, Shade::Light)
    };

    if run == Run::Rings {
        for ring in 0..thickness {
            let r = rect.inset(ring);
            if r.w <= 0 || r.h <= 0 {
                break;
            }
            let (top, bottom) = (r.top(), r.bottom() - 1);
            let (left, right) = (r.left(), r.right() - 1);

            out.push((Rect::new(r.x, r.y, r.w, 1), tl));
            out.push((Rect::new(r.x, r.y, 1, r.h), tl));
            out.push((Rect::new(r.x, bottom, r.w, 1), br));
            out.push((Rect::new(right, r.y, 1, r.h), br));
            // The two corners the edges disagree on go to the light side, Motif's way.
            out.push((Rect::new(right, top, 1, 1), tl));
            out.push((Rect::new(left, bottom, 1, 1), tl));
        }
        return;
    }

    let b = thickness.min(rect.w / 2).min(rect.h / 2).max(0);
    if b == 0 {
        out.push((rect, Shade::Face));
        return;
    }
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    // The pair on the butting ends runs the full span; the other pair is inset between them.
    // No two quads overlap either way, so nothing has to arbitrate at the piece's own corners.
    match run {
        Run::Vertical => {
            out.push((Rect::new(x, y, w, b), tl));
            out.push((Rect::new(x, y + b, b, h - 2 * b), tl));
            out.push((Rect::new(x, y + h - b, w, b), br));
            out.push((Rect::new(x + w - b, y + b, b, h - 2 * b), br));
        }
        Run::Horizontal => {
            out.push((Rect::new(x, y, b, h), tl));
            out.push((Rect::new(x + b, y, w - 2 * b, b), tl));
            out.push((Rect::new(x + w - b, y, b, h), br));
            out.push((Rect::new(x + b, y + h - b, w - 2 * b, b), br));
        }
        Run::Rings => unreachable!("handled above"),
    }
    out.push((Rect::new(x + b, y + b, w - 2 * b, h - 2 * b), Shade::Face));
}

/// Emit a beveled *ring* of thickness `band` inside `outer`: raised on the outer edge, sunken
/// on the inner one, with the band's face between and nothing crossing the corners.
///
/// Not four [`quads`] bands. A band bevels all four of its own edges, so the top band's lower
/// shadow -- the line that reads as the seam under the titlebar -- would carry on past the
/// titlebar and across both top corners, and the bottom band would do the same. A ring has only
/// two edges to shade, so the corners stay plain face and it reads as one continuous piece,
/// which is how IRIX drew a fixed-size window's border.
pub fn ring_quads(out: &mut Vec<(Rect, Shade)>, outer: Rect, band: i32, bevel: i32) {
    let (x, y, w, h) = (outer.x, outer.y, outer.w, outer.h);
    let b = bevel.min(band).max(0);
    if w <= 0 || h <= 0 || band <= 0 {
        return;
    }

    // The face behind both edges, as four runs rather than one rect over the whole window --
    // the middle belongs to the client, and a quad that size would be drawn every frame for
    // nothing. First, because the edges go over it; see the module's ordering note.
    out.push((Rect::new(x, y, w, band), Shade::Face));
    out.push((Rect::new(x, y + h - band, w, band), Shade::Face));
    out.push((Rect::new(x, y + band, band, h - 2 * band), Shade::Face));
    out.push((
        Rect::new(x + w - band, y + band, band, h - 2 * band),
        Shade::Face,
    ));

    // Same strip layout as `quads`: the horizontals run the full span and the verticals sit
    // between them, so the four strips of one edge do not overlap each other and the corners
    // need no arbitration.
    let edge = |out: &mut Vec<(Rect, Shade)>, r: Rect, top_left: Shade, bottom_right: Shade| {
        let (x, y, w, h) = (r.x, r.y, r.w, r.h);
        out.push((Rect::new(x, y, w, b), top_left));
        out.push((Rect::new(x, y + h - b, w, b), bottom_right));
        out.push((Rect::new(x, y + b, b, h - 2 * b), top_left));
        out.push((Rect::new(x + w - b, y + b, b, h - 2 * b), bottom_right));
    };
    // Inner edge, sunken: shaded the opposite way round from the outer one, which is what
    // makes the band stand up off the window rather than being a flat frame.
    let inner = Rect::new(
        x + band - b,
        y + band - b,
        w - 2 * (band - b),
        h - 2 * (band - b),
    );
    edge(out, inner, Shade::Dark, Shade::Light);
    // Outer edge, raised.
    edge(out, outer, Shade::Light, Shade::Dark);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Which shade covers a point, taking the *last* quad that contains it -- paint order.
    fn shade_at(quads: &[(Rect, Shade)], x: i32, y: i32) -> Option<Shade> {
        quads
            .iter()
            .rfind(|(r, _)| r.contains(x, y))
            .map(|(_, s)| *s)
    }

    #[test]
    fn rings_put_light_top_left_and_dark_bottom_right() {
        let mut out = Vec::new();
        quads(&mut out, Rect::new(0, 0, 8, 8), 2, true, Run::Rings);
        assert_eq!(shade_at(&out, 3, 0), Some(Shade::Light));
        assert_eq!(shade_at(&out, 0, 3), Some(Shade::Light));
        assert_eq!(shade_at(&out, 3, 7), Some(Shade::Dark));
        assert_eq!(shade_at(&out, 7, 3), Some(Shade::Dark));
        // Rings never touch the interior; that is the caller's fill.
        assert_eq!(shade_at(&out, 4, 4), None);
    }

    #[test]
    fn rings_give_the_disputed_corners_to_the_light_side() {
        let mut out = Vec::new();
        quads(&mut out, Rect::new(0, 0, 8, 8), 1, true, Run::Rings);
        // Top-right and bottom-left are where the light and dark edges meet. Motif's rule
        // hands both to the light side, and paint order is what enforces it.
        assert_eq!(shade_at(&out, 7, 0), Some(Shade::Light));
        assert_eq!(shade_at(&out, 0, 7), Some(Shade::Light));
    }

    #[test]
    fn sunken_is_the_reverse_of_raised() {
        let mut out = Vec::new();
        quads(&mut out, Rect::new(0, 0, 8, 8), 2, false, Run::Rings);
        assert_eq!(shade_at(&out, 3, 0), Some(Shade::Dark));
        assert_eq!(shade_at(&out, 3, 7), Some(Shade::Light));
    }

    #[test]
    fn strip_runs_do_not_overlap() {
        for run in [Run::Horizontal, Run::Vertical] {
            let mut out = Vec::new();
            quads(&mut out, Rect::new(0, 0, 20, 12), 2, true, run);
            for y in 0..12 {
                for x in 0..20 {
                    let hits = out.iter().filter(|(r, _)| r.contains(x, y)).count();
                    assert_eq!(hits, 1, "{run:?} covers ({x},{y}) {hits} times");
                }
            }
        }
    }

    #[test]
    fn a_run_shades_the_butting_ends_full_span() {
        // Vertical pieces butt top to bottom, so the top and bottom shadows must reach the
        // piece's own left and right edges -- otherwise the seam between two stacked pieces
        // stops short and reads as a mark that only got halfway across.
        let mut out = Vec::new();
        quads(&mut out, Rect::new(0, 0, 20, 12), 2, true, Run::Vertical);
        assert_eq!(shade_at(&out, 0, 0), Some(Shade::Light));
        assert_eq!(shade_at(&out, 19, 0), Some(Shade::Light));
        assert_eq!(shade_at(&out, 0, 11), Some(Shade::Dark));
        assert_eq!(shade_at(&out, 19, 11), Some(Shade::Dark));
    }

    #[test]
    fn a_piece_with_no_room_for_a_bevel_is_a_flat_face() {
        // A strip one pixel wide cannot carry a left *and* a right shadow, so it is drawn
        // plain rather than as two overlapping edges with no interior between them.
        for (rect, thickness) in [(Rect::new(0, 0, 1, 4), 2), (Rect::new(0, 0, 8, 8), 0)] {
            let mut out = Vec::new();
            quads(&mut out, rect, thickness, true, Run::Horizontal);
            assert_eq!(
                out.as_slice(),
                [(rect, Shade::Face)],
                "{rect:?} @ {thickness}"
            );
        }
    }

    #[test]
    fn a_bevel_is_clamped_to_half_the_piece() {
        // Asked for more bevel than fits, the piece keeps a one-pixel interior rather than
        // letting the two shadows cross.
        let mut out = Vec::new();
        quads(&mut out, Rect::new(0, 0, 3, 3), 8, true, Run::Horizontal);
        assert_eq!(shade_at(&out, 1, 1), Some(Shade::Face));
        assert_eq!(shade_at(&out, 0, 1), Some(Shade::Light));
        assert_eq!(shade_at(&out, 2, 1), Some(Shade::Dark));
    }

    #[test]
    fn a_ring_leaves_the_middle_alone() {
        let mut out = Vec::new();
        ring_quads(&mut out, Rect::new(0, 0, 40, 30), 8, 2);
        // The client's own surface goes in the hole; a quad over it would be overdraw.
        assert_eq!(shade_at(&out, 20, 15), None);
        // The face is emitted first and the edges over it, so the band's middle stays face.
        assert_eq!(shade_at(&out, 20, 4), Some(Shade::Face));
        // Outer edge raised, inner edge sunken: light outside, dark just inside.
        assert_eq!(shade_at(&out, 20, 0), Some(Shade::Light));
        assert_eq!(shade_at(&out, 20, 6), Some(Shade::Dark));
    }
}
