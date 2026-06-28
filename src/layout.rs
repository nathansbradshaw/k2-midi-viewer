use crate::key::{Cluster, Key};

// Sharp notes (C#, D#, F#, G#, A#) get the dark alpha color — mirrors piano black keys.
// Natural notes (C, D, E, F, G, A, B) get the light alpha color — mirrors piano white keys.
fn note_cluster(midi: u8) -> Cluster {
    match midi % 12 {
        1 | 3 | 6 | 8 | 10 => Cluster::Alpha,
        _ => Cluster::AlphaLight,
    }
}

// Returns pitch-class name from a raw MIDI note number.
// Octave-independent: the firmware's octave setting shifts all notes uniformly.
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

pub fn build_layout() -> Vec<Key> {
    let mut id = 0u32;
    let mut next_id = || {
        id += 1;
        id
    };

    let mut keys = Vec::new();

    // --- Encoders: 8 knobs along the top ---
    for i in 0..8 {
        keys.push(Key::new(next_id(), "", i as f32, 0.0, Cluster::Encoder).knob());
    }

    // --- Alpha block ---
    // Rows 1, 3, 5 (unshifted): first key is 1.5u, remaining keys start at col 1.5
    // Rows 2, 4 (shifted): all 1u, starting right at the left edge (col 0)
    //
    // MIDI assignments from firmware SWITCH_TO_NOTE:
    //   Row 1 HE1–14:  base 78, +2/key  (14 keys)
    //   Row 2 HE15–29: base 71, +2/key  (15 keys)
    //   Row 3 HE30–43: base 66, +2/key  (14 keys)
    //   Row 4 HE44–58: base 59, +2/key  (15 keys)
    //   Row 5 HE59–70: 54,[gap],58–76,[gap],80  (12 keys; gaps at positions 1 & 12)

    // Row 1
    keys.push(Key::new(next_id(), note_name(78), 0.0, 1.0, note_cluster(78)).size(1.5, 1.0));
    for c in 1..14_usize {
        let midi = 78 + 2 * c as u8;
        keys.push(Key::new(next_id(), note_name(midi), 0.5 + c as f32, 1.0, note_cluster(midi)));
    }

    // Row 2
    for c in 0..15_usize {
        let midi = 71 + 2 * c as u8;
        keys.push(Key::new(next_id(), note_name(midi), c as f32, 2.0, note_cluster(midi)));
    }

    // Row 3
    keys.push(Key::new(next_id(), note_name(66), 0.0, 3.0, note_cluster(66)).size(1.5, 1.0));
    for c in 1..14_usize {
        let midi = 66 + 2 * c as u8;
        keys.push(Key::new(next_id(), note_name(midi), 0.5 + c as f32, 3.0, note_cluster(midi)));
    }

    // Row 4
    for c in 0..15_usize {
        let midi = 59 + 2 * c as u8;
        keys.push(Key::new(next_id(), note_name(midi), c as f32, 4.0, note_cluster(midi)));
    }

    // Row 5: 1.5u F# at col 0, 1u gap, A#–E at cols 2.5–11.5, 1u gap (USB connector), G# at col 13.5
    keys.push(Key::new(next_id(), note_name(54), 0.0, 5.0, note_cluster(54)).size(1.5, 1.0));
    for c in 1..11_usize {
        let midi = (56 + 2 * c) as u8; // c=1→58(A#), c=10→76(E)
        keys.push(Key::new(next_id(), note_name(midi), 1.5 + c as f32, 5.0, note_cluster(midi)));
    }
    keys.push(Key::new(next_id(), note_name(80), 13.5, 5.0, note_cluster(80)));

    // --- Nav cluster ---
    let nav_col = 16.5;
    keys.push(Key::new(next_id(), "Insert", nav_col, 1.0, Cluster::Nav));
    keys.push(Key::new(next_id(), "Home", nav_col + 1.0, 1.0, Cluster::Nav));
    keys.push(Key::new(next_id(), "PgUp", nav_col + 2.0, 1.0, Cluster::Nav));
    keys.push(Key::new(next_id(), "Delete", nav_col, 2.0, Cluster::Nav));
    keys.push(Key::new(next_id(), "End", nav_col + 1.0, 2.0, Cluster::Nav));
    keys.push(Key::new(next_id(), "PgDn", nav_col + 2.0, 2.0, Cluster::Nav));

    // --- Arrow cluster (inverted-T) ---
    let arrow_col = nav_col + 0.5;
    let arrow_row = 4.0;
    keys.push(Key::new(next_id(), "↑", arrow_col + 1.0, arrow_row, Cluster::Arrow));
    keys.push(Key::new(next_id(), "←", arrow_col, arrow_row + 1.0, Cluster::Arrow));
    keys.push(Key::new(next_id(), "↓", arrow_col + 1.0, arrow_row + 1.0, Cluster::Arrow));
    keys.push(Key::new(next_id(), "→", arrow_col + 2.0, arrow_row + 1.0, Cluster::Arrow));

    // --- Numpad ---
    let np = 20.5;
    keys.push(Key::new(next_id(), "Num", np, 0.0, Cluster::Numpad).sub("Lock"));
    keys.push(Key::new(next_id(), "/", np + 1.0, 0.0, Cluster::Numpad));
    keys.push(Key::new(next_id(), "*", np + 2.0, 0.0, Cluster::Numpad));
    keys.push(Key::new(next_id(), "-", np + 3.0, 0.0, Cluster::Numpad));

    keys.push(Key::new(next_id(), "7", np, 1.0, Cluster::Numpad).sub("Home"));
    keys.push(Key::new(next_id(), "8", np + 1.0, 1.0, Cluster::Numpad).sub("↑"));
    keys.push(Key::new(next_id(), "9", np + 2.0, 1.0, Cluster::Numpad).sub("Pg Up"));
    keys.push(Key::new(next_id(), "+", np + 3.0, 1.0, Cluster::Numpad).size(1.0, 2.0));

    keys.push(Key::new(next_id(), "4", np, 2.0, Cluster::Numpad).sub("←"));
    keys.push(Key::new(next_id(), "5", np + 1.0, 2.0, Cluster::Numpad));
    keys.push(Key::new(next_id(), "6", np + 2.0, 2.0, Cluster::Numpad).sub("→"));

    keys.push(Key::new(next_id(), "1", np, 3.0, Cluster::Numpad).sub("End"));
    keys.push(Key::new(next_id(), "2", np + 1.0, 3.0, Cluster::Numpad).sub("↓"));
    keys.push(Key::new(next_id(), "3", np + 2.0, 3.0, Cluster::Numpad).sub("Pg Dn"));
    keys.push(Key::new(next_id(), "Shift", np + 3.0, 3.0, Cluster::Numpad));

    keys.push(Key::new(next_id(), "0", np, 4.0, Cluster::Numpad).sub("Ins"));
    keys.push(Key::new(next_id(), "00", np + 1.0, 4.0, Cluster::Numpad));
    keys.push(Key::new(next_id(), ".", np + 2.0, 4.0, Cluster::Numpad).sub("Del"));
    keys.push(Key::new(next_id(), "PgUp", np + 3.0, 4.0, Cluster::Numpad));

    keys
}
