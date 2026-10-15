// SPDX-License-Identifier: GPL-3.0-or-later
//! Colors and geometry, generated from the shared palette.
//!
//! [`generated`] comes from `wlrix-assets/tools/palettegen`, out of the same JSON the
//! Avalonia theme's scheme dictionaries are built from, so the Rust components and the C#
//! applications render one Indigo Magic. Edit the JSON and re-run `just palette`; never
//! edit the generated file.
//!
//! # Why this is a value and not a wall of constants
//!
//! It used to be three: thirteen `const`s for the compositor in smithay's `Color32F`,
//! twenty-three for the greeter and twelve for the desktop in a packed `u32`, each list
//! hand-picked in the generator. Nothing could change palette while running, the same color
//! had three names, and the window chrome -- the most visible surface in the desktop -- was
//! bound to none of them and carried eight literals of its own.
//!
//! A [`Palette`] is a plain `Copy` struct of `&'static` data, so drawing code takes a
//! `&'static Palette` and nothing that holds one grows a lifetime parameter.
//!
//! # Choosing one
//!
//! [`by_id`] answers to what a config file or a `--palette` flag names, and returns `None`
//! rather than a fallback -- only the caller knows whether an unknown id deserves a warning
//! and [`DEFAULT`], or is a hard error. [`resolve`] is the warn-and-fall-back form, which is
//! what every component actually wants: a mistyped scheme name must not leave someone
//! staring at a black screen.

mod generated;

pub use generated::{ALL, DEFAULT, Metrics, Palette};

/// The scheme a config file or a `--palette` flag names.
pub fn by_id(id: &str) -> Option<&'static Palette> {
    generated::by_id(id)
}

/// The scheme `id` names, or [`DEFAULT`] if it names nothing.
///
/// Returns the reason alongside, so the caller can log it in its own voice rather than this
/// crate printing to a stream it does not own. `None` for `id` is not a failure: it is a
/// component that was given no scheme, which is the ordinary case.
pub fn resolve(id: Option<&str>) -> (&'static Palette, Option<UnknownPalette>) {
    match id {
        None | Some("") => (DEFAULT, None),
        Some(id) => match by_id(id) {
            Some(palette) => (palette, None),
            None => (DEFAULT, Some(UnknownPalette(id.to_string()))),
        },
    }
}

/// An id that named no shipped scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownPalette(pub String);

impl std::fmt::Display for UnknownPalette {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown palette \"{}\"; known: ", self.0)?;
        for (i, palette) in ALL.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            f.write_str(palette.id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;

    /// The values the three generated `palette.rs` files held before this crate existed.
    ///
    /// Not a tautology against the generator: this is the record of what the desktop looked
    /// like when the palette was three hand-picked `const` lists, and it is what says the
    /// rewrite changed no pixel. The last four were literals typed into
    /// `wlrix-compositor/src/decoration.rs` and are the reason `toward()` exists.
    #[test]
    fn classic_matches_the_constants_it_replaced() {
        let p = by_id("classic").expect("classic ships");
        assert_eq!(p.desktop, Rgb(0xff_555555));
        assert_eq!(p.panel, Rgb(0xff_c1c1c1));
        assert_eq!(p.face, Rgb(0xff_999999));
        assert_eq!(p.face_top_shadow, Rgb(0xff_e5e5e5));
        assert_eq!(p.face_bottom_shadow, Rgb(0xff_545454));
        assert_eq!(p.armed, Rgb(0xff_828282));
        assert_eq!(p.text_field_background, Rgb(0xff_b98e8e));
        assert_eq!(p.login_background, Rgb(0xff_5680ab));
        assert_eq!(p.outer_line, Rgb(0xff_000000));
        assert_eq!(p.icon_tint, Rgb(0xff_aaaaaa));
        assert_eq!(p.select_fill, Rgb(0xff_ffff00));
        assert_eq!(p.drag_outline, Rgb(0xff_ff0000));
        // The window chrome, previously `c(165, 159, 128)` and friends.
        assert_eq!(p.title_active, Rgb(0xff_a59f80));
        assert_eq!(p.title_active_top_shadow, Rgb(0xff_d9d6c9));
        assert_eq!(p.title_active_bottom_shadow, Rgb(0xff_635f4d));
        assert_eq!(p.title_active_armed, Rgb(0xff_847f66));
        assert_eq!(p.title_inactive, Rgb(0xff_808080));
        assert_eq!(p.title_inactive_top_shadow, Rgb(0xff_c9c9c9));
        assert_eq!(p.title_inactive_bottom_shadow, Rgb(0xff_4d4d4d));
        assert_eq!(p.title_inactive_armed, Rgb(0xff_666666));
        assert_eq!(p.icon_well, Rgb(0xff_464a52));
        assert_eq!(p.metrics.shadow_thickness_default, 2);
        assert_eq!(p.metrics.scroll_bar_size, 18);
    }

    #[test]
    fn gotham_is_dark_and_classic_is_not() {
        assert!(by_id("gotham").expect("gotham ships").dark);
        assert!(!by_id("classic").expect("classic ships").dark);
    }

    #[test]
    fn every_scheme_is_reachable_by_its_own_id() {
        for palette in ALL {
            assert_eq!(by_id(palette.id), Some(*palette), "{}", palette.id);
        }
        assert_eq!(by_id("nothing-like-this"), None);
    }

    #[test]
    fn the_default_is_classic() {
        assert_eq!(DEFAULT.id, "classic");
        assert_eq!(resolve(None).0.id, "classic");
        assert_eq!(resolve(Some("")).0.id, "classic");
    }

    #[test]
    fn an_unknown_id_falls_back_and_says_so() {
        let (palette, why) = resolve(Some("indigo-magic"));
        assert_eq!(
            palette, DEFAULT,
            "a typo must not leave a component unpainted"
        );
        let why = why.expect("the caller has to be able to warn about it");
        let text = why.to_string();
        assert!(text.contains("indigo-magic"), "{text}");
        assert!(
            text.contains("classic") && text.contains("gotham"),
            "{text}"
        );
    }

    #[test]
    fn role_and_set_role_agree_with_the_fields() {
        // The generated accessors are what a future loader writes through, so they have to
        // reach the same storage the drawing code reads.
        let mut p = *DEFAULT;
        assert_eq!(p.role("face"), Some(p.face));
        assert_eq!(
            p.role("titleActiveTopShadow"),
            Some(p.title_active_top_shadow)
        );
        assert_eq!(p.role("nonesuch"), None);

        assert!(p.set_role("face", Rgb(0xff_010203)));
        assert_eq!(p.face, Rgb(0xff_010203));
        assert!(!p.set_role("nonesuch", Rgb(0)));
    }

    #[test]
    fn every_role_name_resolves() {
        let p = DEFAULT;
        for name in Palette::ROLE_NAMES {
            assert!(p.role(name).is_some(), "{name} is listed but not readable");
        }
        assert_eq!(Palette::ROLE_NAMES.len(), 68);
    }
}
