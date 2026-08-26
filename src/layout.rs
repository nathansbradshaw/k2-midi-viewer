use crate::key::{Cluster, Key, KeyId};
use std::collections::{HashMap, HashSet};

/// Horizontal anchors for the keyboard's three major sections. Their spacing
/// is one compact half-unit gap, matching the arrow-to-numpad separation.
pub const NAV_COL: f32 = 15.5;
pub const NUMPAD_COL: f32 = 19.0;

/// Column pitch and width for the 12 encoder knobs. Chosen so the whole
/// encoder row still lands inside the same horizontal footprint the original
/// 8 full-width knobs occupied — NAV_COL is calibrated to the alpha block's
/// real PCB spacing (see `right_hand_sections_share_alignment_and_spacing`)
/// and must not shift to make room here.
pub const KNOB_COL_STEP: f32 = 2.0 / 3.0;
pub const KNOB_WIDTH: f32 = 0.5;

pub struct Layout {
    pub keys: Vec<Key>,
    /// All KeyIds for each note, ordered top-row first (Row 1 → Row 5).
    /// Multiple entries exist where rows overlap (same note appears on two rows).
    pub note_to_all_keys: HashMap<u8, Vec<KeyId>>,
    /// Set of all raw firmware MIDI notes present on this keyboard.
    pub keyboard_notes: HashSet<u8>,
    /// GM percussion note → drum pad key. Separate from `note_to_all_keys`
    /// because the firmware dedicates the Numpad cluster to fixed drum-channel
    /// notes (see keyboard-keyboard/code/src/constants.rs DRUM_NOTE) — a hit
    /// there isn't octave-shifted or looked up like a melodic key.
    pub drum_note_to_key: HashMap<u8, KeyId>,
}

/// GM percussion notes assigned to the Numpad cluster, one per key, in the
/// same order the keys are declared below.
///
/// Confirmed from keyboard-keyboard/code: `constants.rs` DRUM_NOTE assigns
/// notes 36 (Bass Drum) … 55 (Splash Cymbal) to switches HE81–HE100 in order,
/// and the KiCad PCB places those 20 switches on a uniform 4-column × 5-row
/// grid (checked via footprint coordinates — columns at x≈340/359/378/397,
/// rows at y≈-141/-121/-100/-80/-59, i.e. exactly row-major reading order).
/// This cluster mirrors that same 4×5 grid, so DRUM_NOTE's linear order maps
/// directly onto construction order below, top-to-bottom left-to-right.
pub const DRUM_PAD_NOTES: [u8; 20] = [
    36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55,
];

// Sharp notes (C#, D#, F#, G#, A#) → dark alpha; naturals → light alpha.
fn note_cluster(midi: u8) -> Cluster {
    match midi % 12 {
        1 | 3 | 6 | 8 | 10 => Cluster::Alpha,
        _ => Cluster::AlphaLight,
    }
}

fn note_name(midi: u8) -> &'static str {
    match midi % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        11 => "B",
        _ => unreachable!(),
    }
}

pub fn build_layout() -> Layout {
    let mut id = 0u32;
    let mut next_id = || {
        id += 1;
        id
    };
    let mut keys: Vec<Key> = Vec::new();

    // --- Encoders ---
    for i in 0..crate::synth::KNOB_COUNT {
        keys.push(
            Key::new(next_id(), "", i as f32 * KNOB_COL_STEP, 0.0, Cluster::Encoder)
                .size(KNOB_WIDTH, 1.0)
                .knob(i as u8),
        );
    }

    // --- Alpha block ---
    // Rows 1,3,5: first key 1.5u at col 0, remaining at col 0.5+c (c=1..)
    // Rows 2,4:   all 1u starting at col 0
    //
    // Raw MIDI from firmware SWITCH_TO_NOTE:
    //   Row 1 HE1–14:  base 78 +2/key (14 keys)
    //   Row 2 HE15–29: base 71 +2/key (15 keys)
    //   Row 3 HE30–43: base 66 +2/key (14 keys)
    //   Row 4 HE44–58: base 59 +2/key (15 keys)
    //   Row 5 HE59–70: 54,[gap],58–76,[gap],80 (12 keys)

    // Row 1
    let m = 78u8;
    keys.push(
        Key::new(next_id(), note_name(m), 0.0, 1.0, note_cluster(m))
            .size(1.5, 1.0)
            .note(m),
    );
    for c in 1..14_usize {
        let m = 78 + 2 * c as u8;
        if c == 13 {
            keys.push(
                Key::new(
                    next_id(),
                    note_name(m),
                    0.5 + c as f32,
                    1.0,
                    note_cluster(m),
                )
                .size(1.5, 1.0)
                .note(m),
            );
        } else {
            keys.push(
                Key::new(
                    next_id(),
                    note_name(m),
                    0.5 + c as f32,
                    1.0,
                    note_cluster(m),
                )
                .note(m),
            );
        }
    }

    // Row 2
    for c in 0..15_usize {
        let m = 71 + 2 * c as u8;
        keys.push(Key::new(next_id(), note_name(m), c as f32, 2.0, note_cluster(m)).note(m));
    }

    // Row 3
    let m = 66u8;
    keys.push(
        Key::new(next_id(), note_name(m), 0.0, 3.0, note_cluster(m))
            .size(1.5, 1.0)
            .note(m),
    );
    for c in 1..14_usize {
        let m = 66 + 2 * c as u8;
        keys.push(
            Key::new(
                next_id(),
                note_name(m),
                0.5 + c as f32,
                3.0,
                note_cluster(m),
            )
            .note(m),
        );
    }

    // Row 4
    for c in 0..15_usize {
        let m = 59 + 2 * c as u8;
        keys.push(Key::new(next_id(), note_name(m), c as f32, 4.0, note_cluster(m)).note(m));
    }

    // Row 5: 1.5u F# at col 0, 1u gap, A#–E at cols 2.5–11.5, 1u gap (USB), G# at col 13.5
    let m = 54u8;
    keys.push(
        Key::new(next_id(), note_name(m), 0.0, 5.0, note_cluster(m))
            .size(1.5, 1.0)
            .note(m),
    );
    for c in 1..11_usize {
        let m = (56 + 2 * c) as u8; // c=1→58(A#) … c=10→76(E)
        keys.push(
            Key::new(
                next_id(),
                note_name(m),
                1.5 + c as f32,
                5.0,
                note_cluster(m),
            )
            .note(m),
        );
    }
    let m = 80u8;
    keys.push(Key::new(next_id(), note_name(m), 13.5, 5.0, note_cluster(m)).note(m));

    // --- Nav ---
    let nav_col = NAV_COL;
    keys.push(Key::new(next_id(), "Insert", nav_col, 1.0, Cluster::Nav).sub("Triangle"));
    keys.push(Key::new(
        next_id(),
        "Home",
        nav_col + 1.0,
        1.0,
        Cluster::Nav,
    ).sub("Square"));
    keys.push(Key::new(
        next_id(),
        "PgUp",
        nav_col + 2.0,
        1.0,
        Cluster::Nav,
    ).sub("Saw"));
    keys.push(Key::new(next_id(), "Delete", nav_col, 2.0, Cluster::Nav));
    keys.push(Key::new(next_id(), "End", nav_col + 1.0, 2.0, Cluster::Nav));
    keys.push(Key::new(
        next_id(),
        "PgDn",
        nav_col + 2.0,
        2.0,
        Cluster::Nav,
    ));

    // --- Arrow ---
    let arrow_col = nav_col;
    let arrow_row = 4.0;
    keys.push(Key::new(
        next_id(),
        "↑",
        arrow_col + 1.0,
        arrow_row,
        Cluster::Arrow,
    ));
    keys.push(Key::new(
        next_id(),
        "←",
        arrow_col,
        arrow_row + 1.0,
        Cluster::Arrow,
    ));
    keys.push(Key::new(
        next_id(),
        "↓",
        arrow_col + 1.0,
        arrow_row + 1.0,
        Cluster::Arrow,
    ));
    keys.push(Key::new(
        next_id(),
        "→",
        arrow_col + 2.0,
        arrow_row + 1.0,
        Cluster::Arrow,
    ));

    // --- Numpad (doubles as the drum pad cluster — see DRUM_PAD_NOTES) ---
    let numpad_start = keys.len();
    let np = NUMPAD_COL;
    keys.push(Key::new(next_id(), "Num", np, 0.0, Cluster::Numpad).sub("Lock"));
    keys.push(Key::new(next_id(), "/", np + 1.0, 0.0, Cluster::Numpad));
    keys.push(Key::new(next_id(), "*", np + 2.0, 0.0, Cluster::Numpad));
    keys.push(Key::new(next_id(), "-", np + 3.0, 0.0, Cluster::Numpad));

    keys.push(Key::new(next_id(), "7", np, 1.0, Cluster::Numpad).sub("Home"));
    keys.push(Key::new(next_id(), "8", np + 1.0, 1.0, Cluster::Numpad).sub("↑"));
    keys.push(Key::new(next_id(), "9", np + 2.0, 1.0, Cluster::Numpad).sub("Pg Up"));
    keys.push(Key::new(next_id(), "+", np + 3.0, 1.0, Cluster::Numpad));

    keys.push(Key::new(next_id(), "4", np, 2.0, Cluster::Numpad).sub("←"));
    keys.push(Key::new(next_id(), "5", np + 1.0, 2.0, Cluster::Numpad));
    keys.push(Key::new(next_id(), "6", np + 2.0, 2.0, Cluster::Numpad).sub("→"));
    // The real board's drum grid is a uniform 4×5 — no key here spans two rows
    // (confirmed via KiCad footprint coordinates for HE81–HE100), so "+" above
    // is split into two 1×1 keys instead of one 1×2 key.
    keys.push(Key::new(next_id(), "+", np + 3.0, 2.0, Cluster::Numpad));

    keys.push(Key::new(next_id(), "1", np, 3.0, Cluster::Numpad).sub("End"));
    keys.push(Key::new(next_id(), "2", np + 1.0, 3.0, Cluster::Numpad).sub("↓"));
    keys.push(Key::new(next_id(), "3", np + 2.0, 3.0, Cluster::Numpad).sub("Pg Dn"));
    keys.push(Key::new(next_id(), "Shift", np + 3.0, 3.0, Cluster::Numpad));

    keys.push(Key::new(next_id(), "0", np, 4.0, Cluster::Numpad).sub("Ins"));
    keys.push(Key::new(next_id(), "00", np + 1.0, 4.0, Cluster::Numpad));
    keys.push(Key::new(next_id(), ".", np + 2.0, 4.0, Cluster::Numpad).sub("Del"));
    keys.push(Key::new(next_id(), "PgUp", np + 3.0, 4.0, Cluster::Numpad));

    // Build note maps from the midi_note field on each key.
    // Keys are iterated in row order (1→5), so note_to_all_keys Vecs are top→bottom.
    let mut note_to_all_keys: HashMap<u8, Vec<KeyId>> = HashMap::new();
    for key in &keys {
        if let Some(n) = key.midi_note {
            note_to_all_keys.entry(n).or_default().push(key.id);
        }
    }

    let keyboard_notes: HashSet<u8> = note_to_all_keys.keys().copied().collect();

    let drum_note_to_key: HashMap<u8, KeyId> = keys[numpad_start..]
        .iter()
        .zip(DRUM_PAD_NOTES.iter())
        .map(|(key, &note)| (note, key.id))
        .collect();

    Layout {
        keys,
        note_to_all_keys,
        keyboard_notes,
        drum_note_to_key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::key_rect;

    fn horizontal_bounds(layout: &Layout, cluster: Cluster) -> (f32, f32) {
        layout.keys.iter()
            .filter(|key| key.cluster == cluster)
            .map(key_rect)
            .fold((f32::MAX, f32::MIN), |(left, right), rect| {
                (left.min(rect.x), right.max(rect.x + rect.width))
            })
    }

    #[test]
    fn right_hand_sections_share_alignment_and_spacing() {
        let layout = build_layout();
        let alpha = layout.keys.iter()
            .filter(|key| matches!(key.cluster, Cluster::Alpha | Cluster::AlphaLight))
            .map(key_rect)
            .fold((f32::MAX, f32::MIN), |(left, right), rect| {
                (left.min(rect.x), right.max(rect.x + rect.width))
            });
        let nav = horizontal_bounds(&layout, Cluster::Nav);
        let arrows = horizontal_bounds(&layout, Cluster::Arrow);
        let numpad = horizontal_bounds(&layout, Cluster::Numpad);

        assert_eq!(nav, arrows);
        assert_eq!(nav.0 - alpha.1, numpad.0 - arrows.1);
    }
}
