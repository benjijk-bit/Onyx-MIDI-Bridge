import { invoke } from "./vendor/api/core.js";
import { listen } from "./vendor/api/event.js";
import { open, save } from "./vendor/plugin-dialog/index.js";

let midiInConnected = false;
let midiOutConnected = false;
let allTargets = [];
let targetsByAddress = new Map();
let learnedMidi = null;
let learnUnlisten = null;
// Display text Onyx pushes over OSC (playback names, bank number), keyed by address.
const onyxTexts = new Map();
// "Playback N" category -> the address Onyx sends that slot's playback name to.
let playbackNameAddress = new Map();
let targetsRenderQueued = false;
const BANK_LABEL_ADDRESS = "/Mx/label/4401/text";

const el = (id) => document.getElementById(id);

function showError(msg) {
  const toast = el("toast");
  toast.textContent = msg;
  toast.style.display = "block";
  clearTimeout(showError._t);
  showError._t = setTimeout(() => {
    toast.style.display = "none";
  }, 5000);
}

function setStatus(id, state, text) {
  const node = el(id);
  node.className = "status" + (state ? " " + state : "");
  node.innerHTML = '<span class="dot"></span>' + text;
}

function escapeHtml(s) {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}

// ---------- Devices ----------

function fillSelect(select, items) {
  const prev = select.value;
  select.innerHTML = "";
  for (const name of items) {
    const opt = document.createElement("option");
    opt.value = name;
    opt.textContent = name;
    select.appendChild(opt);
  }
  if (items.includes(prev)) select.value = prev;
}

async function refreshDevices() {
  try {
    const [inputs, outputs] = await Promise.all([
      invoke("list_midi_inputs"),
      invoke("list_midi_outputs"),
    ]);
    fillSelect(el("midi-in-select"), inputs);
    fillSelect(el("midi-out-select"), outputs);
  } catch (e) {
    showError("Failed to list MIDI devices: " + e);
  }
}

async function connectMidiIn() {
  const btn = el("connect-midi-in");
  const portName = el("midi-in-select").value;
  if (!portName) {
    showError("No MIDI input selected.");
    return;
  }
  btn.disabled = true;
  try {
    await invoke("connect_midi_input", { portName });
    midiInConnected = true;
    setStatus("midi-in-status", "attempted", "Connected: " + escapeHtml(portName));
  } catch (e) {
    setStatus("midi-in-status", "failed", "Failed: " + escapeHtml(String(e)));
    btn.disabled = false;
  }
}

async function connectMidiOut() {
  const btn = el("connect-midi-out");
  const portName = el("midi-out-select").value;
  if (!portName) {
    showError("No MIDI output selected.");
    return;
  }
  btn.disabled = true;
  try {
    await invoke("connect_midi_output", { portName });
    midiOutConnected = true;
    setStatus("midi-out-status", "attempted", "Connected: " + escapeHtml(portName));
    updateDirectionOptions();
  } catch (e) {
    setStatus("midi-out-status", "failed", "Failed: " + escapeHtml(String(e)));
    btn.disabled = false;
  }
}

async function connectOnyx() {
  const ip = el("onyx-ip").value.trim();
  const sendPort = Number(el("onyx-send-port").value);
  const listenPort = Number(el("onyx-listen-port").value);
  if (!ip || !sendPort || !listenPort) {
    showError("Fill in the Onyx IP and both ports.");
    return;
  }
  try {
    await invoke("connect_onyx", { ip, sendPort, listenPort });
    setStatus(
      "onyx-status",
      "attempted",
      `Connected: ${escapeHtml(ip)}:${sendPort} (listen ${listenPort})`,
    );
    el("reconnect-hint").classList.remove("visible");
  } catch (e) {
    setStatus("onyx-status", "failed", "Failed: " + escapeHtml(String(e)));
  }
}

function updateDirectionOptions() {
  const select = el("direction-select");
  for (const opt of select.options) {
    if (opt.value === "osc_to_midi" || opt.value === "bidirectional") {
      opt.disabled = !midiOutConnected;
    }
  }
  if (select.selectedOptions[0]?.disabled) {
    select.value = "midi_to_osc";
  }
}

// ---------- Onyx targets / mapping creation ----------

async function refreshTargets() {
  try {
    const data = await invoke("get_onyx_targets");
    allTargets = (data.targets || []).filter((t) => t.kind !== "label");
    targetsByAddress = new Map(allTargets.map((t) => [t.address, t]));
    // Onyx sends a playback slot's name to the button one above its fader (fader 4203 -> 4204).
    playbackNameAddress = new Map();
    for (const t of allTargets) {
      const m = t.category.startsWith("Playback ") && t.address.match(/^\/Mx\/fader\/(\d+)$/);
      if (m) playbackNameAddress.set(t.category, `/Mx/button/${Number(m[1]) + 1}/text`);
    }
    renderTargetOptions("");
  } catch (e) {
    showError("Failed to load Onyx target list: " + e);
  }
}

function playbackName(category) {
  return onyxTexts.get(playbackNameAddress.get(category)) || "";
}

function onOnyxText({ address, text }) {
  onyxTexts.set(address, text);
  if (address === BANK_LABEL_ADDRESS) {
    el("onyx-bank").textContent = `Onyx bank: ${text}`;
  }
  // Onyx sends names in bursts (all 20 slots on a bank change); redraw once per frame.
  if (!targetsRenderQueued) {
    targetsRenderQueued = true;
    requestAnimationFrame(() => {
      targetsRenderQueued = false;
      renderTargetOptions(el("target-filter").value);
    });
  }
}

function renderTargetOptions(filterText) {
  const select = el("target-select");
  const prev = select.value;
  select.innerHTML = "";
  const needle = filterText.trim().toLowerCase();
  const byCategory = new Map();
  for (const t of allTargets) {
    if (
      needle &&
      !t.name.toLowerCase().includes(needle) &&
      !t.category.toLowerCase().includes(needle) &&
      !playbackName(t.category).toLowerCase().includes(needle)
    ) {
      continue;
    }
    if (!byCategory.has(t.category)) byCategory.set(t.category, []);
    byCategory.get(t.category).push(t);
  }
  for (const [category, items] of byCategory) {
    const group = document.createElement("optgroup");
    const name = playbackName(category);
    group.label = name ? `${category} — ${name}` : category;
    for (const t of items) {
      const opt = document.createElement("option");
      opt.value = t.address;
      opt.textContent = `${t.name} (${t.kind})`;
      group.appendChild(opt);
    }
    select.appendChild(group);
  }
  if (targetsByAddress.has(prev)) select.value = prev;
  updateAddButtonState();
}

function selectedTarget() {
  return targetsByAddress.get(el("target-select").value) || null;
}

function updateAddButtonState() {
  el("add-mapping-btn").disabled = !(selectedTarget() && learnedMidi);
}

// ---------- MIDI Learn ----------

const KIND_LABELS = { control_change: "CC", note_on: "Note", note_off: "Note" };
const STATUS_NAMES = {
  0x80: "Note Off",
  0x90: "Note On",
  0xa0: "Poly Aftertouch",
  0xb0: "CC",
  0xc0: "Program Change",
  0xd0: "Channel Pressure",
  0xe0: "Pitch Bend",
};

// Channels are stored 0-15 but shown 1-16, matching how controllers label them.
function describeLearned(m) {
  return `${KIND_LABELS[m.kind] || m.kind} ${m.number} · ch ${m.channel + 1}`;
}

function describeRawMidi(bytes) {
  const hex = bytes.map((b) => b.toString(16).padStart(2, "0").toUpperCase()).join(" ");
  const type = bytes[0] & 0xf0;
  if (type === 0xf0) return `${hex} — system message (can't be mapped)`;
  const mappable = type === 0x80 || type === 0x90 || type === 0xb0;
  const number = bytes.length >= 2 ? ` ${bytes[1]}` : "";
  return (
    `${hex} — ${STATUS_NAMES[type] || "Unknown"}${number} · ch ${(bytes[0] & 0x0f) + 1}` +
    (mappable ? "" : " (can't be mapped)")
  );
}

async function startLearn() {
  if (!midiInConnected) {
    showError("Connect a MIDI input first.");
    return;
  }
  const learnBtn = el("learn-btn");
  const cancelBtn = el("cancel-learn-btn");
  const panel = el("learn-panel");
  learnBtn.disabled = true;
  cancelBtn.disabled = false;
  panel.classList.add("armed");
  el("learn-status").textContent = "Move a control now…";
  el("learn-readout").textContent = "";

  try {
    // Register the listener before arming learn mode so we can't miss the event.
    learnUnlisten = await listen("midi-learn-result", (event) => {
      learnedMidi = event.payload;
      el("learn-readout").textContent = describeLearned(learnedMidi);
      finishLearn();
    });
    await invoke("start_midi_learn");
  } catch (e) {
    showError("Failed to start MIDI Learn: " + e);
    finishLearn();
  }
}

function finishLearn() {
  if (learnUnlisten) {
    learnUnlisten();
    learnUnlisten = null;
  }
  el("learn-btn").disabled = false;
  el("cancel-learn-btn").disabled = true;
  el("learn-panel").classList.remove("armed");
  el("learn-status").textContent = "Pick a target, then click Learn and move the control.";
  updateAddButtonState();
}

async function cancelLearn() {
  try {
    await invoke("cancel_midi_learn");
  } catch {
    // Nothing meaningful to show the user if disarming fails; the panel resets regardless.
  }
  finishLearn();
}

// ---------- Mappings ----------

async function addMapping() {
  const target = selectedTarget();
  if (!target || !learnedMidi) return;
  const entry = {
    id: crypto.randomUUID(),
    label:
      el("label-input").value.trim() ||
      (playbackName(target.category)
        ? `${target.name} (${playbackName(target.category)})`
        : target.name),
    midi: {
      kind: learnedMidi.kind,
      channel: learnedMidi.channel,
      number: learnedMidi.number,
    },
    osc_address: target.address,
    feedback_address: target.feedback_address || null,
    value_kind: target.kind,
    direction: el("direction-select").value,
  };
  try {
    await invoke("add_mapping", { entry });
    learnedMidi = null;
    el("learn-readout").textContent = "";
    el("label-input").value = "";
    updateAddButtonState();
    await refreshMappings();
  } catch (e) {
    showError("Failed to add mapping: " + e);
  }
}

function directionLabel(d) {
  return (
    { midi_to_osc: "MIDI → Onyx", osc_to_midi: "Onyx → MIDI", bidirectional: "Bidirectional" }[
      d
    ] || d
  );
}

async function refreshMappings() {
  try {
    const mappings = await invoke("get_mappings");
    const body = el("mappings-body");
    body.innerHTML = "";
    if (mappings.length === 0) {
      body.innerHTML = '<tr class="empty-row"><td colspan="5">No mappings yet.</td></tr>';
      return;
    }
    for (const m of mappings) {
      const tr = document.createElement("tr");
      tr.innerHTML = `
        <td>${escapeHtml(m.label)}</td>
        <td class="mono">${escapeHtml(describeLearned(m.midi))}</td>
        <td class="mono">${escapeHtml(m.osc_address)}</td>
        <td>${escapeHtml(directionLabel(m.direction))}</td>
        <td><button class="danger" data-id="${escapeHtml(m.id)}">Remove</button></td>
      `;
      body.appendChild(tr);
    }
    body.querySelectorAll("button[data-id]").forEach((btn) => {
      btn.addEventListener("click", () => removeMapping(btn.dataset.id));
    });
  } catch (e) {
    showError("Failed to load mappings: " + e);
  }
}

async function removeMapping(id) {
  try {
    await invoke("remove_mapping", { id });
    await refreshMappings();
  } catch (e) {
    showError("Failed to remove mapping: " + e);
  }
}

// ---------- Profiles ----------

async function saveProfile() {
  try {
    const path = await save({
      defaultPath: "onyx-profile.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!path) return;
    const name = path
      .split(/[\\/]/)
      .pop()
      .replace(/\.json$/i, "");
    await invoke("save_profile", { path, name });
  } catch (e) {
    showError("Failed to save profile: " + e);
  }
}

async function loadProfile() {
  try {
    const path = await open({
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!path) return;
    const profile = await invoke("load_profile", { path });
    el("onyx-ip").value = profile.onyx_ip;
    el("onyx-send-port").value = profile.onyx_send_port;
    el("onyx-listen-port").value = profile.onyx_listen_port;
    // Intentionally not auto-reconnecting: silently opening a UDP socket to a
    // freshly loaded address is a surprising side effect for a lighting console
    // bridge. The user confirms by pressing Connect themselves.
    el("reconnect-hint").classList.add("visible");
    await refreshMappings();
  } catch (e) {
    showError("Failed to load profile: " + e);
  }
}

// ---------- Wiring ----------

function wireUp() {
  el("refresh-devices").addEventListener("click", refreshDevices);
  el("connect-midi-in").addEventListener("click", connectMidiIn);
  el("connect-midi-out").addEventListener("click", connectMidiOut);
  el("connect-onyx").addEventListener("click", connectOnyx);
  el("target-filter").addEventListener("input", (e) => renderTargetOptions(e.target.value));
  el("target-select").addEventListener("change", updateAddButtonState);
  el("learn-btn").addEventListener("click", startLearn);
  el("cancel-learn-btn").addEventListener("click", cancelLearn);
  el("add-mapping-btn").addEventListener("click", addMapping);
  el("save-profile-btn").addEventListener("click", saveProfile);
  el("load-profile-btn").addEventListener("click", loadProfile);
}

async function init() {
  wireUp();
  updateDirectionOptions();
  try {
    await listen("midi-activity", (event) => {
      el("midi-activity").textContent = "Last MIDI in: " + describeRawMidi(event.payload);
    });
    await listen("onyx-text", (event) => onOnyxText(event.payload));
    let oscReceived = 0;
    await listen("osc-activity", ({ payload }) => {
      oscReceived += 1;
      el("osc-activity").textContent =
        `Last OSC in (${oscReceived} total): ${payload.address} ${payload.value}`;
    });
  } catch (e) {
    showError("Failed to start event listeners: " + e);
  }
  await refreshDevices();
  await refreshTargets();
  await refreshMappings();
}

document.addEventListener("DOMContentLoaded", init);
