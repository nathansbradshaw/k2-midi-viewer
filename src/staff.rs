use std::collections::{HashMap, HashSet};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Text};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};

use crate::key::KeyId;
use crate::midi::{MidiFile, Note};
use crate::render::CANVAS_FONT;
use crate::synth::DRUM_CHANNEL;
use crate::Message;

const LINE_SPACING: f32 = 12.0;
const HALF_SPACE: f32 = LINE_SPACING / 2.0;
const CLEF_WIDTH: f32 = 60.0;
const BEHIND_BEATS: f32 = 2.0;
const AHEAD_BEATS: f32 = 12.0;

// Diatonic step within the octave (0=C, 1=D, 2=E, 3=F, 4=G, 5=A, 6=B).
// Sharps share their natural's diatonic slot.
const DIATONIC: [i32; 12] = [0, 0, 1, 1, 2, 3, 3, 4, 4, 5, 5, 6];

const NOTE_NAMES: [&str; 12] =
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Staff slot for a MIDI note:  0 = C4 (middle C), positive = up, negative = down.
fn staff_slot(midi: u8) -> i32 {
    let octave = (midi as i32 / 12) - 1; // MIDI 60 = C4 → octave 4
    DIATONIC[(midi % 12) as usize] + (octave - 4) * 7
}

fn is_sharp(midi: u8) -> bool {
    matches!(midi % 12, 1 | 3 | 6 | 8 | 10)
}

/// e.g. 60 → "C4"
pub fn note_name(midi: u8) -> String {
    let octave = (midi as i32 / 12) - 1;
    format!("{}{}", NOTE_NAMES[(midi % 12) as usize], octave)
}

/// Inverse of the `tick_x` mapping used in `draw_staff` — converts a canvas-local
/// x coordinate back into an absolute tick, given the current scroll position.
fn x_to_tick(x: f32, width: f32, tpb: u16, pos: u64) -> u64 {
    let ppb = (width - CLEF_WIDTH) / (BEHIND_BEATS + AHEAD_BEATS);
    let ppt = ppb / tpb as f32;
    let playhead_x = CLEF_WIDTH + BEHIND_BEATS * ppb;
    let dt = (x - playhead_x) as f64 / ppt as f64;
    (pos as f64 + dt).round().max(0.0) as u64
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct StaffCanvas<'a> {
    pub midi_file:     Option<&'a MidiFile>,
    pub position_tick: u64,
    pub track_muted:   &'a [bool],
    pub octave_offset: i8,
    pub track_octave:  &'a [i8],
    /// Currently selected (start_tick, end_tick) range, drawn as a highlight band.
    pub selection:     Option<(u64, u64)>,
    /// Raw firmware notes present on the melodic keyboard, for the out-of-range check.
    pub keyboard_notes: &'a HashSet<u8>,
    /// GM percussion note → drum pad key, for the out-of-range check on channel 10.
    pub drum_note_to_key: &'a HashMap<u8, KeyId>,
}

#[derive(Default)]
pub struct StaffState {
    cache:      canvas::Cache,
    dragging:   bool,
    anchor_x:   f32,
}

impl<'a> canvas::Program<Message> for StaffCanvas<'a> {
    type State = StaffState;

    fn update(
        &self, state: &mut StaffState, event: &canvas::Event,
        bounds: Rectangle, cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let Some(f) = self.midi_file else {
            return None;
        };

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_in(bounds) {
                    if p.x >= CLEF_WIDTH {
                        state.dragging = true;
                        state.anchor_x = p.x;
                        let t = x_to_tick(p.x, bounds.width, f.ticks_per_beat, self.position_tick);
                        return Some(
                            canvas::Action::publish(Message::StaffSelectionChanged(Some((t, t))))
                                .and_capture(),
                        );
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging {
                    if let Some(p) = cursor.position_in(bounds) {
                        let a = x_to_tick(state.anchor_x, bounds.width, f.ticks_per_beat, self.position_tick);
                        let b = x_to_tick(p.x, bounds.width, f.ticks_per_beat, self.position_tick);
                        let range = (a.min(b), a.max(b));
                        return Some(
                            canvas::Action::publish(Message::StaffSelectionChanged(Some(range)))
                                .and_capture(),
                        );
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    let clicked_without_drag = cursor
                        .position_in(bounds)
                        .map(|p| (p.x - state.anchor_x).abs() < 4.0)
                        .unwrap_or(false);
                    if clicked_without_drag {
                        return Some(
                            canvas::Action::publish(Message::StaffSelectionChanged(None))
                                .and_capture(),
                        );
                    }
                    return Some(canvas::Action::capture());
                }
            }
            _ => {}
        }

        None
    }

    fn draw(
        &self, state: &StaffState, renderer: &Renderer,
        _theme: &Theme, bounds: Rectangle, _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        // The staff scrolls every 16 ms, so always force a fresh draw.
        state.cache.clear();
        let geo = state.cache.draw(renderer, bounds.size(), |frame| {
            draw_staff(
                frame, bounds.size(),
                self.midi_file, self.position_tick,
                self.track_muted, self.octave_offset, self.track_octave,
                self.selection,
                self.keyboard_notes, self.drum_note_to_key,
            );
        });
        vec![geo]
    }

    fn mouse_interaction(
        &self, _s: &StaffState, _b: Rectangle, _c: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

// ---------------------------------------------------------------------------
// Colour helpers
// ---------------------------------------------------------------------------

fn track_note_color(track: usize, is_active: bool, is_past: bool) -> Color {
    let (r, g, b) = crate::render::TRACK_COLORS[track % crate::render::TRACK_COLORS.len()];
    let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    if is_active {
        Color::from_rgba(rf, gf, bf, 1.0)           // full track colour
    } else if is_past {
        Color::from_rgba(rf * 0.27, gf * 0.27, bf * 0.27, 1.0) // very dim
    } else {
        Color::from_rgba(rf * 0.72, gf * 0.72, bf * 0.72, 1.0) // dimmed upcoming
    }
}

/// Lowest and highest raw MIDI note the melodic keyboard's physical keys cover.
/// This is fixed hardware geometry — the octave shift moves *songs* into this
/// range (see `note_fits`), it never moves the keyboard itself, so unlike
/// `note_fits` this does not apply `octave_offset`. Notes plotted on the staff
/// are shown in shifted (post-transposition) space, which is exactly the space
/// this raw range lives in, so the two compare directly.
fn keyboard_range(keyboard_notes: &HashSet<u8>) -> Option<(u8, u8)> {
    keyboard_notes
        .iter()
        .fold(None, |acc, &n| match acc {
            None           => Some((n, n)),
            Some((lo, hi)) => Some((lo.min(n), hi.max(n))),
        })
}

/// Whether `note` lands on a physical key — a drum pad for channel 10, or the
/// octave-shifted melodic keyboard otherwise. Mirrors the logic in main.rs's
/// rebuild_all_notes_cache, kept in sync so the staff and keyboard agree.
fn note_fits(
    note: &Note,
    keyboard_notes: &HashSet<u8>,
    drum_note_to_key: &HashMap<u8, KeyId>,
    octave_offset: i8,
    track_octave: &[i8],
) -> bool {
    if note.channel == DRUM_CHANNEL {
        drum_note_to_key.contains_key(&note.midi_note)
    } else {
        let shift = crate::midi::combined_octave_shift(octave_offset, track_octave, note.track);
        let shifted = (note.midi_note as i16 + shift).clamp(0, 127) as u8;
        keyboard_notes.contains(&shifted)
    }
}

fn draw_treble_clef(frame: &mut Frame, g_line_y: f32, color: Color) {
    let stroke = canvas::Stroke::default()
        .with_color(color)
        .with_width(2.8)
        .with_line_cap(canvas::LineCap::Round)
        .with_line_join(canvas::LineJoin::Round);

    let stem = Path::new(|b| {
        b.move_to(Point::new(31.0, g_line_y + 38.0));
        b.bezier_curve_to(
            Point::new(35.0, g_line_y + 19.0),
            Point::new(32.0, g_line_y - 3.0),
            Point::new(29.0, g_line_y - 20.0),
        );
        b.bezier_curve_to(
            Point::new(26.0, g_line_y - 35.0),
            Point::new(30.0, g_line_y - 45.0),
            Point::new(35.0, g_line_y - 48.0),
        );
        b.bezier_curve_to(
            Point::new(42.0, g_line_y - 39.0),
            Point::new(36.0, g_line_y - 27.0),
            Point::new(29.0, g_line_y - 20.0),
        );
    });
    frame.stroke(&stem, stroke);

    let spiral = Path::new(|b| {
        b.move_to(Point::new(31.0, g_line_y - 17.0));
        b.bezier_curve_to(
            Point::new(13.0, g_line_y - 12.0),
            Point::new(14.0, g_line_y + 12.0),
            Point::new(31.0, g_line_y + 14.0),
        );
        b.bezier_curve_to(
            Point::new(47.0, g_line_y + 16.0),
            Point::new(49.0, g_line_y - 5.0),
            Point::new(36.0, g_line_y - 9.0),
        );
        b.bezier_curve_to(
            Point::new(25.0, g_line_y - 13.0),
            Point::new(21.0, g_line_y - 1.0),
            Point::new(27.0, g_line_y + 5.0),
        );
        b.bezier_curve_to(
            Point::new(33.0, g_line_y + 11.0),
            Point::new(43.0, g_line_y + 5.0),
            Point::new(40.0, g_line_y - 2.0),
        );
    });
    frame.stroke(&spiral, stroke);

    let hook = Path::new(|b| {
        b.move_to(Point::new(32.0, g_line_y + 22.0));
        b.bezier_curve_to(
            Point::new(45.0, g_line_y + 23.0),
            Point::new(42.0, g_line_y + 39.0),
            Point::new(28.0, g_line_y + 38.0),
        );
    });
    frame.stroke(&hook, stroke);
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn draw_staff(
    frame: &mut Frame,
    size: Size,
    midi: Option<&MidiFile>,
    pos: u64,
    track_muted: &[bool],
    octave_offset: i8,
    track_octave: &[i8],
    selection: Option<(u64, u64)>,
    keyboard_notes: &HashSet<u8>,
    drum_note_to_key: &HashMap<u8, KeyId>,
) {
    let w = size.width;
    let h = size.height;

    frame.fill(
        &Path::rectangle(Point::ORIGIN, size),
        Color::from_rgb8(0x08, 0x09, 0x04),
    );

    // Persistent phosphor grid and restrained scanlines keep the display
    // visually active in standby without turning the empty state into a
    // different component. These are presentation-only; staff geometry and
    // note interaction remain unchanged.
    let grid_color = Color::from_rgba8(0xd2, 0x96, 0x1e, 0.10);
    let mut x = 0.0;
    while x <= w {
        frame.stroke(
            &Path::line(Point::new(x, 0.0), Point::new(x, h)),
            canvas::Stroke::default().with_color(grid_color).with_width(1.0),
        );
        x += 64.0;
    }
    let mut y = 0.0;
    while y <= h {
        frame.stroke(
            &Path::line(Point::new(0.0, y), Point::new(w, y)),
            canvas::Stroke::default().with_color(grid_color).with_width(1.0),
        );
        y += 32.0;
    }
    let scanline_color = Color::from_rgba(0.0, 0.0, 0.0, 0.075);
    let mut scan_y = 2.0;
    while scan_y < h {
        frame.fill(
            &Path::rectangle(Point::new(0.0, scan_y), Size::new(w, 1.0)),
            scanline_color,
        );
        scan_y += 4.0;
    }

    let Some(f) = midi else {
        frame.fill_text(Text {
            content: "Load a MIDI file to see staff notation".to_string(),
            position: Point::new(w / 2.0, h / 2.0),
            color: Color::from_rgba8(0xe9, 0xa3, 0x29, 0.72),
            size: iced::Pixels(15.0),
            font: CANVAS_FONT,
            align_x: Horizontal::Center.into(),
            align_y: Vertical::Center,
            ..Text::default()
        });
        return;
    };

    let tpb = f.ticks_per_beat as f32;
    let ppb = (w - CLEF_WIDTH) / (BEHIND_BEATS + AHEAD_BEATS); // pixels per beat
    let ppt = ppb / tpb;                                         // pixels per tick
    let playhead_x = CLEF_WIDTH + BEHIND_BEATS * ppb;
    // The device's playable range sits almost entirely at or above middle C, so
    // the single staff drawn here is a treble staff — push it down to leave
    // generous headroom above the top line for the high notes that dominate.
    let ref_y = h * 0.78;

    let tick_x  = |t: u64|  -> f32 { playhead_x + (t as f64 - pos as f64) as f32 * ppt };
    let slot_y  = |s: i32|  -> f32 { ref_y - s as f32 * HALF_SPACE };

    // ── Keyboard range band ─────────────────────────────────────────────────
    // Shades the staff positions the physical keyboard can actually reach.
    // Notes are plotted post-octave-shift, which is the same space the raw
    // keyboard range lives in, so this band is independent of octave_offset.
    if let Some((lo, hi)) = keyboard_range(keyboard_notes) {
        let y_top    = slot_y(staff_slot(hi)) - HALF_SPACE;
        let y_bottom = slot_y(staff_slot(lo)) + HALF_SPACE;
        let range_col = Color::from_rgb8(0xA5, 0xD6, 0xA7);
        frame.fill(
            &Path::rectangle(Point::new(CLEF_WIDTH, y_top), Size::new(w - CLEF_WIDTH, y_bottom - y_top)),
            Color { a: 0.10, ..range_col },
        );
        for y in [y_top, y_bottom] {
            frame.stroke(
                &Path::line(Point::new(CLEF_WIDTH - 4.0, y), Point::new(w, y)),
                canvas::Stroke::default().with_color(Color { a: 0.55, ..range_col }).with_width(1.0),
            );
        }
    }

    // ── Selection band ──────────────────────────────────────────────────────
    if let Some((s, e)) = selection {
        let x0 = tick_x(s).max(CLEF_WIDTH);
        let x1 = tick_x(e).min(w);
        if x1 > x0 {
            let band_col = Color::from_rgb8(0x45, 0xD4, 0xCB);
            frame.fill(
                &Path::rectangle(Point::new(x0, 0.0), Size::new(x1 - x0, h)),
                Color { a: 0.14, ..band_col },
            );
            for x in [x0, x1] {
                frame.stroke(
                    &Path::line(Point::new(x, 0.0), Point::new(x, h)),
                    canvas::Stroke::default().with_color(Color { a: 0.8, ..band_col }).with_width(1.5),
                );
            }
        }
    }

    // ── Staff lines ──────────────────────────────────────────────────────────
    // Single treble staff — the device's range rarely dips into bass territory,
    // so notes below middle C are shown with ledger lines instead of a bass staff.
    let line_col = Color::from_rgba8(0xd2, 0x96, 0x1e, 0.34);
    for &s in &[2i32, 4, 6, 8, 10] {
        frame.stroke(
            &Path::line(Point::new(CLEF_WIDTH - 4.0, slot_y(s)), Point::new(w, slot_y(s))),
            canvas::Stroke::default().with_color(line_col).with_width(1.0),
        );
    }

    // ── Bar lines ───────────────────────────────────────────────────────────
    let ticks_per_bar = (tpb as u64)
        .saturating_mul(f.time_sig.0 as u64)
        .saturating_mul(4)
        / (f.time_sig.1 as u64).max(1);
    if ticks_per_bar > 0 {
        let vis_start = pos.saturating_sub(((BEHIND_BEATS + 1.0) * tpb) as u64);
        let vis_end   = pos + ((AHEAD_BEATS + 1.0) * tpb) as u64;
        let first     = (vis_start / ticks_per_bar) * ticks_per_bar;
        let bar_col   = Color::from_rgba8(0xd2, 0x96, 0x1e, 0.16);
        let mut bt    = first;
        while bt <= vis_end {
            let x = tick_x(bt);
            if x >= CLEF_WIDTH {
                frame.stroke(
                    &Path::line(Point::new(x, 0.0), Point::new(x, h)),
                    canvas::Stroke::default().with_color(bar_col).with_width(1.0),
                );
            }
            match bt.checked_add(ticks_per_bar) {
                Some(v) => bt = v,
                None    => break,
            }
        }
    }

    // ── Clef symbol ─────────────────────────────────────────────────────────
    let clef_col = Color::from_rgb8(0xdf, 0xb8, 0x69);
    draw_treble_clef(frame, slot_y(4), clef_col);

    // ── Notes ───────────────────────────────────────────────────────────────
    // Notes are sorted chronologically by start tick, but a concurrent chord
    // or an overlapping sustained note from another track can start a tick
    // later while still needing its head/ring drawn *above* an earlier note's
    // long duration bar. Drawing each note's whole stack (bar → head → ring →
    // stem) atomically in start-tick order let a later note's wide bar paint
    // over an earlier note's head. Instead, collect the visible notes once and
    // draw every bar first (background), then every head/ring/stem/sharp on
    // top (foreground) — with the currently-active notes drawn last within
    // that foreground pass, so the note actually playing is never buried
    // under a merely-nearby one.
    let vis_start = pos.saturating_sub(((BEHIND_BEATS + 1.0) * tpb) as u64);
    let vis_end   = pos + ((AHEAD_BEATS + 1.0) * tpb) as u64;
    let note_r    = HALF_SPACE * 0.82; // note-head radius

    struct Plotted<'n> {
        note:      &'n Note,
        x:         f32,
        y:         f32,
        slot:      i32,
        shifted:   u8,
        is_active: bool,
        is_past:   bool,
        fits:      bool,
        col:       Color,
    }

    let mut plotted: Vec<Plotted> = f.notes.iter()
        .filter(|note| !(note.start_tick > vis_end || note.end_tick < vis_start))
        .filter(|note| !track_muted.get(note.track).copied().unwrap_or(false))
        .filter_map(|note| {
            let shift = crate::midi::combined_octave_shift(octave_offset, track_octave, note.track);
            let shifted = (note.midi_note as i16 + shift).clamp(0, 127) as u8;
            let slot = staff_slot(shifted);
            let x = tick_x(note.start_tick);
            let y = slot_y(slot);
            if x < CLEF_WIDTH - 24.0 || x > w + 16.0 { return None; }

            let is_active = note.start_tick <= pos && pos < note.end_tick;
            let is_past   = note.end_tick   <= pos;
            Some(Plotted {
                note, x, y, slot, shifted, is_active, is_past,
                fits: note_fits(note, keyboard_notes, drum_note_to_key, octave_offset, track_octave),
                col: track_note_color(note.track, is_active, is_past),
            })
        })
        .collect();

    // Background pass: duration bars and ledger lines for every note.
    for p in &plotted {
        let end_x = tick_x(p.note.end_tick).min(w);
        if end_x > p.x + note_r {
            let alpha = if p.is_active { 0.42f32 } else if p.is_past { 0.16 } else { 0.18 };
            frame.stroke(
                &Path::line(Point::new(p.x, p.y), Point::new(end_x, p.y)),
                canvas::Stroke::default()
                    .with_color(Color { a: alpha, ..p.col })
                    .with_width(note_r * 1.7),
            );
        }
        draw_ledgers(frame, p.slot, p.x, note_r, &slot_y, line_col);
    }

    // Foreground pass: heads, warning rings, stems, and sharps — active notes
    // last so the one actually sounding is always on top.
    plotted.sort_by_key(|p| p.is_active);
    for p in &plotted {
        frame.fill(&Path::circle(Point::new(p.x, p.y), note_r), p.col);

        // Out-of-range warning ring — always drawn at full strength, independent
        // of the active/past dimming above, so it never fades into the background.
        if !p.fits {
            frame.stroke(
                &Path::circle(Point::new(p.x, p.y), note_r + 3.5),
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(0xFF, 0x40, 0x30, 0.95))
                    .with_width(2.2),
            );
        }

        // Stem — up when below the staff's middle line (B4, slot 6), down when above.
        let stem_up = p.slot < 6;
        let sx = p.x + if stem_up { note_r * 0.85 } else { -note_r * 0.85 };
        let sy = p.y + if stem_up { -3.5 * LINE_SPACING } else { 3.5 * LINE_SPACING };
        frame.stroke(
            &Path::line(Point::new(sx, p.y), Point::new(sx, sy)),
            canvas::Stroke::default().with_color(p.col).with_width(1.5),
        );

        // Sharp accidental
        if is_sharp(p.shifted) && !p.is_past {
            frame.fill_text(Text {
                content: "#".to_string(),
                position: Point::new(p.x - note_r * 2.8, p.y),
                color: p.col,
                size: iced::Pixels(11.0),
                font: CANVAS_FONT,
                align_x: Horizontal::Center.into(),
                align_y: Vertical::Center,
                ..Text::default()
            });
        }
    }

    // ── Playhead ─────────────────────────────────────────────────────────────
    frame.stroke(
        &Path::line(Point::new(playhead_x, 0.0), Point::new(playhead_x, h)),
        canvas::Stroke::default()
            .with_color(Color::from_rgba8(0xE9, 0xA3, 0x29, 0.92))
            .with_width(2.0),
    );
}

// ---------------------------------------------------------------------------
// Ledger lines
// ---------------------------------------------------------------------------

fn draw_ledgers(
    frame: &mut Frame,
    slot: i32,
    x: f32,
    note_r: f32,
    slot_y: &impl Fn(i32) -> f32,
    color: Color,
) {
    let hw = note_r * 2.1;

    let draw_at = |frame: &mut Frame, s: i32| {
        let y = slot_y(s);
        frame.stroke(
            &Path::line(Point::new(x - hw, y), Point::new(x + hw, y)),
            canvas::Stroke::default().with_color(color).with_width(1.0),
        );
    };

    // Above the staff top (F5, slot 10): ledgers at 12, 14, … up to even(slot).
    // A note in the space just above the staff (slot 11) gets no ledger;
    // a note on or above the first ledger line (slot ≥ 12) does.
    if slot > 10 {
        let top = if slot % 2 == 0 { slot } else { slot - 1 };
        let mut s = 12i32;
        while s <= top { draw_at(frame, s); s += 2; }
    }

    // Below the staff bottom (E4, slot 2): ledgers at 0 (middle C), -2, -4, …
    // down to even(slot). A note in the space just below the staff (slot 1)
    // gets no ledger; middle C (slot 0) and anything lower does.
    if slot <= 0 {
        let bottom = if slot % 2 == 0 { slot } else { slot + 1 };
        let mut s = 0i32;
        while s >= bottom { draw_at(frame, s); s -= 2; }
    }
}
