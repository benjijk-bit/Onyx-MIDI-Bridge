mod mapping;
mod midi;
mod osc;

use mapping::{Direction, MappingEntry, MappingProfile, MidiKind, ValueKind};
use midir::{MidiInput, MidiOutput, MidiOutputConnection};
use osc::{IncomingOsc, OscOut};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

/// Bundled at compile time so the app doesn't need to locate the file at runtime.
/// Re-run scripts/generate_onyx_map.py and rebuild if the target list changes.
const ONYX_MAP_JSON: &str = include_str!("../../data/onyx_osc_map.json");

pub struct AppStateInner {
    midi_out_conn: Mutex<Option<MidiOutputConnection>>,
    osc_out: Mutex<Option<OscOut>>,
    mappings: Mutex<Vec<MappingEntry>>,
    learn_mode: AtomicBool,
    onyx_conn_info: Mutex<Option<(String, u16, u16)>>, // ip, send_port, listen_port
    app_handle: tauri::AppHandle,
}

type SharedState = Arc<AppStateInner>;

// ---------- MIDI -> OSC ----------

fn handle_incoming_midi(state: &SharedState, parsed: midi::ParsedMidi) {
    if state.learn_mode.swap(false, Ordering::SeqCst) {
        let _ = state.app_handle.emit(
            "midi-learn-result",
            serde_json::json!({
                "kind": match parsed.kind {
                    MidiKind::ControlChange => "control_change",
                    MidiKind::NoteOn => "note_on",
                    MidiKind::NoteOff => "note_off",
                },
                "channel": parsed.channel,
                "number": parsed.number,
            }),
        );
        return;
    }

    let mappings = state.mappings.lock().unwrap();
    for entry in mappings.iter() {
        let matches = entry.midi.channel == parsed.channel
            && entry.midi.number == parsed.number
            && midi_kind_matches(entry.midi.kind, parsed.kind);
        if !matches {
            continue;
        }
        if !matches!(entry.direction, Direction::MidiToOsc | Direction::Bidirectional) {
            continue;
        }
        let guard = state.osc_out.lock().unwrap();
        if let Some(out) = guard.as_ref() {
            match entry.value_kind {
                ValueKind::Fader => {
                    let value = mapping::midi_to_onyx_fader(parsed.value);
                    let _ = out.send_float(&entry.osc_address, value);
                }
                ValueKind::Button => {
                    let value = mapping::midi_to_onyx_button(parsed.value);
                    let _ = out.send_int(&entry.osc_address, value);
                }
            }
        }
    }
}

fn midi_kind_matches(mapped: MidiKind, incoming: MidiKind) -> bool {
    match mapped {
        MidiKind::ControlChange => matches!(incoming, MidiKind::ControlChange),
        // A mapping made from either Note On or Note Off should still trigger on either,
        // so a single "learn" on a button press covers both press and release.
        MidiKind::NoteOn | MidiKind::NoteOff => {
            matches!(incoming, MidiKind::NoteOn | MidiKind::NoteOff)
        }
    }
}

// ---------- OSC -> MIDI (feedback) ----------

fn handle_incoming_osc(state: &SharedState, incoming: IncomingOsc) {
    let mappings = state.mappings.lock().unwrap();
    for entry in mappings.iter() {
        let Some(fb_addr) = entry.feedback_address.as_ref() else {
            continue;
        };
        if fb_addr != &incoming.address {
            continue;
        }
        if !matches!(entry.direction, Direction::OscToMidi | Direction::Bidirectional) {
            continue;
        }
        let mut out_conn = state.midi_out_conn.lock().unwrap();
        let Some(conn) = out_conn.as_mut() else {
            continue;
        };
        // Onyx's docs and third-party integrations disagree on int vs float, so accept
        // either; a message with neither (e.g. a string) is skipped rather than sent as 0,
        // which would slam a motorized fader to the bottom.
        let bytes = match entry.value_kind {
            ValueKind::Fader => {
                let Some(value) = incoming
                    .float_value
                    .or(incoming.int_value.map(|i| i as f32))
                else {
                    continue;
                };
                let raw = mapping::onyx_fader_to_midi(value);
                midi::cc_message(entry.midi.channel, entry.midi.number, raw)
            }
            ValueKind::Button => {
                let Some(value) = incoming
                    .int_value
                    .or(incoming.float_value.map(|f| f.round() as i32))
                else {
                    continue;
                };
                let raw = mapping::onyx_button_to_midi(value);
                match entry.midi.kind {
                    MidiKind::ControlChange => {
                        midi::cc_message(entry.midi.channel, entry.midi.number, raw)
                    }
                    _ => midi::note_on_message(entry.midi.channel, entry.midi.number, raw),
                }
            }
        };
        let _ = conn.send(&bytes);
    }
}

// ---------- Commands ----------

#[tauri::command]
fn list_midi_inputs() -> Result<Vec<String>, String> {
    midi::list_input_ports().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_midi_outputs() -> Result<Vec<String>, String> {
    midi::list_output_ports().map_err(|e| e.to_string())
}

/// Connects the chosen MIDI input port and starts routing its messages
/// through the learn/mapping pipeline. The connection itself is intentionally
/// leaked into a background-owned Box so it stays alive for the process
/// lifetime — v1 does not support disconnect/reconnect cycling.
#[tauri::command]
fn connect_midi_input(port_name: String, state: tauri::State<SharedState>) -> Result<(), String> {
    let midi_in = MidiInput::new("onyx-midi-bridge-in").map_err(|e| e.to_string())?;
    let ports = midi_in.ports();
    let port = ports
        .iter()
        .find(|p| {
            midi_in
                .port_name(p)
                .map(|n| n == port_name)
                .unwrap_or(false)
        })
        .cloned()
        .ok_or_else(|| format!("MIDI input port not found: {port_name}"))?;

    let shared = state.inner().clone();
    let conn = midi_in
        .connect(
            &port,
            "onyx-midi-bridge-in-conn",
            move |_stamp, message, _| {
                if let Some(parsed) = midi::parse_channel_voice(message) {
                    handle_incoming_midi(&shared, parsed);
                }
            },
            (),
        )
        .map_err(|e| e.to_string())?;

    // Leak the connection so its callback + captured state stay alive.
    // Fine for a single-session control app; revisit if you add disconnect support.
    Box::leak(Box::new(conn));
    Ok(())
}

#[tauri::command]
fn connect_midi_output(port_name: String, state: tauri::State<SharedState>) -> Result<(), String> {
    let midi_out = MidiOutput::new("onyx-midi-bridge-out").map_err(|e| e.to_string())?;
    let ports = midi_out.ports();
    let port = ports
        .iter()
        .find(|p| {
            midi_out
                .port_name(p)
                .map(|n| n == port_name)
                .unwrap_or(false)
        })
        .cloned()
        .ok_or_else(|| format!("MIDI output port not found: {port_name}"))?;
    let conn = midi_out
        .connect(&port, "onyx-midi-bridge-out-conn")
        .map_err(|e| e.to_string())?;
    *state.midi_out_conn.lock().unwrap() = Some(conn);
    Ok(())
}

#[tauri::command]
fn connect_onyx(
    ip: String,
    send_port: u16,
    listen_port: u16,
    state: tauri::State<SharedState>,
) -> Result<(), String> {
    let out = OscOut::new(&ip, send_port).map_err(|e| e.to_string())?;
    *state.osc_out.lock().unwrap() = Some(out);
    *state.onyx_conn_info.lock().unwrap() = Some((ip, send_port, listen_port));

    let shared = state.inner().clone();
    osc::start_listener(listen_port, move |incoming| {
        handle_incoming_osc(&shared, incoming);
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn start_midi_learn(state: tauri::State<SharedState>) {
    state.learn_mode.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn cancel_midi_learn(state: tauri::State<SharedState>) {
    state.learn_mode.store(false, Ordering::SeqCst);
}

#[tauri::command]
fn add_mapping(entry: MappingEntry, state: tauri::State<SharedState>) {
    let mut mappings = state.mappings.lock().unwrap();
    mappings.retain(|e| e.id != entry.id);
    mappings.push(entry);
}

#[tauri::command]
fn remove_mapping(id: String, state: tauri::State<SharedState>) {
    let mut mappings = state.mappings.lock().unwrap();
    mappings.retain(|e| e.id != id);
}

#[tauri::command]
fn get_mappings(state: tauri::State<SharedState>) -> Vec<MappingEntry> {
    state.mappings.lock().unwrap().clone()
}

#[tauri::command]
fn get_onyx_targets() -> Result<Value, String> {
    serde_json::from_str(ONYX_MAP_JSON).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_profile(path: String, name: String, state: tauri::State<SharedState>) -> Result<(), String> {
    let entries = state.mappings.lock().unwrap().clone();
    let (ip, send_port, listen_port) = state
        .onyx_conn_info
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| ("127.0.0.1".to_string(), 8000, 9000));
    let profile = MappingProfile {
        name,
        onyx_ip: ip,
        onyx_send_port: send_port,
        onyx_listen_port: listen_port,
        entries,
    };
    let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_profile(path: String, state: tauri::State<SharedState>) -> Result<MappingProfile, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let profile: MappingProfile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    *state.mappings.lock().unwrap() = profile.entries.clone();
    Ok(profile)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state: SharedState = Arc::new(AppStateInner {
                midi_out_conn: Mutex::new(None),
                osc_out: Mutex::new(None),
                mappings: Mutex::new(Vec::new()),
                learn_mode: AtomicBool::new(false),
                onyx_conn_info: Mutex::new(None),
                app_handle: app.handle().clone(),
            });
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_midi_inputs,
            list_midi_outputs,
            connect_midi_input,
            connect_midi_output,
            connect_onyx,
            start_midi_learn,
            cancel_midi_learn,
            add_mapping,
            remove_mapping,
            get_mappings,
            get_onyx_targets,
            save_profile,
            load_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
