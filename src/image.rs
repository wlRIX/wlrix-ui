// SPDX-License-Identifier: GPL-3.0-or-later
//! Turning icon files into pixels.
//!
//! Two sources, one answer: an SVG goes through `resvg`, a PNG through `png`, and both come
//! back as an [`Image`] of premultiplied `Argb8888` at the size asked for -- the same form the
//! `wl_shm` buffer wants, so drawing one is a straight composite with no conversion.
//!
//! Resolving an `Icon=` *name* to a file is not here: that is the icon-theme spec --
//! index.theme, inheritance, size directories, the `/usr/share/pixmaps` fallback -- and it
//! belongs to whichever component reads `.desktop` files. This takes a path.
//!
//! Two shapes of request, see [`Fit`]: a square icon at a theme size ([`Images::load`]), and
//! artwork that has to fill a fixed rectangle with no gap ([`Images::load_cover`]).
//!
//! SVG is rendered *at* the requested size rather than rendered once and scaled, so an icon is
//! crisp at whatever the configured icon size happens to be. A PNG is resampled, since there is
//! nothing else to be done with it.
//!
//! Loading is cached by (path, size, fit). An icon is redrawn on every hover and every drag
//! frame, and re-rasterizing an SVG at 60Hz would be absurd.
//!
//! The two decoders are separate features. `png` alone is a small dependency; `svg` drags in
//! resvg, usvg, tiny-skia and a font database, which a consumer that only ever reads PNG
//! should not pay for. `image` is both, which is what every consumer of this module asked for
//! before the split.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::canvas::Canvas;
use crate::color::Rgb;

/// A decoded icon: premultiplied `Argb8888`, ready to composite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: i32,
    pub height: i32,
    /// `width * height` pixels, packed as native-endian ARGB with the color channels already
    /// multiplied by alpha.
    pixels: Vec<u32>,
}

impl Image {
    /// One pixel, or fully transparent off the edge.
    pub fn get(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return 0;
        }
        self.pixels[(y * self.width + x) as usize]
    }

    /// The raw pixels, row-major, premultiplied native-endian ARGB.
    ///
    /// For a consumer that *uploads* the image rather than compositing it through
    /// [`Image::draw`] -- a compositor turning one into a texture wants bytes, not a canvas.
    ///
    /// **Native-endian ARGB is `Argb8888` on a little-endian machine**, because the byte order
    /// of `0xAARRGGBB` there is `B, G, R, A`, which is what that DRM fourcc names. Getting this
    /// backwards is not a compile error and not obviously wrong on screen: `wlrix-compositor`
    /// had its cursors as `Abgr8888` for months, and the swap only showed once a theme used a
    /// color that was not black, white or gray. The alpha is already multiplied in, which is
    /// what those DRM formats mean and what every blend downstream assumes.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Draw the image with its top-left at `(x, y)`, compositing over what is there.
    ///
    /// Source-over in premultiplied space, matching [`Canvas::blend`] -- an icon's antialiased
    /// edge has to stay partly transparent so the wallpaper shows through it.
    pub fn draw(&self, canvas: &mut Canvas, x: i32, y: i32) {
        for row in 0..self.height {
            for column in 0..self.width {
                let pixel = self.get(column, row);
                let alpha = (pixel >> 24) as u8;
                if alpha == 0 {
                    continue;
                }
                canvas.blend_premultiplied(x + column, y + row, pixel);
            }
        }
    }

    /// The image with every color channel multiplied by `tint`, alpha untouched.
    ///
    /// Multiply rather than replace, because replacing flattens a drawing into one solid
    /// shape. The magic carpet is a white body with a black outline and a shadow: multiplied
    /// by yellow the body goes yellow and the outline stays black, which is the tint reading
    /// as a highlight. Replaced by yellow it would be a yellow blob.
    ///
    /// This is what gives a launcher the same gray/white/yellow states as the drawn-in-code
    /// icons **without** recoloring the application's own artwork, which wants its own
    /// colors: the carpet is tinted, the symbol standing on it is not.
    pub fn multiplied(&self, tint: Rgb) -> Image {
        let (r, g, b) = tint.channels();
        let pixels = self
            .pixels
            .iter()
            .map(|pixel| {
                // Premultiplied in, premultiplied out: scaling a channel down can never take
                // it above the alpha it started under.
                let scale = |channel: u32, by: u8| (channel * u32::from(by) + 127) / 255;
                (pixel & 0xff00_0000)
                    | (scale((pixel >> 16) & 0xff, r) << 16)
                    | (scale((pixel >> 8) & 0xff, g) << 8)
                    | scale(pixel & 0xff, b)
            })
            .collect();
        Image {
            width: self.width,
            height: self.height,
            pixels,
        }
    }

    /// One pixel, clamping to the edge rather than falling off it.
    ///
    /// What a resampler wants, and the opposite of [`Image::get`]: reading transparent black
    /// past the edge would darken and fade the outermost row and column of every scaled image,
    /// since a filter kernel straddles the boundary there. Only the PNG path resamples -- an
    /// SVG is rendered at the size asked for instead.
    #[cfg(feature = "png")]
    fn clamped(&self, x: i32, y: i32) -> u32 {
        let x = x.clamp(0, self.width - 1);
        let y = y.clamp(0, self.height - 1);
        self.pixels[(y * self.width + x) as usize]
    }
}

/// How a drawing is made to fill the box it was asked for.
///
/// Also the cache discriminant: `load(p, n)` and `load_cover(p, n, n)` ask for the same
/// dimensions and want different pixels, so the box alone cannot key them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Fit {
    /// A square icon at a size the theme lookup asked for. An SVG is scaled to fit and
    /// centered; a PNG is scaled to the box outright. That the two disagree about a
    /// non-square source is longstanding and deliberate-by-now: the theme lookup asks for
    /// the nearest size it has, so a PNG that reaches here is square already.
    Icon,
    /// Fill the box entirely, keeping the aspect ratio and cropping whatever overflows.
    Cover,
}

/// What was asked for. See [`Fit`] for why the mode is part of it.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    path: PathBuf,
    width: i32,
    height: i32,
    fit: Fit,
}

/// Icons already loaded, keyed by what was asked for.
#[derive(Default)]
pub struct Images {
    /// `None` records a file that failed to load, so a broken icon is not retried every frame.
    cache: HashMap<Key, Option<Image>>,
}

impl Images {
    /// Load `path` at `size` square, from the cache when it has been seen before.
    pub fn load(&mut self, path: &Path, size: i32) -> Option<&Image> {
        self.cached(
            Key {
                path: path.to_path_buf(),
                width: size,
                height: size,
                fit: Fit::Icon,
            },
            |key| decode(&key.path, key.width, key.height, key.fit),
        )
    }

    /// Load `path` scaled to fill `width` x `height` exactly, cropping whatever does not fit.
    ///
    /// Unlike [`Images::load`], which wants a square icon at a theme size, this is for artwork
    /// that has to *fill* a fixed rectangle: a minimized window's picture in its 4Dwm tile,
    /// where a gap at the edge would show the well through and read as a drawing bug. The
    /// scale is the larger of the two ratios, so nothing is ever left uncovered, and the
    /// overflow is trimmed evenly from both sides, so the subject stays centered.
    pub fn load_cover(&mut self, path: &Path, width: i32, height: i32) -> Option<&Image> {
        self.cached(
            Key {
                path: path.to_path_buf(),
                width,
                height,
                fit: Fit::Cover,
            },
            |key| decode(&key.path, key.width, key.height, key.fit),
        )
    }

    /// Load artwork the program carries itself, cached under a made-up `key`.
    ///
    /// The magic carpets are `include_bytes!`d rather than read from a data directory, so they
    /// cannot go missing from an installed desktop -- but they still want the same cache as
    /// everything else, since they are re-rasterized on every frame otherwise.
    #[cfg(feature = "svg")]
    pub fn load_bytes(&mut self, key: &Path, bytes: &[u8], size: i32) -> Option<&Image> {
        self.cached(
            Key {
                path: key.to_path_buf(),
                width: size,
                height: size,
                fit: Fit::Icon,
            },
            |entry| {
                (entry.width > 0)
                    .then(|| from_svg(bytes, entry.width))
                    .flatten()
            },
        )
    }

    /// Forget everything. Called when the icon size changes, which invalidates every entry.
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// The entry for `key`, decoding it with `load` on a miss.
    ///
    /// A failed decode is cached as `None` too: it will fail again, and retrying it means a
    /// decode attempt every frame for nothing.
    fn cached(&mut self, key: Key, load: impl FnOnce(&Key) -> Option<Image>) -> Option<&Image> {
        if !self.cache.contains_key(&key) {
            let loaded = load(&key);
            self.cache.insert(key.clone(), loaded);
        }
        self.cache.get(&key)?.as_ref()
    }
}

/// The eight bytes every PNG starts with.
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Decode one file into a `width` x `height` image, picking the reader by what the bytes
/// actually are.
///
/// The extension is a hint, not evidence -- themes do ship a `.png` that is really an SVG --
/// so the content is sniffed first and the extension only breaks ties.
fn decode(path: &Path, width: i32, height: i32, fit: Fit) -> Option<Image> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.starts_with(PNG_MAGIC) {
        return png_image(&bytes, width, height, fit);
    }
    svg_image(&bytes, width, height, fit)
}

/// The PNG branch of [`decode`], or nothing at all when the decoder is not compiled in.
#[cfg(feature = "png")]
fn png_image(bytes: &[u8], width: i32, height: i32, fit: Fit) -> Option<Image> {
    let native = decode_png(bytes)?;
    Some(match fit {
        Fit::Icon => stretch(&native, width, height),
        Fit::Cover => cover(&native, width, height),
    })
}

#[cfg(not(feature = "png"))]
fn png_image(_bytes: &[u8], _width: i32, _height: i32, _fit: Fit) -> Option<Image> {
    None
}

/// The SVG branch of [`decode`], or nothing at all when the decoder is not compiled in.
#[cfg(feature = "svg")]
fn svg_image(bytes: &[u8], width: i32, height: i32, fit: Fit) -> Option<Image> {
    render_svg(bytes, width, height, fit)
}

#[cfg(not(feature = "svg"))]
fn svg_image(_bytes: &[u8], _width: i32, _height: i32, _fit: Fit) -> Option<Image> {
    None
}

/// Rasterize an SVG at exactly `size` square.
///
/// Aspect ratio is kept: a non-square drawing is scaled to fit and centered, so a wide icon
/// does not come out stretched.
#[cfg(feature = "svg")]
fn from_svg(bytes: &[u8], size: i32) -> Option<Image> {
    render_svg(bytes, size, size, Fit::Icon)
}

/// Rasterize an SVG into a `width` x `height` pixmap.
///
/// [`Fit::Icon`] scales to fit and leaves the remainder transparent; [`Fit::Cover`] scales to
/// fill and lets the overflow fall outside the pixmap. Either way the drawing is centered, and
/// either way it is rendered *at* this size rather than rendered once and scaled.
#[cfg(feature = "svg")]
fn render_svg(bytes: &[u8], width: i32, height: i32, fit: Fit) -> Option<Image> {
    use resvg::tiny_skia;
    use resvg::usvg;

    if width <= 0 || height <= 0 {
        return None;
    }
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &options).ok()?;

    let drawing = tree.size();
    if drawing.width() <= 0.0 || drawing.height() <= 0.0 {
        return None;
    }
    let (across, down) = (
        width as f32 / drawing.width(),
        height as f32 / drawing.height(),
    );
    let scale = match fit {
        Fit::Icon => across.min(down),
        Fit::Cover => across.max(down),
    };
    let offset_x = (width as f32 - drawing.width() * scale) / 2.0;
    let offset_y = (height as f32 - drawing.height() * scale) / 2.0;

    let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32)?;
    let transform =
        tiny_skia::Transform::from_translate(offset_x, offset_y).pre_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia hands back premultiplied RGBA; the canvas wants premultiplied ARGB.
    let pixels = pixmap
        .pixels()
        .iter()
        .map(|pixel| {
            (u32::from(pixel.alpha()) << 24)
                | (u32::from(pixel.red()) << 16)
                | (u32::from(pixel.green()) << 8)
                | u32::from(pixel.blue())
        })
        .collect();

    Some(Image {
        width,
        height,
        pixels,
    })
}

/// Decode a PNG at its own size, as premultiplied ARGB.
///
/// Separate from the scaling so [`cover`] and [`stretch`] can share it -- the color-type
/// handling below is the part that took finding, and it must not be written twice.
#[cfg(feature = "png")]
fn decode_png(bytes: &[u8]) -> Option<Image> {
    // `Cursor`, because png 0.18's decoder wants `BufRead + Seek` and a bare slice is neither.
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // Expand palettes and low bit depths, and narrow 16-bit channels to 8. Themes ship all of
    // these -- `/usr/share/pixmaps/archlinux-logo.png` is 8-bit *colormap* -- and handling only
    // plain RGB/RGBA meant those icons silently failed to load.
    decoder.set_transformations(png::Transformations::normalize_to_color8());

    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;

    let (width, height) = (info.width as i32, info.height as i32);
    if width <= 0 || height <= 0 {
        return None;
    }
    // After normalizing there are only these four, all 8 bits per channel.
    let (channels, gray) = match info.color_type {
        png::ColorType::Rgba => (4, false),
        png::ColorType::Rgb => (3, false),
        png::ColorType::GrayscaleAlpha => (2, true),
        png::ColorType::Grayscale => (1, true),
        png::ColorType::Indexed => return None,
    };

    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in 0..height {
        for column in 0..width {
            let offset = ((row * width + column) * channels) as usize;
            // Gray expands to three equal channels; the alpha is the last one when there is
            // one, and opaque otherwise.
            let (red, green, blue) = if gray {
                let gray = buffer[offset];
                (gray, gray, gray)
            } else {
                (buffer[offset], buffer[offset + 1], buffer[offset + 2])
            };
            let alpha = match channels {
                4 | 2 => buffer[offset + channels as usize - 1],
                _ => 255,
            };
            let premultiply = |channel: u8| (u32::from(channel) * u32::from(alpha) + 127) / 255;
            pixels.push(
                (u32::from(alpha) << 24)
                    | (premultiply(red) << 16)
                    | (premultiply(green) << 8)
                    | premultiply(blue),
            );
        }
    }

    Some(Image {
        width,
        height,
        pixels,
    })
}

/// Scale `source` to `width` x `height` outright, aspect ratio and all.
///
/// Nearest-neighbor: the theme lookup asks for the size we want, so the scaling here is
/// usually small or absent, and a nicer filter is not worth the code. [`cover`] is the one
/// that filters, because it is handed whatever artwork a user dropped in a directory.
#[cfg(feature = "png")]
fn stretch(source: &Image, width: i32, height: i32) -> Image {
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in 0..height {
        let source_y = (row * source.height / height).clamp(0, source.height - 1);
        for column in 0..width {
            let source_x = (column * source.width / width).clamp(0, source.width - 1);
            pixels.push(source.clamped(source_x, source_y));
        }
    }
    Image {
        width,
        height,
        pixels,
    }
}

/// Scale `source` to cover `width` x `height`, cropping the overflow evenly from both sides.
///
/// Bilinear, unlike [`stretch`]: the source here is artwork of whatever size someone supplied,
/// and the target is the output's *physical* pixels, so a fractional display scale routinely
/// asks for 128x101 from an 85x67 drawing. Nearest at that ratio doubles some rows and not
/// others, which on a photographic icon reads as damage rather than as scaling.
///
/// Sampling in premultiplied space is the correct space for it: interpolating the color
/// channels separately from alpha is what puts halos around a transparent edge.
///
/// A source already at the target size and aspect passes through untouched -- the sample points
/// land exactly on pixel centers and every weight comes out 0 or 1.
#[cfg(feature = "png")]
fn cover(source: &Image, width: i32, height: i32) -> Image {
    // The largest scale that leaves no gap, and the source rectangle that maps onto the whole
    // target under it. `min` guards the rounding: the crop can never exceed the source.
    let scale = (width as f64 / source.width as f64).max(height as f64 / source.height as f64);
    let crop_w = (width as f64 / scale).min(source.width as f64);
    let crop_h = (height as f64 / scale).min(source.height as f64);
    let left = (source.width as f64 - crop_w) / 2.0;
    let top = (source.height as f64 - crop_h) / 2.0;

    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in 0..height {
        // Pixel centers, hence the half-pixel in and the half-pixel back out: sampling at the
        // pixel's corner instead shifts the whole image half a pixel up and left.
        let y = top + (row as f64 + 0.5) * crop_h / height as f64 - 0.5;
        for column in 0..width {
            let x = left + (column as f64 + 0.5) * crop_w / width as f64 - 0.5;
            pixels.push(bilinear(source, x, y));
        }
    }
    Image {
        width,
        height,
        pixels,
    }
}

/// One bilinearly interpolated pixel of `source`, clamping at its edges.
#[cfg(feature = "png")]
fn bilinear(source: &Image, x: f64, y: f64) -> u32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as i32, y0 as i32);

    let corners = [
        (source.clamped(x0, y0), (1.0 - fx) * (1.0 - fy)),
        (source.clamped(x0 + 1, y0), fx * (1.0 - fy)),
        (source.clamped(x0, y0 + 1), (1.0 - fx) * fy),
        (source.clamped(x0 + 1, y0 + 1), fx * fy),
    ];

    let mut out = 0u32;
    for shift in [24, 16, 8, 0] {
        let channel: f64 = corners
            .iter()
            .map(|(pixel, weight)| ((pixel >> shift) & 0xff) as f64 * weight)
            .sum();
        out |= (channel.round().clamp(0.0, 255.0) as u32) << shift;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 SVG: a solid red square filling its whole viewBox.
    #[cfg(feature = "svg")]
    const RED_SQUARE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
        <rect x="0" y="0" width="10" height="10" fill="#ff0000"/></svg>"##;

    /// A white body with a black outline and a half-transparent black shadow -- the shape of
    /// the artwork `multiplied` exists for, without depending on another repo's assets.
    #[cfg(feature = "svg")]
    const BADGE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
        <rect x="1" y="1" width="8" height="8" fill="#ffffff" stroke="#000000"
              stroke-width="1"/>
        <rect x="3" y="3" width="4" height="4" fill="#000000" fill-opacity="0.75"/></svg>"##;

    /// A wide SVG, for the aspect-ratio check.
    #[cfg(feature = "svg")]
    const WIDE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10">
        <rect x="0" y="0" width="20" height="10" fill="#00ff00"/></svg>"##;

    fn alpha(pixel: u32) -> u8 {
        (pixel >> 24) as u8
    }

    #[test]
    #[cfg(feature = "svg")]
    fn an_svg_rasterizes_at_the_size_asked_for() {
        let image = from_svg(RED_SQUARE, 32).expect("should rasterize");
        assert_eq!((image.width, image.height), (32, 32));
        // The middle is opaque red.
        let middle = image.get(16, 16);
        assert_eq!(alpha(middle), 255);
        assert_eq!((middle >> 16) & 0xff, 0xff, "red channel");
        assert_eq!(middle & 0xff, 0x00, "blue channel");
    }

    #[test]
    #[cfg(feature = "svg")]
    fn the_same_svg_at_two_sizes_gives_two_sizes() {
        // Rendered at the requested size, not rendered once and scaled.
        for size in [16, 24, 48, 64, 128] {
            let image = from_svg(RED_SQUARE, size).expect("should rasterize");
            assert_eq!((image.width, image.height), (size, size));
        }
    }

    #[test]
    #[cfg(feature = "svg")]
    fn a_wide_svg_is_letterboxed_rather_than_stretched() {
        let image = from_svg(WIDE, 40).expect("should rasterize");
        assert_eq!((image.width, image.height), (40, 40));
        // 2:1 into a square: the drawing occupies the middle 20 rows, so the top is empty.
        assert_eq!(alpha(image.get(20, 2)), 0, "the top should be empty");
        assert_eq!(alpha(image.get(20, 20)), 255, "the middle should be drawn");
        assert_eq!(alpha(image.get(20, 37)), 0, "the bottom should be empty");
    }

    #[test]
    #[cfg(feature = "svg")]
    fn a_wide_svg_covering_a_square_fills_it_instead() {
        // The point of `Fit::Cover`: the same drawing that letterboxes above must leave no
        // gap here, because the box it fills is a well with its own color behind it.
        let image = render_svg(WIDE, 40, 40, Fit::Cover).expect("should rasterize");
        assert_eq!((image.width, image.height), (40, 40));
        for y in [1, 20, 38] {
            assert_eq!(alpha(image.get(20, y)), 255, "row {y} should be drawn");
        }
    }

    /// A tiny PNG in the given color type, built here so the test does not depend on what
    /// happens to be installed.
    #[cfg(feature = "png")]
    fn png_bytes(color: png::ColorType, depth: png::BitDepth) -> Vec<u8> {
        png_bytes_sized(color, depth, 4, 4)
    }

    /// The same, at a chosen size, with a per-pixel pattern so cropping can be seen.
    ///
    /// Every pixel's red channel is its column and its green its row, so a test can read a
    /// decoded pixel and say exactly which source pixel it came from.
    #[cfg(feature = "png")]
    fn png_bytes_sized(
        color: png::ColorType,
        depth: png::BitDepth,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(color);
            encoder.set_depth(depth);
            if color == png::ColorType::Indexed {
                // A four-color palette, and pixels indexing into it.
                encoder.set_palette(vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
            }
            let mut writer = encoder.write_header().expect("header");
            let count = (width * height) as usize;
            let data: Vec<u8> = match color {
                png::ColorType::Indexed => (0..count).map(|i| (i % 4) as u8).collect(),
                png::ColorType::Grayscale => vec![128; count],
                png::ColorType::GrayscaleAlpha => vec![128; count * 2],
                png::ColorType::Rgb => (0..count)
                    .flat_map(|i| {
                        let (x, y) = (i as u32 % width, i as u32 / width);
                        [x as u8, y as u8, 200]
                    })
                    .collect(),
                png::ColorType::Rgba => (0..count)
                    .flat_map(|i| {
                        let (x, y) = (i as u32 % width, i as u32 / width);
                        [x as u8, y as u8, 200, 255]
                    })
                    .collect(),
            };
            writer.write_image_data(&data).expect("write");
        }
        out
    }

    #[cfg(feature = "png")]
    fn red(pixel: u32) -> u32 {
        (pixel >> 16) & 0xff
    }

    #[cfg(feature = "png")]
    fn green(pixel: u32) -> u32 {
        (pixel >> 8) & 0xff
    }

    #[test]
    #[cfg(feature = "png")]
    fn every_png_flavour_a_theme_ships_decodes() {
        // The one that caught this: `/usr/share/pixmaps/archlinux-logo.png` is 8-bit
        // *colormap*, and rejecting palettes meant it silently failed to load. Grayscale and
        // 16-bit turn up in themes too.
        for (color, depth) in [
            (png::ColorType::Rgba, png::BitDepth::Eight),
            (png::ColorType::Rgb, png::BitDepth::Eight),
            (png::ColorType::Indexed, png::BitDepth::Eight),
            (png::ColorType::Grayscale, png::BitDepth::Eight),
            (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight),
        ] {
            let bytes = png_bytes(color, depth);
            let image = png_image(&bytes, 16, 16, Fit::Icon)
                .unwrap_or_else(|| panic!("{color:?} at {depth:?} failed to decode"));
            assert_eq!((image.width, image.height), (16, 16), "{color:?}");
            assert!(
                image.pixels.iter().any(|pixel| alpha(*pixel) != 0),
                "{color:?} decoded to nothing"
            );
        }
    }

    #[test]
    #[cfg(feature = "png")]
    fn a_source_already_at_the_target_passes_through_untouched() {
        // The common case for the minimize icons: the artwork is authored at the well's size,
        // so covering it must be a copy rather than a resample. If the sample points were off
        // by half a pixel this would blur every one of them.
        let bytes = png_bytes_sized(png::ColorType::Rgb, png::BitDepth::Eight, 8, 6);
        let native = decode_png(&bytes).expect("decode");
        let covered = cover(&native, 8, 6);
        assert_eq!(covered, native);
    }

    #[test]
    #[cfg(feature = "png")]
    fn a_source_wider_than_the_target_is_cropped_left_and_right() {
        // 12x6 into 8x6: the height already fits, so the scale is 1 and two columns come off
        // each side. Column 0 of the result must be column 2 of the source, and the rows must
        // be untouched.
        let bytes = png_bytes_sized(png::ColorType::Rgb, png::BitDepth::Eight, 12, 6);
        let native = decode_png(&bytes).expect("decode");
        let covered = cover(&native, 8, 6);
        assert_eq!((covered.width, covered.height), (8, 6));
        for row in 0..6 {
            for column in 0..8 {
                let pixel = covered.get(column, row);
                assert_eq!(red(pixel), (column + 2) as u32, "column at {column},{row}");
                assert_eq!(green(pixel), row as u32, "row at {column},{row}");
            }
        }
    }

    #[test]
    #[cfg(feature = "png")]
    fn a_source_taller_than_the_target_is_cropped_top_and_bottom() {
        let bytes = png_bytes_sized(png::ColorType::Rgb, png::BitDepth::Eight, 8, 10);
        let native = decode_png(&bytes).expect("decode");
        let covered = cover(&native, 8, 6);
        assert_eq!((covered.width, covered.height), (8, 6));
        for row in 0..6 {
            for column in 0..8 {
                let pixel = covered.get(column, row);
                assert_eq!(red(pixel), column as u32, "column at {column},{row}");
                assert_eq!(green(pixel), (row + 2) as u32, "row at {column},{row}");
            }
        }
    }

    #[test]
    #[cfg(feature = "png")]
    fn covering_never_leaves_a_gap() {
        // The property the whole mode exists for: whatever the source aspect, every pixel of
        // the target is covered, so the well's color never shows through at an edge.
        for (source_w, source_h) in [(87, 67), (85, 68), (80, 63), (200, 20), (20, 200)] {
            let bytes = png_bytes_sized(
                png::ColorType::Rgba,
                png::BitDepth::Eight,
                source_w,
                source_h,
            );
            let native = decode_png(&bytes).expect("decode");
            for (width, height) in [(85, 67), (128, 101)] {
                let covered = cover(&native, width, height);
                assert_eq!((covered.width, covered.height), (width, height));
                assert!(
                    covered.pixels.iter().all(|pixel| alpha(*pixel) == 255),
                    "{source_w}x{source_h} -> {width}x{height} left a transparent pixel"
                );
            }
        }
    }

    #[test]
    #[cfg(feature = "png")]
    fn a_real_palette_png_from_the_system_decodes() {
        // Belt and braces against the synthetic one above: if this file is not installed the
        // test skips rather than failing on a machine that simply lacks it.
        let path = Path::new("/usr/share/pixmaps/archlinux-logo.png");
        if !path.is_file() {
            return;
        }
        let image = decode(path, 48, 48, Fit::Icon).expect("a palette PNG should decode");
        assert_eq!((image.width, image.height), (48, 48));
        assert!(image.pixels.iter().any(|pixel| alpha(*pixel) != 0));
    }

    #[test]
    fn nonsense_does_not_panic() {
        assert!(decode(Path::new("/nonexistent/icon.svg"), 32, 32, Fit::Icon).is_none());
        assert!(decode(Path::new("/nonexistent/icon.png"), 32, 32, Fit::Cover).is_none());
        #[cfg(feature = "svg")]
        assert!(from_svg(b"not an svg at all", 32).is_none());
        #[cfg(feature = "png")]
        assert!(png_image(b"\x89PNG\r\n\x1a\ngarbage", 32, 32, Fit::Icon).is_none());
        // A zero or negative size is a configuration accident, not a crash.
        assert!(decode(Path::new("/nonexistent"), 0, 0, Fit::Icon).is_none());
        assert!(decode(Path::new("/nonexistent"), 16, 0, Fit::Cover).is_none());
    }

    #[test]
    #[cfg(feature = "svg")]
    fn multiplying_keeps_the_shape_and_the_dark_parts() {
        let image = from_svg(RED_SQUARE, 16).expect("should rasterize");
        let tinted = image.multiplied(Rgb(0xff_ffff00));
        assert_eq!((tinted.width, tinted.height), (16, 16));
        // Same coverage: a tint must not change what is drawn, only its color.
        for y in 0..16 {
            for x in 0..16 {
                assert_eq!(alpha(tinted.get(x, y)), alpha(image.get(x, y)));
            }
        }
        // Red multiplied by yellow keeps its red and loses its (already absent) blue.
        let middle = tinted.get(8, 8);
        assert_eq!((middle >> 16) & 0xff, 0xff, "red");
        assert_eq!(middle & 0xff, 0x00, "blue");
    }

    #[test]
    #[cfg(feature = "svg")]
    fn multiplying_by_white_changes_nothing() {
        // The hover state is `ICON_TINT_HOVER`, which is white -- so hovering must leave the
        // artwork exactly as drawn rather than shifting it a channel.
        let image = from_svg(BADGE, 48).expect("badge");
        assert_eq!(image.multiplied(Rgb(0xff_ffffff)), image);
    }

    #[test]
    #[cfg(feature = "svg")]
    fn multiplying_keeps_dark_parts_dark_and_brightens_light_ones() {
        // The whole reason for multiply over replace. Replacing would make the artwork one
        // flat yellow shape; multiplying turns its white body yellow while its outline and
        // shadow stay dark, which is what reads as a highlight rather than a blob.
        let image = from_svg(BADGE, 48).expect("badge");
        let tinted = image.multiplied(Rgb(0xff_ffff00));

        // These are *premultiplied*, so a channel is already scaled by its alpha -- the
        // shadow is 75%-opacity black, which is alpha 191 with a red of 0. Comparing
        // the channel to a flat threshold would call that "dark" only by accident, so the
        // brightness worth testing is the channel *relative to* its alpha.
        let brightness = |pixel: u32| {
            let alpha = (pixel >> 24) & 0xff;
            (alpha > 0).then(|| (((pixel >> 16) & 0xff) * 255) / alpha)
        };
        let mut checked_dark = false;
        let mut checked_light = false;

        for y in 0..48 {
            for x in 0..48 {
                let (before, after) = (image.get(x, y), tinted.get(x, y));
                let (Some(was), Some(now)) = (brightness(before), brightness(after)) else {
                    continue;
                };
                if was < 40 {
                    assert!(now < 40, "a dark pixel was brightened at {x},{y}");
                    checked_dark = true;
                }
                if was > 200 {
                    // Yellow keeps red and green and drops blue, so the body stays bright.
                    assert!(now > 200, "a light pixel was darkened at {x},{y}");
                    assert_eq!(after & 0xff, 0, "blue should be gone at {x},{y}");
                    checked_light = true;
                }
            }
        }
        assert!(checked_dark, "found no dark pixel to check (the shadow?)");
        assert!(checked_light, "found no light pixel to check (the body?)");
    }

    #[test]
    fn a_failed_load_is_remembered_rather_than_retried() {
        let mut images = Images::default();
        let missing = Path::new("/nonexistent/icon.svg");
        assert!(images.load(missing, 32).is_none());
        assert!(images.load(missing, 32).is_none());
        assert_eq!(images.cache.len(), 1, "should have cached the failure");
    }

    #[test]
    #[cfg(feature = "svg")]
    fn loading_twice_reuses_the_first_result() {
        let dir = std::env::temp_dir().join("wlrix-ui-image-cache-test");
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let file = dir.join("square.svg");
        std::fs::write(&file, RED_SQUARE).expect("scratch file");

        let mut images = Images::default();
        assert!(images.load(&file, 48).is_some());
        assert!(images.load(&file, 48).is_some());
        assert_eq!(images.cache.len(), 1);
        // A different size is a different entry, not a replacement: an SVG is rendered *at*
        // the size asked for, so a cache that ignored size would hand back a blurry icon.
        assert!(images.load(&file, 64).is_some());
        assert_eq!(images.cache.len(), 2);

        images.clear();
        assert_eq!(images.cache.len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(feature = "svg")]
    fn the_two_fits_are_separate_cache_entries() {
        // Same path, same numbers, different pixels: without the mode in the key, whichever
        // was asked for first would answer for both.
        let dir = std::env::temp_dir().join("wlrix-ui-image-fit-test");
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let file = dir.join("wide.svg");
        std::fs::write(&file, WIDE).expect("scratch file");

        let mut images = Images::default();
        let fitted = images.load(&file, 40).expect("fitted").clone();
        let covered = images.load_cover(&file, 40, 40).expect("covered").clone();
        assert_eq!(images.cache.len(), 2);
        assert_ne!(fitted, covered);
        // And it is the covered one that fills: the fitted one is empty at the top.
        assert_eq!(alpha(fitted.get(20, 2)), 0);
        assert_eq!(alpha(covered.get(20, 2)), 255);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
