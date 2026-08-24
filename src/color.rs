// SPDX-License-Identifier: GPL-3.0-or-later
//! A colour.

/// A color, packed as native-endian ARGB.
///
/// Packed rather than four bytes because a [`Canvas`](crate::canvas::Canvas) is a byte run
/// over a `wl_shm` `Argb8888` buffer: a fill is then one store per pixel with no reassembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u32);

impl Rgb {
    /// An opaque color from its three channels.
    pub const fn from_channels(r: u8, g: u8, b: u8) -> Rgb {
        Rgb(0xff00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }

    /// The color's channels, for blending text coverage against a background.
    pub fn channels(self) -> (u8, u8, u8) {
        let [_, r, g, b] = self.0.to_be_bytes();
        (r, g, b)
    }

    /// Mix towards `other` by `amount` (0 = unchanged, 255 = fully `other`).
    ///
    /// This is what turns a glyph's coverage into pixels on an opaque surface: a plain
    /// interpolation is right there and there is no premultiplied alpha to get wrong. The
    /// transparent case belongs to [`Canvas::blend`](crate::canvas::Canvas::blend), which has
    /// the destination alpha to work with.
    pub fn blend(self, other: Rgb, amount: u8) -> Rgb {
        let (r0, g0, b0) = self.channels();
        let (r1, g1, b1) = other.channels();
        let mix = |from: u8, to: u8| -> u32 {
            let (from, to) = (i32::from(from), i32::from(to));
            (from + (to - from) * i32::from(amount) / 255) as u32
        };
        Rgb(0xff00_0000 | (mix(r0, r1) << 16) | (mix(g0, g1) << 8) | mix(b0, b1))
    }

    /// The color as straight (non-premultiplied) linear-ish floats, alpha last.
    ///
    /// What the compositor builds a `smithay::backend::renderer::Color32F` from -- that type
    /// is a `[f32; 4]` newtype with a `From` impl, so this is all the interop needed and
    /// `wlrix-ui` does not have to depend on smithay. Which matters: smithay is pinned to a
    /// git rev in `wlrix-compositor`, and a dependency here would make every bump of it a
    /// two-repo lockstep.
    pub fn to_f32_array(self) -> [f32; 4] {
        let (r, g, b) = self.channels();
        [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: Rgb = Rgb(0xff00_0000);
    const WHITE: Rgb = Rgb(0xffff_ffff);

    #[test]
    fn channels_unpack_argb() {
        assert_eq!(Rgb(0xff_5680ab).channels(), (0x56, 0x80, 0xab));
    }

    #[test]
    fn from_channels_packs_what_channels_unpacks() {
        assert_eq!(Rgb::from_channels(0x56, 0x80, 0xab), Rgb(0xff_5680ab));
    }

    #[test]
    fn blending_by_the_extremes_is_exact() {
        // Text at zero coverage must leave the background untouched, and at full
        // coverage must be the foreground exactly -- any drift shows as haloed glyphs.
        assert_eq!(BLACK.blend(WHITE, 0), BLACK);
        assert_eq!(BLACK.blend(WHITE, 255), WHITE);
        assert_eq!(WHITE.blend(BLACK, 255), BLACK);
    }

    #[test]
    fn blending_stays_opaque_and_in_range() {
        for amount in [0u8, 1, 64, 128, 200, 254, 255] {
            let mixed = BLACK.blend(WHITE, amount);
            assert_eq!(mixed.0 >> 24, 0xff, "alpha lost at {amount}");
            let (r, _, _) = mixed.channels();
            assert_eq!(r, amount, "channel should track coverage at {amount}");
        }
    }

    #[test]
    fn f32_array_matches_the_generated_constants_it_replaces() {
        // `#555555` was `Color32F::new(0.333333, 0.333333, 0.333333, 1.0)` in the
        // compositor's generated palette. Same value, computed rather than baked.
        let [r, g, b, a] = Rgb(0xff_555555).to_f32_array();
        assert!((r - 0.333333).abs() < 1e-5);
        assert_eq!(r, g);
        assert_eq!(g, b);
        assert_eq!(a, 1.0);
        assert_eq!(WHITE.to_f32_array(), [1.0, 1.0, 1.0, 1.0]);
    }
}
