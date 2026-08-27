//! Pixel-space calibration extracted from annotated copies of the photographic
//! alpha-row sources. This module is also included by `build.rs`, keeping sprite
//! extraction and runtime placement on the same geometry.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextGuide {
    /// A purple marker identifies the preferred center of the label.
    Center { x: f32, y: f32 },
    /// A yellow rectangle is a hard boundary that rendered text may not cross.
    Bounds(SourceRect),
}

pub const ALPHA_SOURCE_SIZES: [(f32, f32); 5] = [
    (2129.0, 184.0),
    (2119.0, 180.0),
    (2114.0, 184.0),
    (2128.0, 184.0),
    (2131.0, 177.0),
];

pub const fn has_annotated_boundaries(row: usize) -> bool {
    matches!(row, 2 | 3)
}

// Red separator centers from `2nd row copy.png`. The source has fifteen keys.
const ROW_2_BOUNDARIES: [f32; 16] = [
    0.0, 153.0, 298.0, 438.0, 584.0, 722.0, 866.0, 1005.0, 1141.0, 1283.0, 1422.0, 1557.0, 1692.0,
    1827.0, 1963.0, 2119.0,
];

// Centers of the solid purple marker interiors from `2nd row copy.png`.
const ROW_2_TEXT_CENTERS: [(f32, f32); 15] = [
    (62.5, 94.0),
    (209.7, 97.2),
    (355.9, 97.4),
    (497.4, 97.2),
    (640.1, 97.6),
    (785.3, 97.8),
    (926.5, 98.0),
    (1061.8, 94.0),
    (1200.9, 94.1),
    (1344.5, 94.3),
    (1482.2, 93.2),
    (1619.4, 95.6),
    (1767.5, 97.7),
    (1904.8, 98.0),
    (2036.9, 98.1),
];

// Red separator centers from `3rd row copy.png`. This source has fourteen
// keys: wide caps at each end with twelve standard caps between them.
const ROW_3_BOUNDARIES: [f32; 15] = [
    0.0, 226.0, 366.0, 509.0, 654.0, 789.0, 929.0, 1075.0, 1207.0, 1346.0, 1482.0, 1619.0, 1753.0,
    1888.0, 2114.0,
];

// Outer pixel extents of the yellow text-boundary marks in
// `3rd row copy.png`. Runtime rendering fits labels within these rectangles.
const ROW_3_TEXT_BOUNDS: [SourceRect; 14] = [
    SourceRect {
        x: 13.0,
        y: 38.0,
        width: 174.0,
        height: 107.0,
    },
    SourceRect {
        x: 228.0,
        y: 38.0,
        width: 111.0,
        height: 108.0,
    },
    SourceRect {
        x: 368.0,
        y: 41.0,
        width: 108.0,
        height: 100.0,
    },
    SourceRect {
        x: 510.0,
        y: 36.0,
        width: 144.0,
        height: 106.0,
    },
    SourceRect {
        x: 654.0,
        y: 40.0,
        width: 135.0,
        height: 107.0,
    },
    SourceRect {
        x: 789.0,
        y: 35.0,
        width: 116.0,
        height: 107.0,
    },
    SourceRect {
        x: 940.0,
        y: 40.0,
        width: 108.0,
        height: 100.0,
    },
    SourceRect {
        x: 1083.0,
        y: 41.0,
        width: 109.0,
        height: 101.0,
    },
    SourceRect {
        x: 1212.0,
        y: 43.0,
        width: 109.0,
        height: 101.0,
    },
    SourceRect {
        x: 1354.0,
        y: 43.0,
        width: 109.0,
        height: 101.0,
    },
    SourceRect {
        x: 1495.0,
        y: 43.0,
        width: 104.0,
        height: 101.0,
    },
    SourceRect {
        x: 1639.0,
        y: 43.0,
        width: 95.0,
        height: 102.0,
    },
    SourceRect {
        x: 1775.0,
        y: 43.0,
        width: 103.0,
        height: 103.0,
    },
    SourceRect {
        x: 1911.0,
        y: 40.0,
        width: 176.0,
        height: 104.0,
    },
];

fn annotated_key_index(row: usize, col: f32) -> Option<usize> {
    if !has_annotated_boundaries(row) {
        return None;
    }
    match row {
        2 => Some(col.round() as usize),
        3 if col == 0.0 => Some(0),
        3 => Some((col - 0.5).round() as usize),
        _ => unreachable!(),
    }
}

/// Returns the source-pixel crop for an alpha key. Annotated rows use their
/// measured separators; remaining rows retain the documented 15-unit grid.
pub fn alpha_key_source_rect(row: usize, col: f32, width: f32) -> SourceRect {
    let (source_width, source_height) = ALPHA_SOURCE_SIZES[row - 1];
    let annotated = annotated_key_index(row, col).and_then(|index| match row {
        2 => ROW_2_BOUNDARIES
            .get(index..=index + 1)
            .map(|pair| (pair[0], pair[1])),
        3 => ROW_3_BOUNDARIES
            .get(index..=index + 1)
            .map(|pair| (pair[0], pair[1])),
        _ => None,
    });
    let (x0, x1) = annotated.unwrap_or_else(|| {
        (
            (col / 15.0 * source_width).round(),
            ((col + width) / 15.0 * source_width).round(),
        )
    });
    SourceRect {
        x: x0,
        y: 0.0,
        width: x1 - x0,
        height: source_height,
    }
}

pub fn alpha_text_guide(row: usize, col: f32) -> Option<TextGuide> {
    let index = annotated_key_index(row, col)?;
    match row {
        2 => ROW_2_TEXT_CENTERS
            .get(index)
            .map(|&(x, y)| TextGuide::Center { x, y }),
        3 => ROW_3_TEXT_BOUNDS.get(index).copied().map(TextGuide::Bounds),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotated_boundaries_cover_both_rows_without_gaps() {
        assert_eq!(ROW_2_BOUNDARIES.first(), Some(&0.0));
        assert_eq!(ROW_2_BOUNDARIES.last(), Some(&ALPHA_SOURCE_SIZES[1].0));
        assert_eq!(ROW_3_BOUNDARIES.first(), Some(&0.0));
        assert_eq!(ROW_3_BOUNDARIES.last(), Some(&ALPHA_SOURCE_SIZES[2].0));
        assert!(ROW_2_BOUNDARIES.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(ROW_3_BOUNDARIES.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn every_trial_text_guide_stays_inside_its_key() {
        for column in 0..15 {
            let key = alpha_key_source_rect(2, column as f32, 1.0);
            let Some(TextGuide::Center { x, y }) = alpha_text_guide(2, column as f32) else {
                panic!("row 2 key must have a center guide");
            };
            assert!(x >= key.x && x <= key.x + key.width);
            assert!(y >= key.y && y <= key.y + key.height);
        }

        for index in 0..14 {
            let col = if index == 0 { 0.0 } else { index as f32 + 0.5 };
            let width = if index == 0 || index == 13 { 1.5 } else { 1.0 };
            let key = alpha_key_source_rect(3, col, width);
            let Some(TextGuide::Bounds(bounds)) = alpha_text_guide(3, col) else {
                panic!("row 3 key must have a bounds guide");
            };
            assert!(bounds.x >= key.x);
            assert!(bounds.x + bounds.width <= key.x + key.width);
            assert!(bounds.y >= key.y);
            assert!(bounds.y + bounds.height <= key.y + key.height);
        }
    }
}
