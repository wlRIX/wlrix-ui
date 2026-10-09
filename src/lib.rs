// SPDX-License-Identifier: GPL-3.0-or-later
//! The drawing primitives and the color palette shared by wlRIX's Rust components.
//!
//! `wlrix-compositor` draws window frames and menus, `wlrix-greeter` draws a Motif login
//! dialog and `wlrix-desktop` draws icons and a menu. All three draw the same IRIX chrome,
//! and until this crate existed all three had their own copy of it -- copied by hand,
//! annotated as copied, and slowly diverging. See `_docs/case-for-ui.md`, which named the
//! condition for pulling them out: a third consumer.
//!
//! # What is here
//!
//! - [`color`] and [`canvas`]: a packed ARGB color and a borrowed `wl_shm` surface.
//! - [`bevel`]: Motif bevel geometry with no opinion about where the pixels go, so the
//!   clients' pixel fills and the compositor's GPU quads come from one rule.
//! - [`motif`] and [`widget`]: the client-side sink for that geometry, and the widgets built
//!   on it.
//! - [`text`]: one cosmic-text setup for all three, including the system-font-directory
//!   workaround the greeter needs under greetd and the compositor silently lacked.
//! - [`palette`]: the generated color schemes, as values a running program can swap.
//! - [`icon_state`]: how an icon shows rest, hover and selection -- the one rule the desktop's
//!   icons and the greeter's user pictures both follow.
//!
//! # What is deliberately not here
//!
//! Layout, hit-testing and event handling. They are what each component *is*, and a shared
//! widget that owned them would have to know about three different event loops.
//!
//! Also not here: the pidfile, signal, XDG and logging modules, which are copied between the
//! components in the same way and for the same reason. They are not UI, and they belong in a
//! `wlrix-common` rather than being folded in here because this crate is the one place they
//! would fit.

pub mod bevel;
pub mod canvas;
pub mod color;
pub mod icon_state;
#[cfg(any(feature = "png", feature = "svg"))]
pub mod image;
pub mod mask;
pub mod motif;
pub mod palette;

#[cfg(feature = "text")]
pub mod text;
#[cfg(feature = "text")]
pub mod widget;

pub use canvas::{Canvas, Rect};
pub use color::Rgb;
pub use icon_state::IconState;
pub use palette::Palette;
