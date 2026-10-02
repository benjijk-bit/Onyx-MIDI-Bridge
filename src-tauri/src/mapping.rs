use serde::{Deserialize, Serialize};

/// The kind of raw MIDI message a mapping is triggered by.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MidiKind {
    ControlChange,
    NoteOn,
    NoteOff,
}

/// A specific MIDI control identity: kind + channel + controller/note number.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MidiTarget {
    pub kind: MidiKind,
    pub channel: u8, // 0-15
    pub number: u8,  // CC number or note number, 0-127
}

/// What kind of OSC target this maps to, which decides how the raw
/// 0-127 MIDI value is scaled.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    /// Continuous fader, scaled 0-127 -> 0.0-255.0 (Onyx's float fader range).
    Fader,
    /// Momentary/toggle button, MIDI value > 0 treated as "down" (1), 0 as "up" (0).
    Button,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    MidiToOsc,
    OscToMidi,
    Bidirectional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingEntry {
    pub id: String,
    pub label: String,
    pub midi: MidiTarget,
    pub osc_address: String,
    /// Only used when direction includes OscToMidi feedback.
    pub feedback_address: Option<String>,
    pub value_kind: ValueKind,
    pub direction: Direction,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MappingProfile {
    pub name: String,
    pub onyx_ip: String,
    pub onyx_send_port: u16,
    pub onyx_listen_port: u16,
    pub entries: Vec<MappingEntry>,
}

/// Convert a raw 0-127 MIDI value into the float Onyx expects for a fader (0.0-255.0).
pub fn midi_to_onyx_fader(raw: u8) -> f32 {
    (raw as f32 / 127.0) * 255.0
}

/// Convert an Onyx fader float (0.0-255.0) back into a 0-127 MIDI value.
pub fn onyx_fader_to_midi(value: f32) -> u8 {
    let clamped = value.clamp(0.0, 255.0);
    ((clamped / 255.0) * 127.0).round() as u8
}

/// Convert a raw MIDI value into an Onyx button int (0 or 1).
pub fn midi_to_onyx_button(raw: u8) -> i32 {
    if raw > 0 {
        1
    } else {
        0
    }
}

/// Convert an Onyx button int (0/1) into a MIDI value (0 or 127) for LED feedback.
pub fn onyx_button_to_midi(value: i32) -> u8 {
    if value != 0 {
        127
    } else {
        0
    }
}
