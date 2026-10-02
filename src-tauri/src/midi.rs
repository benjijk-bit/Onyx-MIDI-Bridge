use crate::mapping::MidiKind;
use midir::{MidiInput, MidiOutput};

pub fn list_input_ports() -> anyhow::Result<Vec<String>> {
    let midi_in = MidiInput::new("onyx-midi-bridge-list-in")?;
    let mut names = Vec::new();
    for port in midi_in.ports() {
        names.push(midi_in.port_name(&port).unwrap_or_else(|_| "Unknown".into()));
    }
    Ok(names)
}

pub fn list_output_ports() -> anyhow::Result<Vec<String>> {
    let midi_out = MidiOutput::new("onyx-midi-bridge-list-out")?;
    let mut names = Vec::new();
    for port in midi_out.ports() {
        names.push(midi_out.port_name(&port).unwrap_or_else(|_| "Unknown".into()));
    }
    Ok(names)
}

/// A decoded raw MIDI message, channel-voice messages only (the ones we care about).
#[derive(Debug, Clone, Copy)]
pub struct ParsedMidi {
    pub kind: MidiKind,
    pub channel: u8,
    pub number: u8,
    pub value: u8,
}

/// Parses the first 3 bytes of a channel voice MIDI message.
/// Returns None for anything else (system messages, running status, etc. — v1 keeps this simple).
pub fn parse_channel_voice(bytes: &[u8]) -> Option<ParsedMidi> {
    if bytes.len() < 3 {
        return None;
    }
    let status = bytes[0];
    let channel = status & 0x0F;
    let kind = match status & 0xF0 {
        0x90 if bytes[2] > 0 => MidiKind::NoteOn,
        0x90 => MidiKind::NoteOff, // note-on with velocity 0 is a note-off, per spec
        0x80 => MidiKind::NoteOff,
        0xB0 => MidiKind::ControlChange,
        _ => return None,
    };
    Some(ParsedMidi {
        kind,
        channel,
        number: bytes[1],
        value: bytes[2],
    })
}

pub fn cc_message(channel: u8, number: u8, value: u8) -> Vec<u8> {
    vec![0xB0 | (channel & 0x0F), number & 0x7F, value & 0x7F]
}

pub fn note_on_message(channel: u8, number: u8, velocity: u8) -> Vec<u8> {
    vec![0x90 | (channel & 0x0F), number & 0x7F, velocity & 0x7F]
}
