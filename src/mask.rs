// SPDX-License-Identifier: GPL-3.0-or-later
//! A coverage mask: one shape, tinted at draw time.
//!
//! **An icon is drawn into a mask, not straight onto the canvas.** It is one shape whose
//! color is decided by its state -- knocked-back gray at rest, full white under the pointer,
//! IRIX's lamp yellow when selected -- so drawing the shape once as coverage and blending
//! the tint through it means the states cannot drift apart. It is also what makes real
//! artwork a drop-in later: supply a mask instead of a drawing routine and nothing above
//! changes.
//!
//! The shapes themselves are not here. What a folder or a launcher badge looks like is
//! `wlrix-desktop`'s business, and drawing one only needs [`Mask`]'s builders.

use crate::canvas::{Canvas, Rect};
use crate::color::Rgb;

/// A square, single-channel coverage bitmap: 0 is untouched, 255 fully the icon.
///
/// Small enough (64x64 by default) that a `Vec<u8>` per draw is not worth avoiding, and
/// keeping it owned means the shape routines can overdraw freely without clipping logic.
pub struct Mask {
    size: i32,
    coverage: Vec<u8>,
}

impl Mask {
    pub fn new(size: i32) -> Self {
        Self {
            size,
            coverage: vec![0; (size * size).max(0) as usize],
        }
    }

    pub fn set(&mut self, x: i32, y: i32, coverage: u8) {
        if x < 0 || y < 0 || x >= self.size || y >= self.size {
            return;
        }
        let index = (y * self.size + x) as usize;
        // Max, not overwrite: shapes are drawn over each other and the darker one must not
        // punch a hole in what is already there.
        self.coverage[index] = self.coverage[index].max(coverage);
    }

    pub fn get(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.size || y >= self.size {
            return 0;
        }
        self.coverage[(y * self.size + x) as usize]
    }

    /// Fill a rectangle solid.
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        for row in y..y + h {
            for column in x..x + w {
                self.set(column, row, 255);
            }
        }
    }

    /// Draw a one-pixel outline.
    pub fn outline(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        for column in x..x + w {
            self.set(column, y, 255);
            self.set(column, y + h - 1, 255);
        }
        for row in y..y + h {
            self.set(x, row, 255);
            self.set(x + w - 1, row, 255);
        }
    }

    /// Clear a rectangle back to nothing, for punching a window out of a filled shape.
    pub fn erase(&mut self, x: i32, y: i32, w: i32, h: i32) {
        for row in y..y + h {
            for column in x..x + w {
                if column < 0 || row < 0 || column >= self.size || row >= self.size {
                    continue;
                }
                self.coverage[(row * self.size + column) as usize] = 0;
            }
        }
    }
}

impl Mask {
    /// The mask's edge length in pixels.
    pub fn size(&self) -> i32 {
        self.size
    }

    /// Blend the mask over `rect` in `color`, scaling it to fit.
    ///
    /// Nearest-neighbor, deliberately: these are small emblems on a pixel grid and a
    /// smoothed one reads as blurry rather than as larger.
    pub fn draw(&self, canvas: &mut Canvas, rect: Rect, color: Rgb) {
        if self.size <= 0 || rect.w <= 0 || rect.h <= 0 {
            return;
        }
        for row in 0..rect.h {
            let source_y = row * self.size / rect.h;
            for column in 0..rect.w {
                let source_x = column * self.size / rect.w;
                let coverage = self.get(source_x, source_y);
                if coverage == 0 {
                    continue;
                }
                canvas.blend(rect.x + column, rect.y + row, color, coverage);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_takes_the_greater_coverage() {
        // Shapes are drawn over each other; the fainter one must not punch a hole in what is
        // already there, which is what a plain overwrite would do.
        let mut mask = Mask::new(4);
        mask.set(1, 1, 200);
        mask.set(1, 1, 40);
        assert_eq!(mask.get(1, 1), 200);
        mask.set(1, 1, 255);
        assert_eq!(mask.get(1, 1), 255);
    }

    #[test]
    fn erase_punches_through_regardless() {
        // Unlike `set`, this one has to win -- it is how a window is cut out of a filled
        // shape.
        let mut mask = Mask::new(4);
        mask.rect(0, 0, 4, 4);
        mask.erase(1, 1, 2, 2);
        assert_eq!(mask.get(1, 1), 0);
        assert_eq!(mask.get(0, 0), 255);
    }

    #[test]
    fn drawing_off_mask_is_clipped_not_a_panic() {
        let mut mask = Mask::new(4);
        mask.rect(-10, -10, 40, 40);
        mask.outline(-5, -5, 30, 30);
        mask.set(100, 100, 255);
        assert_eq!(mask.get(0, 0), 255);
        assert_eq!(mask.get(9, 9), 0);
    }

    #[test]
    fn an_outline_is_hollow() {
        let mut mask = Mask::new(6);
        mask.outline(0, 0, 6, 6);
        assert_eq!(mask.get(0, 0), 255);
        assert_eq!(mask.get(3, 0), 255);
        assert_eq!(mask.get(3, 3), 0, "the middle is not part of an outline");
    }

    #[test]
    fn drawing_scales_to_the_target_rect() {
        let mut mask = Mask::new(2);
        mask.rect(0, 0, 1, 2); // the left half
        let mut buf = vec![0u8; 4 * 4 * 4];
        let mut canvas = Canvas::new(&mut buf, 4, 4);
        mask.draw(&mut canvas, Rect::new(0, 0, 4, 4), Rgb(0xff_ffffff));
        assert_eq!(canvas.get(0, 0), Rgb(0xff_ffffff));
        assert_eq!(canvas.get(1, 3), Rgb(0xff_ffffff));
        assert_eq!(canvas.get(2, 0), Rgb(0), "the right half is untouched");
    }
}
