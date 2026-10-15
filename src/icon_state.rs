// SPDX-License-Identifier: GPL-3.0-or-later
//! How an icon shows that it is at rest, under the pointer, or selected -- IRIX's rule, measured
//! off a real IRIX login screen rather than guessed.
//!
//! **Only the icon's body changes.** IRIX drew icons with a "body" color that the state swapped:
//! at rest a 50% checkerboard of white and a knocked-back gray, under the pointer solid white,
//! selected solid lamp yellow. The outline, the drop shadow and any colored detail are drawn
//! the same in all three. On the login screen's figure-at-a-workstation, the body is pure white,
//! the outline black and the shadow `#868686`, and only the white moved.
//!
//! Artwork here is bitmaps, not IRIX's vector icons with a body color named per shape, so the
//! body is found by lightness: a pixel whose every channel is at least [`BODY_THRESHOLD`] is
//! body, and is **multiplied** by the state's color. On pure white that is the color itself,
//! exactly IRIX's swap. On the off-white bodies the wlrix icon theme draws in -- the folder's
//! `#eeeeec`, a text file's `#ffefe5` -- it keeps their faint shading. Everything darker is left
//! exactly as drawn: the black outline, the `#808080` shadow, the folder's tan, a `#999999`
//! detail.
//!
//! This replaced multiplying the *whole* icon by one color, which tinted the shadow too -- olive
//! under yellow -- and drew the rest state as a flat gray where IRIX had a stipple.
//!
//! A picture that is not an icon -- a user's own account picture -- is never recolored. IRIX
//! framed those instead, in the same three colors; see [`IconState::frame`].

use crate::color::Rgb;
use crate::palette::Palette;

/// The lightest a pixel can be in every channel and still be drawing rather than body.
///
/// Between the darkest body color the artwork uses (`#ffefe5`'s blue, `0xe5`) and the lightest
/// non-body one (the default user picture's shadow, `0x86`, and the icon theme's `#999999`
/// detail), with room on both sides for antialiasing.
pub const BODY_THRESHOLD: u8 = 0xd0;

/// Which state an icon is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    /// At rest.
    Normal,
    /// The pointer is over it.
    Hover,
    /// Picked.
    Selected,
    /// Picked, in a window that does not have the keyboard. IRIX knocked the yellow back the
    /// way it knocked a resting icon back: the same checkerboard, with the selection yellow
    /// where the rest state has white -- measured off an IRIX screen, `#ffff00` and `#c9c9c9`
    /// pixel for pixel. So the selection that keys would act on is the one drawn solid. A
    /// window's business: the desktop never holds the keyboard and has no use for it.
    SelectedInactive,
    /// What stays behind where a dragged icon was picked up: IRIX left a dark gray ghost of it
    /// there for as long as the drag lasted, while the icon itself -- [`IconState::Selected`]
    /// -- followed the pointer.
    Ghost,
    /// An icon that will accept what is being dragged over it, which IRIX turned royal blue.
    DropTarget,
}

impl IconState {
    /// The state of an icon that is or is not selected and hovered. Selection wins, so a
    /// selected icon under the pointer stays yellow rather than flicking to white.
    pub fn of(selected: bool, hovered: bool) -> Self {
        if selected {
            IconState::Selected
        } else if hovered {
            IconState::Hover
        } else {
            IconState::Normal
        }
    }

    /// The color an icon's body takes at `(x, y)` of the icon.
    ///
    /// At rest that alternates pixel by pixel between white and the palette's rest gray, on a
    /// checkerboard anchored to the icon. In `classic-g10` that gray is `#c9c9c9`, exactly the
    /// one measured off IRIX; the other schemes carry IRIX's light gray at their own gamma.
    pub fn body(self, palette: &Palette, x: i32, y: i32) -> Rgb {
        match self {
            IconState::Normal if (x + y).rem_euclid(2) == 0 => Rgb(0xff_ffffff),
            IconState::Normal => palette.icon_tint,
            IconState::Hover => palette.icon_tint_hover,
            IconState::Selected => palette.icon_tint_selected,
            IconState::SelectedInactive if (x + y).rem_euclid(2) == 0 => palette.icon_tint_selected,
            IconState::SelectedInactive => palette.icon_tint,
            IconState::Ghost => palette.icon_tint_ghost,
            IconState::DropTarget => palette.icon_tint_drop,
        }
    }

    /// The color a solid shape takes when it has no body to speak of -- a coverage mask, drawn
    /// in one color: the body color without the stipple, which a mask's antialiased edge would
    /// break up into noise.
    pub fn flat(self, palette: &Palette) -> Rgb {
        match self {
            IconState::Normal => palette.icon_tint,
            IconState::Hover => palette.icon_tint_hover,
            IconState::Selected => palette.icon_tint_selected,
            // A mask cannot carry the checkerboard, and selected is what it has to say.
            IconState::SelectedInactive => palette.icon_tint_selected,
            IconState::Ghost => palette.icon_tint_ghost,
            IconState::DropTarget => palette.icon_tint_drop,
        }
    }

    /// The color of the frame around a picture that is shown as it is rather than recolored: a
    /// user's own account picture. Black at rest, then white and yellow.
    pub fn frame(self, palette: &Palette) -> Rgb {
        match self {
            IconState::Normal => palette.outer_line,
            IconState::Hover => palette.icon_tint_hover,
            IconState::Selected => palette.icon_tint_selected,
            // A frame is one color, so it cannot carry the checkerboard either.
            IconState::SelectedInactive => palette.icon_tint_selected,
            IconState::Ghost => palette.icon_tint_ghost,
            IconState::DropTarget => palette.icon_tint_drop,
        }
    }

    /// How many pixels deep that frame is, counted inward from one pixel outside the picture.
    /// A highlighted frame is two: it takes the picture's own outermost row as well.
    pub fn frame_width(self) -> i32 {
        match self {
            IconState::Normal => 1,
            IconState::Hover
            | IconState::Selected
            | IconState::SelectedInactive
            | IconState::Ghost
            | IconState::DropTarget => 2,
        }
    }

    /// One **straight** (not premultiplied) RGBA pixel of an icon at `(x, y)`, in this state.
    pub fn paint(self, palette: &Palette, x: i32, y: i32, [r, g, b, a]: [u8; 4]) -> [u8; 4] {
        if a == 0 || r.min(g).min(b) < BODY_THRESHOLD {
            return [r, g, b, a];
        }
        let (br, bg, bb) = self.body(palette, x, y).channels();
        let multiply =
            |channel: u8, by: u8| ((u32::from(channel) * u32::from(by) + 127) / 255) as u8;
        [multiply(r, br), multiply(g, bg), multiply(b, bb), a]
    }

    /// One **premultiplied** native-endian ARGB pixel, as [`crate::canvas::Canvas`] and the
    /// image cache hold them, in this state.
    ///
    /// Whether a pixel is body is decided on its straight color, so the antialiased edge of a
    /// white body -- white at partial alpha, which premultiplied is a gray -- still counts.
    pub fn paint_premultiplied(self, palette: &Palette, x: i32, y: i32, pixel: u32) -> u32 {
        let [a, r, g, b] = pixel.to_be_bytes();
        if a == 0 {
            return pixel;
        }
        let unmultiply = |channel: u8| {
            ((u32::from(channel) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
        };
        let straight = [unmultiply(r), unmultiply(g), unmultiply(b), a];
        if straight[0].min(straight[1]).min(straight[2]) < BODY_THRESHOLD {
            return pixel;
        }
        // Multiplying the premultiplied channels by the body color is the same as multiplying
        // the straight ones and premultiplying again, without the round trip's rounding.
        let (br, bg, bb) = self.body(palette, x, y).channels();
        let multiply = |channel: u8, by: u8| (u32::from(channel) * u32::from(by) + 127) / 255;
        (u32::from(a) << 24) | (multiply(r, br) << 16) | (multiply(g, bg) << 8) | multiply(b, bb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> &'static Palette {
        crate::palette::resolve(None).0
    }

    fn opaque(rgb: Rgb) -> [u8; 4] {
        let (r, g, b) = rgb.channels();
        [r, g, b, 255]
    }

    #[test]
    fn selection_wins_over_hover() {
        assert_eq!(IconState::of(true, true), IconState::Selected);
        assert_eq!(IconState::of(false, true), IconState::Hover);
        assert_eq!(IconState::of(false, false), IconState::Normal);
    }

    #[test]
    fn a_white_body_takes_the_state_color_outright() {
        let p = palette();
        let white = [255; 4];
        assert_eq!(
            IconState::Selected.paint(p, 0, 0, white),
            opaque(p.icon_tint_selected)
        );
        assert_eq!(
            IconState::Hover.paint(p, 0, 0, white),
            opaque(p.icon_tint_hover)
        );
    }

    #[test]
    fn a_ghost_is_dark_gray_and_a_drop_target_royal_blue() {
        // After the IRIX Interactive Desktop UI Guidelines' figure of icon states: what a drag
        // leaves behind is dark gray, and the icon that will take the drop is royal blue. Solid,
        // like hover and selection, with no stipple.
        let p = palette();
        let white = [255; 4];
        for (x, y) in [(0, 0), (1, 0)] {
            assert_eq!(
                IconState::Ghost.paint(p, x, y, white),
                opaque(p.icon_tint_ghost)
            );
            assert_eq!(
                IconState::DropTarget.paint(p, x, y, white),
                opaque(p.icon_tint_drop)
            );
        }
        assert_eq!(p.icon_tint_ghost, Rgb(0xff_555555));

        assert_eq!(p.icon_tint_drop, Rgb(0xff_0000ff));
    }

    #[test]
    fn selected_without_the_keyboard_is_the_rest_checkerboard_in_yellow() {
        // Measured off IRIX: a selected folder in a window without input focus is #ffff00 and
        // #c9c9c9, alternating pixel by pixel -- the rest state's stipple with yellow for white.
        let p = palette();
        let white = [255; 4];
        let state = IconState::SelectedInactive;
        assert_eq!(state.paint(p, 0, 0, white), opaque(p.icon_tint_selected));
        assert_eq!(state.paint(p, 1, 1, white), opaque(p.icon_tint_selected));
        assert_eq!(state.paint(p, 1, 0, white), opaque(p.icon_tint));
        assert_eq!(state.paint(p, 0, 1, white), opaque(p.icon_tint));
        // In the same phase as the rest state's, so an icon changing between the two does not
        // shift its pattern by a pixel.
        assert_eq!(IconState::Normal.paint(p, 1, 0, white), opaque(p.icon_tint));
    }

    #[test]
    fn at_rest_the_body_is_a_checkerboard_of_white_and_the_rest_gray() {
        let p = palette();
        let white = [255; 4];
        assert_eq!(IconState::Normal.paint(p, 0, 0, white), white);
        assert_eq!(IconState::Normal.paint(p, 1, 1, white), white);
        assert_eq!(IconState::Normal.paint(p, 1, 0, white), opaque(p.icon_tint));
        assert_eq!(IconState::Normal.paint(p, 0, 1, white), opaque(p.icon_tint));
        // Anchored to the icon, so a negative coordinate does not break the pattern.
        assert_eq!(
            IconState::Normal.paint(p, -1, 0, white),
            opaque(p.icon_tint)
        );
    }

    #[test]
    fn the_outline_the_shadow_and_colored_detail_never_change() {
        // Measured off IRIX: black and the #868686 shadow are identical in every state. The
        // icon theme's #808080 shadow, #999999 detail and the folder's tan are the same case.
        let p = palette();
        for state in [
            IconState::Normal,
            IconState::Hover,
            IconState::Selected,
            IconState::SelectedInactive,
            IconState::Ghost,
            IconState::DropTarget,
        ] {
            for pixel in [
                [0, 0, 0, 255],
                [0x86, 0x86, 0x86, 255],
                [0x80, 0x80, 0x80, 255],
                [0x99, 0x99, 0x99, 255],
                [0xde, 0xcd, 0x87, 255],
                [0xe0, 0x20, 0x20, 255],
            ] {
                assert_eq!(state.paint(p, 0, 0, pixel), pixel, "{state:?} {pixel:?}");
                assert_eq!(state.paint(p, 1, 0, pixel), pixel, "{state:?} {pixel:?}");
            }
        }
    }

    #[test]
    fn an_off_white_body_is_tinted_and_keeps_its_shading() {
        // The icon theme's text file is #ffefe5. Selected it goes yellow, a shade off pure.
        let p = palette();
        let paper = IconState::Selected.paint(p, 0, 0, [0xff, 0xef, 0xe5, 255]);
        let (r, g, b) = p.icon_tint_selected.channels();
        assert_eq!(paper[3], 255);
        assert!(paper[0] <= r && paper[1] <= g && paper[2] <= b);
        assert_ne!(paper, [0xff, 0xef, 0xe5, 255], "it changed");
    }

    #[test]
    fn the_premultiplied_form_agrees_with_the_straight_one() {
        let p = palette();
        for state in [IconState::Normal, IconState::Hover, IconState::Selected] {
            for (x, y) in [(0, 0), (1, 0)] {
                // Opaque white, opaque shadow, and a half-covered white edge.
                for (straight, premultiplied) in [
                    ([255, 255, 255, 255], 0xffff_ffff_u32),
                    ([0x80, 0x80, 0x80, 255], 0xff80_8080),
                    ([255, 255, 255, 0x80], 0x8080_8080),
                ] {
                    let [r, g, b, a] = state.paint(p, x, y, straight);
                    let expected = (u32::from(a) << 24)
                        | (((u32::from(r) * u32::from(a) + 127) / 255) << 16)
                        | (((u32::from(g) * u32::from(a) + 127) / 255) << 8)
                        | ((u32::from(b) * u32::from(a) + 127) / 255);
                    let got = state.paint_premultiplied(p, x, y, premultiplied);
                    for shift in [0, 8, 16, 24] {
                        let (e, g) = ((expected >> shift) & 0xff, (got >> shift) & 0xff);
                        assert!(
                            e.abs_diff(g) <= 1,
                            "{state:?} {straight:?}: {expected:08x} vs {got:08x}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_highlighted_frame_is_two_deep_and_a_resting_one_black() {
        let p = palette();
        assert_eq!(
            IconState::Normal.frame(p).channels(),
            p.outer_line.channels()
        );
        assert_eq!(IconState::Normal.frame_width(), 1);
        assert_eq!(IconState::Selected.frame_width(), 2);
        assert_eq!(
            IconState::Hover.frame(p).channels(),
            p.icon_tint_hover.channels()
        );
    }
}
