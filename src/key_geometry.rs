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
    matches!(row, 1..=5)
}

const ROW_1_BOUNDARIES: [f32; 15] = [
    0.0, 233.0, 379.0, 519.0, 661.0, 803.0, 946.0, 1085.0, 1222.0, 1360.0, 1497.0, 1631.0, 1769.0,
    1902.0, 2129.0,
];

const ROW_1_TEXT_BOUNDS: [SourceRect; 14] = [
    SourceRect {
        x: 19.0,
        y: 29.0,
        width: 177.0,
        height: 112.0,
    },
    SourceRect {
        x: 236.0,
        y: 31.0,
        width: 109.0,
        height: 107.0,
    },
    SourceRect {
        x: 382.0,
        y: 30.0,
        width: 106.0,
        height: 108.0,
    },
    SourceRect {
        x: 522.0,
        y: 30.0,
        width: 109.0,
        height: 106.0,
    },
    SourceRect {
        x: 664.0,
        y: 30.0,
        width: 108.0,
        height: 107.0,
    },
    SourceRect {
        x: 809.0,
        y: 31.0,
        width: 109.0,
        height: 108.0,
    },
    SourceRect {
        x: 950.0,
        y: 32.0,
        width: 109.0,
        height: 108.0,
    },
    SourceRect {
        x: 1090.0,
        y: 30.0,
        width: 104.0,
        height: 108.0,
    },
    SourceRect {
        x: 1231.0,
        y: 38.0,
        width: 105.0,
        height: 102.0,
    },
    SourceRect {
        x: 1373.0,
        y: 37.0,
        width: 104.0,
        height: 103.0,
    },
    SourceRect {
        x: 1509.0,
        y: 37.0,
        width: 105.0,
        height: 104.0,
    },
    SourceRect {
        x: 1652.0,
        y: 45.0,
        width: 101.0,
        height: 96.0,
    },
    SourceRect {
        x: 1789.0,
        y: 36.0,
        width: 102.0,
        height: 101.0,
    },
    SourceRect {
        x: 1922.0,
        y: 38.0,
        width: 177.0,
        height: 102.0,
    },
];

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

// The divider at 1971 is the darkest separator valley between the final two
// keys. The current fourth-row annotation has yellow label boxes for both keys
// but is missing that one red divider.
const ROW_4_BOUNDARIES: [f32; 16] = [
    0.0, 164.0, 306.0, 447.0, 589.0, 732.0, 876.0, 1014.0, 1153.0, 1293.0, 1431.0, 1565.0, 1699.0,
    1835.0, 1971.0, 2128.0,
];

const ROW_4_TEXT_BOUNDS: [SourceRect; 15] = [
    SourceRect {
        x: 18.0,
        y: 38.0,
        width: 107.0,
        height: 110.0,
    },
    SourceRect {
        x: 168.0,
        y: 38.0,
        width: 108.0,
        height: 111.0,
    },
    SourceRect {
        x: 306.0,
        y: 40.0,
        width: 104.0,
        height: 104.0,
    },
    SourceRect {
        x: 452.0,
        y: 41.0,
        width: 108.0,
        height: 104.0,
    },
    SourceRect {
        x: 595.0,
        y: 39.0,
        width: 105.0,
        height: 105.0,
    },
    SourceRect {
        x: 734.0,
        y: 41.0,
        width: 106.0,
        height: 106.0,
    },
    SourceRect {
        x: 882.0,
        y: 43.0,
        width: 106.0,
        height: 102.0,
    },
    SourceRect {
        x: 1023.0,
        y: 41.0,
        width: 107.0,
        height: 103.0,
    },
    SourceRect {
        x: 1164.0,
        y: 40.0,
        width: 107.0,
        height: 103.0,
    },
    SourceRect {
        x: 1302.0,
        y: 44.0,
        width: 107.0,
        height: 103.0,
    },
    SourceRect {
        x: 1440.0,
        y: 40.0,
        width: 107.0,
        height: 103.0,
    },
    SourceRect {
        x: 1579.0,
        y: 42.0,
        width: 107.0,
        height: 103.0,
    },
    SourceRect {
        x: 1720.0,
        y: 41.0,
        width: 103.0,
        height: 103.0,
    },
    SourceRect {
        x: 1859.0,
        y: 37.0,
        width: 106.0,
        height: 106.0,
    },
    SourceRect {
        x: 1997.0,
        y: 40.0,
        width: 103.0,
        height: 103.0,
    },
];

// Bottom-row boundaries include the two intentional empty intervals at
// indices 1 and 12.
const ROW_5_BOUNDARIES: [f32; 15] = [
    0.0, 250.0, 369.0, 521.0, 664.0, 806.0, 949.0, 1086.0, 1225.0, 1361.0, 1497.0, 1630.0, 1766.0,
    1904.0, 2131.0,
];

const ROW_5_TEXT_BOUNDS: [Option<SourceRect>; 14] = [
    Some(SourceRect {
        x: 20.0,
        y: 37.0,
        width: 179.0,
        height: 111.0,
    }),
    None,
    Some(SourceRect {
        x: 381.0,
        y: 43.0,
        width: 107.0,
        height: 102.0,
    }),
    Some(SourceRect {
        x: 524.0,
        y: 43.0,
        width: 107.0,
        height: 103.0,
    }),
    Some(SourceRect {
        x: 668.0,
        y: 43.0,
        width: 108.0,
        height: 103.0,
    }),
    Some(SourceRect {
        x: 811.0,
        y: 43.0,
        width: 108.0,
        height: 103.0,
    }),
    Some(SourceRect {
        x: 952.0,
        y: 43.0,
        width: 108.0,
        height: 103.0,
    }),
    Some(SourceRect {
        x: 1098.0,
        y: 43.0,
        width: 108.0,
        height: 103.0,
    }),
    Some(SourceRect {
        x: 1234.0,
        y: 43.0,
        width: 109.0,
        height: 104.0,
    }),
    Some(SourceRect {
        x: 1373.0,
        y: 49.0,
        width: 102.0,
        height: 98.0,
    }),
    Some(SourceRect {
        x: 1507.0,
        y: 45.0,
        width: 103.0,
        height: 99.0,
    }),
    Some(SourceRect {
        x: 1645.0,
        y: 48.0,
        width: 103.0,
        height: 99.0,
    }),
    None,
    Some(SourceRect {
        x: 1927.0,
        y: 43.0,
        width: 174.0,
        height: 99.0,
    }),
];

fn annotated_key_index(row: usize, col: f32) -> Option<usize> {
    if !has_annotated_boundaries(row) {
        return None;
    }
    match row {
        1 | 3 if col == 0.0 => Some(0),
        1 | 3 => Some((col - 0.5).round() as usize),
        2 | 4 => Some(col.round() as usize),
        5 if col == 0.0 => Some(0),
        5 => Some((col - 0.5).round() as usize),
        _ => unreachable!(),
    }
}

/// Returns the source-pixel crop for an alpha key. Annotated rows use their
/// measured separators; remaining rows retain the documented 15-unit grid.
pub fn alpha_key_source_rect(row: usize, col: f32, width: f32) -> SourceRect {
    let (source_width, source_height) = ALPHA_SOURCE_SIZES[row - 1];
    let annotated = annotated_key_index(row, col).and_then(|index| match row {
        1 => ROW_1_BOUNDARIES
            .get(index..=index + 1)
            .map(|pair| (pair[0], pair[1])),
        2 => ROW_2_BOUNDARIES
            .get(index..=index + 1)
            .map(|pair| (pair[0], pair[1])),
        3 => ROW_3_BOUNDARIES
            .get(index..=index + 1)
            .map(|pair| (pair[0], pair[1])),
        4 => ROW_4_BOUNDARIES
            .get(index..=index + 1)
            .map(|pair| (pair[0], pair[1])),
        5 => ROW_5_BOUNDARIES
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
        1 => ROW_1_TEXT_BOUNDS.get(index).copied().map(TextGuide::Bounds),
        2 => ROW_2_TEXT_CENTERS
            .get(index)
            .map(|&(x, y)| TextGuide::Center { x, y }),
        3 => ROW_3_TEXT_BOUNDS.get(index).copied().map(TextGuide::Bounds),
        4 => ROW_4_TEXT_BOUNDS.get(index).copied().map(TextGuide::Bounds),
        5 => ROW_5_TEXT_BOUNDS
            .get(index)
            .copied()
            .flatten()
            .map(TextGuide::Bounds),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotated_boundaries_cover_every_alpha_row_without_gaps() {
        for (row, boundaries) in [
            (1, ROW_1_BOUNDARIES.as_slice()),
            (2, ROW_2_BOUNDARIES.as_slice()),
            (3, ROW_3_BOUNDARIES.as_slice()),
            (4, ROW_4_BOUNDARIES.as_slice()),
            (5, ROW_5_BOUNDARIES.as_slice()),
        ] {
            assert_eq!(boundaries.first(), Some(&0.0));
            assert_eq!(boundaries.last(), Some(&ALPHA_SOURCE_SIZES[row - 1].0));
            assert!(boundaries.windows(2).all(|pair| pair[0] < pair[1]));
        }
    }

    #[test]
    fn every_trial_text_guide_stays_inside_its_key() {
        let assert_guide = |row, col, width| {
            let key = alpha_key_source_rect(row, col, width);
            match alpha_text_guide(row, col).expect("annotated key must have a text guide") {
                TextGuide::Center { x, y } => {
                    assert!(x >= key.x && x <= key.x + key.width);
                    assert!(y >= key.y && y <= key.y + key.height);
                }
                TextGuide::Bounds(bounds) => {
                    assert!(bounds.x >= key.x);
                    assert!(bounds.x + bounds.width <= key.x + key.width);
                    assert!(bounds.y >= key.y);
                    assert!(bounds.y + bounds.height <= key.y + key.height);
                }
            }
        };

        for row in [1, 3] {
            for index in 0..14 {
                let col = if index == 0 { 0.0 } else { index as f32 + 0.5 };
                let width = if index == 0 || index == 13 { 1.5 } else { 1.0 };
                assert_guide(row, col, width);
            }
        }
        for row in [2, 4] {
            for column in 0..15 {
                assert_guide(row, column as f32, 1.0);
            }
        }
        assert_guide(5, 0.0, 1.5);
        for column in 2..=11 {
            assert_guide(5, column as f32 + 0.5, 1.0);
        }
        assert_guide(5, 13.5, 1.5);
    }
}
