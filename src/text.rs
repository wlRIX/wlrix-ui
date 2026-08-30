// SPDX-License-Identifier: GPL-3.0-or-later
//! Text, rasterized with cosmic-text.
//!
//! IRIX's `FontPalette` asks for Helvetica, which is proprietary. Rather than name a free
//! stand-in and hope it is installed, wlRIX asks cosmic-text for a sans-serif face and lets
//! it resolve a concrete family from the system font database. The reason for cosmic-text
//! over a single loaded face is **fallback and shaping**: a name in Japanese (or any script
//! the primary family lacks) renders through the system's CJK font instead of as
//! missing-glyph boxes, and shaping gives correct kerning and complex-script layout.
//!
//! Two sinks, one shaping path. [`Fonts::draw`] blends a run straight into a [`Canvas`],
//! which is what a `wl_shm` client wants; [`Fonts::rasterize`] hands back a standalone
//! coverage [`Raster`] for a caller that has to build a texture instead, which is what the
//! compositor wants. Before this crate the compositor had a whole second cosmic-text wrapper
//! for that difference -- and, because it was a separate wrapper, no
//! [`ensure_system_dirs`].

use std::collections::HashMap;

use cosmic_text::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight,
};

use crate::canvas::{Canvas, Rect};
use crate::color::Rgb;

/// Line height as a multiple of the font size -- typical sans leading, used when a face's own
/// metrics are unavailable.
const LINE_FACTOR: f32 = 1.3;

/// The three faces the IRIX chrome uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    /// Field contents, names, buttons, icon labels.
    Regular,
    /// Labels, and menu and window-title text.
    Bold,
    /// The full name beside the greeter's field, and the `wlRIX` wordmark.
    Italic,
}

/// A text run's fixed parameters: which face and size, where, and in what color.
///
/// Grouped so [`Fonts::draw`] takes one description of the run rather than a long list
/// of positional arguments.
#[derive(Clone, Copy)]
pub struct Run {
    pub face: Face,
    pub px: f32,
    /// The run's left edge.
    pub x: i32,
    /// The baseline the glyphs sit on.
    pub baseline: i32,
    pub color: Rgb,
}

/// A face's vertical metrics at one size: the top-to-baseline distance and the line box.
#[derive(Clone, Copy)]
struct LineMetrics {
    ascent: i32,
    line_height: i32,
}

/// A loaded font stack. Shaping and rasterization go through cosmic-text; vertical metrics
/// are cached per (face, size) so per-frame layout code can ask for them freely.
pub struct Fonts {
    system: FontSystem,
    swash: SwashCache,
    /// Keyed by face and pixel size (as its bit pattern, so it can be hashed).
    metrics: HashMap<(Face, u32), LineMetrics>,
}

impl Fonts {
    /// Ready cosmic-text over the system font database. Errs, rather than drawing nothing, if
    /// the system has no fonts at all -- a screen that explains why beats a blank one.
    pub fn load() -> Result<Self, String> {
        let mut system = FontSystem::new();
        ensure_system_dirs(system.db_mut());
        if system.db().is_empty() {
            return Err("no fonts found: cosmic-text's system font database is empty".to_string());
        }
        Ok(Self {
            system,
            swash: SwashCache::new(),
            metrics: HashMap::new(),
        })
    }

    /// What is being drawn with, for a one-line note at startup.
    pub fn family(&self) -> &str {
        "system sans-serif (cosmic-text)"
    }

    /// How many faces cosmic-text found, for the startup log -- a small number means the
    /// process is not seeing the system font directories.
    pub fn face_count(&self) -> usize {
        self.system.db().len()
    }

    /// Whether `sample` shapes to real glyphs rather than `.notdef` tofu -- i.e. whether the
    /// font environment can render it. Logged at startup for a script (CJK) the base sans
    /// lacks, so a screen full of boxes is diagnosable from the log alone.
    pub fn can_render(&mut self, sample: &str) -> bool {
        let buffer = self.shape(Face::Regular, 16.0, sample);
        buffer
            .layout_runs()
            .flat_map(|run| run.glyphs)
            .all(|glyph| glyph.glyph_id != 0)
    }

    /// The distance from the top of a line to the baseline, so callers can turn a box
    /// top into a baseline for [`draw`].
    pub fn ascent(&mut self, face: Face, px: f32) -> i32 {
        self.line_metrics(face, px).ascent
    }

    /// A line's natural height, for spacing rows.
    pub fn line_height(&mut self, face: Face, px: f32) -> i32 {
        self.line_metrics(face, px).line_height
    }

    /// How wide `text` is at this face and size, in whole pixels.
    pub fn width(&mut self, face: Face, px: f32, text: &str) -> i32 {
        if text.is_empty() {
            return 0;
        }
        let buffer = self.shape(face, px, text);
        buffer
            .layout_runs()
            .next()
            .map(|run| run.line_w.ceil() as i32)
            .unwrap_or(0)
    }

    /// Draw `text` with the given face, size, position and color. The run is shaped, then each
    /// covered pixel is blended onto the canvas at the run's color -- cosmic-text hands back
    /// coverage in the alpha channel, which is exactly what [`Canvas::blend`] wants.
    pub fn draw(&mut self, canvas: &mut Canvas, run: Run, text: &str) {
        if text.is_empty() {
            return;
        }
        let buffer = self.shape(run.face, run.px, text);
        // Place the buffer so its baseline lands on `run.baseline`. cosmic-text draws from the
        // buffer top; the first line's baseline sits `line_y` below it.
        let baseline = buffer
            .layout_runs()
            .next()
            .map(|line| line.line_y)
            .unwrap_or(run.px);
        let dy = run.baseline - baseline.round() as i32;
        let (r, g, b) = run.color.channels();
        let color = run.color;

        buffer.draw(
            &mut self.system,
            &mut self.swash,
            Color::rgba(r, g, b, 255),
            |x, y, w, h, pixel| {
                let coverage = pixel.a();
                if coverage == 0 {
                    return;
                }
                for row in 0..h as i32 {
                    for col in 0..w as i32 {
                        canvas.blend(run.x + x + col, dy + y + row, color, coverage);
                    }
                }
            },
        );
    }

    /// The baseline that centers one line of `face`/`px` text vertically in `rect`.
    ///
    /// There were three of these, and no two agreed: the greeter's button labels, the
    /// desktop's menu rows and the compositor's window titles each centered text with their
    /// own arithmetic and their own fudge constant. Rows of the same nominal height in two
    /// components did not line up.
    pub fn centered_baseline(&mut self, face: Face, px: f32, rect: Rect) -> i32 {
        let m = self.line_metrics(face, px);
        rect.y + (rect.h - m.line_height) / 2 + m.ascent
    }

    /// Shape `text` into a standalone 8-bit coverage bitmap.
    ///
    /// For a caller that cannot blend into a [`Canvas`] because it is building a texture --
    /// the compositor rasterizes a window title once and uploads it. `None` for text that
    /// shapes to nothing, which is a caller's cue to draw no title at all rather than upload
    /// a zero-sized buffer.
    ///
    /// Coverage, not color: the caller owns the colorization, because the compositor needs
    /// its result premultiplied and a client does not. Keeping the color out is also what
    /// lets a caller cache one raster per string instead of one per string *and* color --
    /// which matters the moment the palette can change while running.
    pub fn rasterize(&mut self, face: Face, px: f32, text: &str) -> Option<Raster> {
        if text.is_empty() {
            return None;
        }
        let buffer = self.shape(face, px, text);
        let width = buffer
            .layout_runs()
            .next()
            .map(|run| run.line_w.ceil() as i32)?;
        let height = self.line_metrics(face, px).line_height;
        if width <= 0 || height <= 0 {
            return None;
        }

        let mut raster = Raster {
            width,
            height,
            coverage: vec![0u8; (width * height) as usize],
        };
        // White at full alpha: `draw` hands the closure the glyph color multiplied by
        // coverage, so an opaque white source makes the alpha channel the coverage itself.
        buffer.draw(
            &mut self.system,
            &mut self.swash,
            Color::rgba(255, 255, 255, 255),
            |x, y, w, h, pixel| {
                let coverage = pixel.a();
                if coverage == 0 {
                    return;
                }
                for row in 0..h as i32 {
                    for col in 0..w as i32 {
                        raster.max(x + col, y + row, coverage);
                    }
                }
            },
        );
        Some(raster)
    }

    /// Vertical metrics for a face at a size, shaped once from a reference string and cached.
    fn line_metrics(&mut self, face: Face, px: f32) -> LineMetrics {
        let key = (face, px.to_bits());
        if let Some(metrics) = self.metrics.get(&key) {
            return *metrics;
        }
        // A reference with both an ascender and a descender, so the line box is realistic.
        let buffer = self.shape(face, px, "Ag");
        let metrics = buffer
            .layout_runs()
            .next()
            .map(|run| LineMetrics {
                ascent: (run.line_y - run.line_top).ceil() as i32,
                line_height: run.line_height.ceil() as i32,
            })
            .unwrap_or(LineMetrics {
                ascent: px.ceil() as i32,
                line_height: (px * LINE_FACTOR).ceil() as i32,
            });
        self.metrics.insert(key, metrics);
        metrics
    }

    /// Shape one line of `text` into an unwrapped buffer.
    ///
    /// cosmic-text keeps the weight/slant when it falls back for an uncovered glyph, so a CJK
    /// name in the italic full-name face matches a *Latin* italic font (which has no CJK) and
    /// comes out as `.notdef` tofu -- even though an upright CJK face covers it. When a styled
    /// shaping leaves any glyph missing, reshape upright in the regular face, which has the
    /// widest coverage: a readable upright name beats an italic row of boxes. Latin names shape
    /// cleanly in italic and keep it.
    fn shape(&mut self, face: Face, px: f32, text: &str) -> Buffer {
        let buffer = self.shape_with(attrs(face), px, text);
        if face != Face::Regular && has_missing_glyphs(&buffer) {
            return self.shape_with(attrs(Face::Regular), px, text);
        }
        buffer
    }

    /// Shape `text` with explicit attributes into an unwrapped buffer.
    fn shape_with(&mut self, attrs: Attrs, px: f32, text: &str) -> Buffer {
        let metrics = Metrics::new(px, (px * LINE_FACTOR).ceil());
        let mut buffer = Buffer::new(&mut self.system, metrics);
        buffer.set_text(&mut self.system, text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.system, false);
        buffer
    }
}

/// Whether shaping left any glyph as `.notdef` -- i.e. no font in the fallback chain covered it.
fn has_missing_glyphs(buffer: &Buffer) -> bool {
    buffer
        .layout_runs()
        .flat_map(|run| run.glyphs)
        .any(|glyph| glyph.glyph_id == 0)
}

/// Make sure the standard system font directories are in the database.
///
/// cosmic-text's default system scan is fontconfig-driven, and fontconfig resolves its font
/// directories from the environment -- `HOME`, XDG config, `/etc/fonts`. `wlrix-greeter` runs
/// before login in a minimal greetd session where that resolution can come up short and drop
/// directories, most visibly the system CJK font, so a non-Latin full name renders as tofu even
/// though the font is installed. Loading the well-known directories directly makes a component
/// independent of that. Directories fontconfig already covered are skipped, so faces are not
/// loaded twice.
///
/// Public because it is not only the greeter's problem: the compositor's own cosmic-text
/// wrapper went without this, and would have rendered the same tofu in a window title the day
/// anything ran it in a stripped environment.
pub fn ensure_system_dirs(db: &mut cosmic_text::fontdb::Database) {
    use cosmic_text::fontdb::Source;

    const DIRS: &[&str] = &["/usr/share/fonts", "/usr/local/share/fonts"];
    for dir in DIRS {
        let path = std::path::Path::new(dir);
        if !path.is_dir() {
            continue;
        }
        let already = db.faces().any(|face| match &face.source {
            Source::File(file) => file.starts_with(path),
            _ => false,
        });
        if !already {
            db.load_fonts_dir(path);
        }
    }
}

/// The cosmic-text attributes for a face: a sans-serif family, plus the weight or slant. The
/// concrete family (and any per-glyph fallback) is left to cosmic-text's font matching.
fn attrs(face: Face) -> Attrs<'static> {
    let base = Attrs::new().family(Family::SansSerif);
    match face {
        Face::Regular => base,
        Face::Bold => base.weight(Weight::BOLD),
        Face::Italic => base.style(Style::Italic),
    }
}

/// A shaped run as 8-bit coverage, with no color of its own.
///
/// Row-major, `width * height` bytes, origin top-left, baseline placed as [`Fonts::draw`]
/// would place it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: i32,
    pub height: i32,
    pub coverage: Vec<u8>,
}

impl Raster {
    /// Coverage at a point; 0 outside.
    pub fn get(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return 0;
        }
        self.coverage[(y * self.width + x) as usize]
    }

    /// Take the greater of what is there and `coverage`.
    ///
    /// `max` rather than overwrite, because overlapping glyph boxes are ordinary -- an
    /// accent over a letter, or a script that stacks -- and letting the second write win
    /// would punch the first one's antialiasing back out.
    fn max(&mut self, x: i32, y: i32, coverage: u8) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let cell = &mut self.coverage[(y * self.width + x) as usize];
        *cell = (*cell).max(coverage);
    }

    /// Blend the raster onto a canvas at `color`, top-left at `(x, y)`.
    pub fn draw(&self, canvas: &mut Canvas, x: i32, y: i32, color: Rgb) {
        for row in 0..self.height {
            for col in 0..self.width {
                canvas.blend(x + col, y + row, color, self.get(col, row));
            }
        }
    }

    /// The raster as premultiplied `Argb8888`, one `u32` per pixel, at `color`.
    ///
    /// What a caller uploading a texture wants. Premultiplied because that is what both
    /// `wl_shm`'s `Argb8888` and the compositor's renderer expect, and because the
    /// alternative -- straight alpha, converted later -- is the bug that shows up as a dark
    /// halo around every glyph on a light title bar.
    pub fn premultiplied(&self, color: Rgb) -> Vec<u32> {
        let (r, g, b) = color.channels();
        self.coverage
            .iter()
            .map(|&a| {
                let mul = |c: u8| (u32::from(c) * u32::from(a) + 127) / 255;
                (u32::from(a) << 24) | (mul(r) << 16) | (mul(g) << 8) | mul(b)
            })
            .collect()
    }
}

/// Break a label into at most `max_lines` lines that fit `width`.
///
/// Prefers to break between words, so a launcher's `Name` does not read as "mpv Media Play /
/// er" -- but **only when that does not cost the end of the name**. Tidy words are worth less
/// than the whole label: "mpv メディアプレイヤー" breaks after "mpv" if it is allowed to, which
/// leaves a nearly empty first line and a second too wide to fit, so it falls back to breaking
/// mid-run and shows everything. Filenames go the same way, having no word boundary to find.
///
/// A name too long for the lines available either way is cut and given an ellipsis, since a
/// label that overflowed its cell would collide with its neighbor.
pub fn wrap(
    fonts: &mut Fonts,
    face: Face,
    px: f32,
    name: &str,
    width: i32,
    max_lines: usize,
) -> Vec<String> {
    if width <= 0 || max_lines == 0 {
        return Vec::new();
    }
    if fonts.width(face, px, name) <= width {
        return vec![name.to_owned()];
    }

    let (words, words_cut) = break_lines(fonts, face, px, name, width, max_lines, true);
    if !words_cut {
        return words;
    }
    // Breaking at spaces lost the end of the name. Try again without them, and take whichever
    // shows more -- if both are cut, the tidier one wins.
    let (characters, characters_cut) = break_lines(fonts, face, px, name, width, max_lines, false);
    if characters_cut { words } else { characters }
}

/// One wrapping pass. Returns the lines, and whether text had to be dropped to fit them.
#[allow(clippy::too_many_arguments)]
fn break_lines(
    fonts: &mut Fonts,
    face: Face,
    px: f32,
    name: &str,
    width: i32,
    max_lines: usize,
    at_spaces: bool,
) -> (Vec<String>, bool) {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    // By `char`, so a multi-byte character is never split down the middle.
    for character in name.chars() {
        let mut candidate = line.clone();
        candidate.push(character);
        if fonts.width(face, px, &candidate) > width && !line.is_empty() {
            // A space at the very start is no use to break at -- it would push an empty line
            // and lose a whole row to nothing.
            let at = at_spaces
                .then(|| line.rfind(' '))
                .flatten()
                .filter(|at| *at > 0);
            let (head, rest) = match at {
                // The space itself is dropped: it did its job by being the break.
                Some(at) => (line[..at].to_owned(), line[at + 1..].to_owned()),
                None => (std::mem::take(&mut line), String::new()),
            };
            lines.push(head);
            // Whatever was after the space starts the next line rather than being lost.
            line = rest;
            if lines.len() == max_lines {
                break;
            }
        }
        line.push(character);
    }

    if lines.len() < max_lines && !line.is_empty() {
        lines.push(line);
        return (lines, false);
    }

    // Ran out of lines with text still to place: mark the last one as cut.
    if let Some(last) = lines.last_mut() {
        while !last.is_empty() && fonts.width(face, px, &format!("{last}…")) > width {
            last.pop();
        }
        last.push('…');
    }
    (lines, true)
}
