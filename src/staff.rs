use std::collections::{HashMap, HashSet};

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Text};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};

use crate::key::KeyId;
use crate::midi::{MidiFile, Note};
use crate::synth::DRUM_CHANNEL;
use crate::Message;

pub const STAFF_HEIGHT: f32 = 180.0;

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
        &self, state: &mut StaffState, event: canvas::Event,
        bounds: Rectangle, cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        let Some(f) = self.midi_file else {
            return (canvas::event::Status::Ignored, None);
        };

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_in(bounds) {
                    if p.x >= CLEF_WIDTH {
                        state.dragging = true;
                        state.anchor_x = p.x;
                        let t = x_to_tick(p.x, bounds.width, f.ticks_per_beat, self.position_tick);
                        return (
                            canvas::event::Status::Captured,
                            Some(Message::StaffSelectionChanged(Some((t, t)))),
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
                        return (
                            canvas::event::Status::Captured,
                            Some(Message::StaffSelectionChanged(Some(range))),
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
                        return (
                            canvas::event::Status::Captured,
                            Some(Message::StaffSelectionChanged(None)),
                        );
                    }
                    return (canvas::event::Status::Captured, None);
                }
            }
            _ => {}
        }

        (canvas::event::Status::Ignored, None)
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
                self.track_muted, self.octave_offset,
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
        Color::new(rf, gf, bf, 1.0)           // full track colour
    } else if is_past {
        Color::new(rf * 0.27, gf * 0.27, bf * 0.27, 1.0) // very dim
    } else {
        Color::new(rf * 0.72, gf * 0.72, bf * 0.72, 1.0) // dimmed upcoming
    }
}

/// Whether `note` lands on a physical key — a drum pad for channel 10, or the
/// octave-shifted melodic keyboard otherwise. Mirrors the logic in main.rs's
/// rebuild_all_notes_cache, kept in sync so the staff and keyboard agree.
fn note_fits(
    note: &Note,
    keyboard_notes: &HashSet<u8>,
    drum_note_to_key: &HashMap<u8, KeyId>,
    octave_offset: i8,
) -> bool {
    if note.channel == DRUM_CHANNEL {
        drum_note_to_key.contains_key(&note.midi_note)
    } else {
        let shifted = (note.midi_note as i16 + octave_offset as i16).clamp(0, 127) as u8;
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

fn draw_bass_clef(frame: &mut Frame, f_line_y: f32, color: Color) {
    let stroke = canvas::Stroke::default()
        .with_color(color)
        .with_width(3.0)
        .with_line_cap(canvas::LineCap::Round)
        .with_line_join(canvas::LineJoin::Round);

    let curve = Path::new(|b| {
        b.move_to(Point::new(17.0, f_line_y - 1.0));
        b.bezier_curve_to(
            Point::new(20.0, f_line_y - 17.0),
            Point::new(42.0, f_line_y - 17.0),
            Point::new(43.0, f_line_y - 2.0),
        );
        b.bezier_curve_to(
            Point::new(44.0, f_line_y + 13.0),
            Point::new(31.0, f_line_y + 23.0),
            Point::new(18.0, f_line_y + 25.0),
        );
    });
    frame.stroke(&curve, stroke);
    frame.fill(&Path::circle(Point::new(17.5, f_line_y - 1.0), 4.5), color);
    frame.fill(&Path::circle(Point::new(49.0, f_line_y - 6.0), 2.6), color);
    frame.fill(&Path::circle(Point::new(49.0, f_line_y + 6.0), 2.6), color);
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
    selection: Option<(u64, u64)>,
    keyboard_notes: &HashSet<u8>,
    drum_note_to_key: &HashMap<u8, KeyId>,
) {
    let w = size.width;
    let h = size.height;

    frame.fill(
        &Path::rectangle(Point::ORIGIN, size),
        Color::from_rgb8(0x09, 0x09, 0x15),
    );

    let Some(f) = midi else {
        frame.fill_text(Text {
            content: "Load a MIDI file to see staff notation".to_string(),
            position: Point::new(w / 2.0, h / 2.0),
            color: Color::from_rgb8(0x76, 0x62, 0x7E),
            size: iced::Pixels(15.0),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });
        return;
    };

    let tpb = f.ticks_per_beat as f32;
    let ppb = (w - CLEF_WIDTH) / (BEHIND_BEATS + AHEAD_BEATS); // pixels per beat
    let ppt = ppb / tpb;                                         // pixels per tick
    let playhead_x = CLEF_WIDTH + BEHIND_BEATS * ppb;
    let ref_y = h * 0.5; // middle C (slot 0) sits at vertical centre

    let tick_x  = |t: u64|  -> f32 { playhead_x + (t as f64 - pos as f64) as f32 * ppt };
    let slot_y  = |s: i32|  -> f32 { ref_y - s as f32 * HALF_SPACE };

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
    let line_col = Color::from_rgb8(0x4A, 0x35, 0x54);
    for &s in &[2i32, 4, 6, 8, 10, -2i32, -4, -6, -8, -10] {
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
        let bar_col   = Color::from_rgb8(0x2B, 0x20, 0x38);
        let mut bt    = first;
        while bt <= vis_end {
            let x = tick_x(bt);
            if x >= CLEF_WIDTH {
                frame.stroke(
                    &Path::line(Point::new(x, slot_y(12)), Point::new(x, slot_y(-12))),
                    canvas::Stroke::default().with_color(bar_col).with_width(1.0),
                );
            }
            match bt.checked_add(ticks_per_bar) {
                Some(v) => bt = v,
                None    => break,
            }
        }
    }

    // ── Clef symbols ────────────────────────────────────────────────────────
    let clef_col = Color::from_rgb8(0xED, 0xC8, 0x9C);
    draw_treble_clef(frame, slot_y(4), clef_col);
    draw_bass_clef(frame, slot_y(-4), clef_col);

    // ── Notes ───────────────────────────────────────────────────────────────
    let vis_start = pos.saturating_sub(((BEHIND_BEATS + 1.0) * tpb) as u64);
    let vis_end   = pos + ((AHEAD_BEATS + 1.0) * tpb) as u64;
    let note_r    = HALF_SPACE * 0.82; // note-head radius

    for note in &f.notes {
        if note.start_tick > vis_end || note.end_tick < vis_start { continue; }
        if track_muted.get(note.track).copied().unwrap_or(false)  { continue; }

        let shifted = (note.midi_note as i16 + octave_offset as i16).clamp(0, 127) as u8;
        let slot = staff_slot(shifted);
        let x    = tick_x(note.start_tick);
        let y    = slot_y(slot);

        if x < CLEF_WIDTH - 24.0 || x > w + 16.0 { continue; }

        let is_active = note.start_tick <= pos && pos < note.end_tick;
        let is_past   = note.end_tick   <= pos;

        let fits = note_fits(note, keyboard_notes, drum_note_to_key, octave_offset);
        let note_col = track_note_color(note.track, is_active, is_past);

        // Duration bar — a thin semi-transparent strip showing note length
        let end_x = tick_x(note.end_tick).min(w);
        if end_x > x + note_r {
            let alpha = if is_active { 0.42f32 } else if is_past { 0.16 } else { 0.18 };
            frame.stroke(
                &Path::line(Point::new(x, y), Point::new(end_x, y)),
                canvas::Stroke::default()
                    .with_color(Color { a: alpha, ..note_col })
                    .with_width(note_r * 1.7),
            );
        }

        // Ledger lines
        draw_ledgers(frame, slot, x, note_r, &slot_y, line_col);

        // Note head (filled circle)
        frame.fill(&Path::circle(Point::new(x, y), note_r), note_col);

        // Out-of-range warning ring — always drawn at full strength, independent
        // of the active/past dimming above, so it never fades into the background.
        if !fits {
            frame.stroke(
                &Path::circle(Point::new(x, y), note_r + 3.5),
                canvas::Stroke::default()
                    .with_color(Color::from_rgba8(0xFF, 0x40, 0x30, 0.95))
                    .with_width(2.2),
            );
        }

        // Stem — up when below the middle line, down when above
        let middle = if slot >= 0 { 6 } else { -6 }; // B4 (treble) or D3 (bass)
        let stem_up = slot < middle;
        let sx = x + if stem_up { note_r * 0.85 } else { -note_r * 0.85 };
        let sy = y + if stem_up { -3.5 * LINE_SPACING } else { 3.5 * LINE_SPACING };
        frame.stroke(
            &Path::line(Point::new(sx, y), Point::new(sx, sy)),
            canvas::Stroke::default().with_color(note_col).with_width(1.5),
        );

        // Sharp accidental
        if is_sharp(shifted) && !is_past {
            frame.fill_text(Text {
                content: "#".to_string(),
                position: Point::new(x - note_r * 2.8, y),
                color: note_col,
                size: iced::Pixels(11.0),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: Vertical::Center,
                ..Text::default()
            });
        }
    }

    // ── Playhead ─────────────────────────────────────────────────────────────
    frame.stroke(
        &Path::line(Point::new(playhead_x, slot_y(13)), Point::new(playhead_x, slot_y(-13))),
        canvas::Stroke::default()
            .with_color(Color::from_rgba8(0xFF, 0x4F, 0x87, 0.92))
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

    if slot >= 0 {
        // Treble zone ──────────────────────────────────────────────────────
        // Middle C (slot 0): one ledger below the treble bottom line (E4, slot 2).
        if slot == 0 {
            draw_at(frame, 0);
        }
        // Above treble top (F5, slot 10): ledgers at 12, 14, … up to even(slot).
        // A note in the space just above the staff (slot 11) gets no ledger;
        // a note on or above the first ledger line (slot ≥ 12) does.
        if slot > 10 {
            let top = if slot % 2 == 0 { slot } else { slot - 1 };
            let mut s = 12i32;
            while s <= top { draw_at(frame, s); s += 2; }
        }
    } else {
        // Bass zone ───────────────────────────────────────────────────────
        // Below bass bottom (G2, slot -10): ledgers at -12, -14, … down to even(slot).
        // A note in the space just below the staff (slot -11) gets no ledger;
        // a note on or below the first ledger line (slot ≤ -12) does.
        if slot < -10 {
            let bottom = if slot % 2 == 0 { slot } else { slot + 1 };
            let mut s = -12i32;
            while s >= bottom { draw_at(frame, s); s -= 2; }
        }
    }
}
