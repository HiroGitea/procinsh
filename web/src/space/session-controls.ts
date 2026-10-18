// Browser acquisition and transport controls; both loaders use the same parser.
import {
  parseRecording,
  SpaceRecorder,
  SpaceReplay,
  MAX_RECORDING_BYTES,
} from "./recording.js";
import type { RecordingEvent } from "./recording.js";
import type { SystemSnapshot } from "../shared/api-types.js";

// IDs are shared by the picker and the URL allowlist.
const bundledRecordings = [
  { id: "demo", label: "Sanitized demo", path: "recordings/demo.json" },
];

function demoControls(open: (id: string) => void) {
  const controls = document.createElement("div");
  controls.id = "demo-controls";
  controls.className = "demo-controls";
  const select = document.createElement("select");
  select.id = "demo-recording";
  select.setAttribute("aria-label", "Demo recording");
  for (const recording of bundledRecordings) {
    const option = document.createElement("option");
    option.value = recording.id;
    option.textContent = recording.label;
    select.append(option);
  }
  const button = document.createElement("button");
  button.id = "open-demo";
  button.type = "button";
  button.textContent = "Open demo";
  button.onclick = () => open(select.value);
  controls.append(select, button);
  return controls;
}

export function mountRecordingControls(
  recorder: SpaceRecorder,
  snapshot: () => SystemSnapshot,
  ready: () => boolean,
) {
  const host = document.querySelector(".tools")!;
  const record = document.createElement("button");
  record.textContent = "Record";
  record.id = "record";
  const download = document.createElement("button");
  download.id = "download-recording";
  download.textContent = "Stop & Download";
  download.hidden = true;
  const status = document.createElement("span");
  status.setAttribute("role", "status");
  status.id = "recording-status";
  const open = document.createElement("button");
  open.id = "open-recording";
  open.type = "button";
  open.onclick = () => {
    location.assign(`${import.meta.env.BASE_URL}replay/space/`);
  };
  open.textContent = "Open recording";
  host.append(
    record,
    download,
    status,
    open,
    demoControls((id) => {
      location.assign(
        `${import.meta.env.BASE_URL}replay/space/?recording=${encodeURIComponent(id)}`,
      );
    }),
  );
  function refresh() {
    record.disabled = recorder.active || !!recorder.recording || !ready();
    download.hidden = !recorder.recording;
    download.textContent = recorder.active
      ? "Stop & Download"
      : "Download recording";
    if (recorder.recording)
      status.textContent = recorder.active
        ? `● REC ${(recorder.elapsed() / 1000).toFixed(1)}s`
        : `${recorder.recording.end_reason} (${(recorder.elapsed() / 1000).toFixed(1)}s saved)`;
  }
  record.onclick = () => {
    try {
      recorder.start(snapshot());
    } catch (error) {
      status.textContent = String(error);
    }
    refresh();
  };
  download.onclick = () => {
    recorder.stop();
    if (!recorder.recording) return;
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(recorder.recording)], {
        type: "application/json;charset=utf-8",
      }),
    );
    const link = document.createElement("a");
    link.href = url;
    link.download = `procinsh-space-${recorder.recording.started_at.replace(/[:.]/g, "-")}.json`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    recorder.recording = null;
    status.textContent = "Recording downloaded. Review before publishing.";
    refresh();
  };
  addEventListener("beforeunload", (event) => {
    if (recorder.recording) {
      event.preventDefault();
      event.returnValue = "";
    }
  });
  setInterval(refresh, 100);
  refresh();
}

export function mountReplayControls(
  reset: (snapshot: SystemSnapshot) => void,
  apply: (event: RecordingEvent) => void,
  clock: (time: number) => void,
) {
  const host = document.querySelector(".tools")!;
  const file = document.createElement("input");
  file.type = "file";
  file.hidden = true;
  file.accept = ".json,application/json";
  file.setAttribute("aria-label", "Open recording JSON");
  const open = document.createElement("button");
  open.id = "open-recording";
  open.type = "button";
  open.textContent = "Open recording";
  open.onclick = () => file.click();
  const play = document.createElement("button");
  play.id = "play";
  const timeline = document.createElement("input");
  timeline.id = "timeline";
  timeline.type = "range";
  timeline.min = "0";
  timeline.max = "1";
  timeline.step = "any";
  timeline.setAttribute("aria-label", "Replay time");
  const speed = document.createElement("select");
  speed.setAttribute("aria-label", "Playback speed");
  for (const rate of [0.1, 0.25, 0.5, 1, 2]) {
    const option = document.createElement("option");
    option.value = String(rate);
    option.textContent = `${rate}×`;
    option.selected = rate === 1;
    speed.append(option);
  }
  const loop = document.createElement("input");
  loop.type = "checkbox";
  loop.id = "loop";
  loop.checked = true;
  const loopLabel = document.createElement("label");
  loopLabel.append(loop, " Loop");
  const status = document.createElement("span");
  status.id = "replay-status";
  status.setAttribute("role", "status");
  const options = document.createElement("div");
  options.className = "replay-controls-row";
  options.append(speed, loopLabel);
  host.append(open, file, play, timeline, options, status);
  let replay: SpaceReplay | null = null;
  let loading = 0;
  let label = "";
  let previousWallTime = performance.now();
  function refresh() {
    for (const element of [play, timeline, speed, loop])
      element.disabled = !replay;
    play.textContent = replay?.playing ? "Pause" : "Play";
    timeline.disabled = !replay || replay.duration === 0;
    timeline.value = String(
      replay?.duration ? replay.time / replay.duration : 0,
    );
    timeline.setAttribute(
      "aria-valuetext",
      replay
        ? `${(replay.time / 1000).toFixed(1)} / ${(replay.duration / 1000).toFixed(1)} seconds`
        : "",
    );
    status.textContent = replay
      ? `${label} · ${(replay.time / 1000).toFixed(1)} / ${(replay.duration / 1000).toFixed(1)}s`
      : label;
  }
  async function load(read: () => Promise<string>, name: string) {
    const request = ++loading;
    if (replay) replay.playing = false;
    label = "Loading…";
    refresh();
    try {
      const text = await read();
      if (request !== loading) return;
      const recording = parseRecording(text);
      replay = new SpaceReplay(recording, reset, apply, clock);
      replay.speed = Number(speed.value);
      loop.checked = true;
      replay.loop = true;
      replay.playing = replay.duration > 0 && !document.hidden;
      previousWallTime = performance.now();
      label = name;
    } catch (error) {
      if (request === loading) {
        replay = null;
        label = error instanceof Error ? error.message : String(error);
      }
    }
    refresh();
  }
  function local(recording: File | undefined) {
    if (!recording) return;
    void load(() => {
      if (recording.size > MAX_RECORDING_BYTES)
        throw new Error("Recording exceeds the 128 MiB local replay limit.");
      return recording.text();
    }, recording.name);
  }
  file.onchange = () => {
    local(file.files?.[0]);
    file.value = "";
  };
  document.addEventListener("dragover", (event) => {
    event.preventDefault();
  });
  document.addEventListener("drop", (event) => {
    event.preventDefault();
    local(event.dataTransfer?.files[0]);
  });
  play.onclick = () => {
    if (!replay) return;
    if (!replay.playing && replay.time === replay.duration) replay.seek(0);
    replay.playing = !replay.playing;
    previousWallTime = performance.now();
    refresh();
  };
  timeline.oninput = () => {
    if (replay) {
      replay.playing = false;
      replay.seek(Number(timeline.value) * replay.duration);
    }
    refresh();
  };
  speed.onchange = () => {
    if (replay) replay.speed = Number(speed.value);
  };
  loop.onchange = () => {
    if (replay) replay.loop = loop.checked;
  };
  document.addEventListener("visibilitychange", () => {
    if (document.hidden && replay) replay.playing = false;
    refresh();
  });
  function openDemo(id: string) {
    const recording = bundledRecordings.find((entry) => entry.id === id);
    if (!recording) {
      label = `Unknown bundled recording: ${id}. Choose a local JSON file.`;
      refresh();
      return;
    }
    demo.querySelector("select")!.value = id;
    void load(async () => {
      const response = await fetch(
        `${import.meta.env.BASE_URL}${recording.path}`,
      );
      if (!response.ok)
        throw new Error(
          `Unable to load bundled recording (${response.status}).`,
        );
      return response.text();
    }, recording.label);
  }
  const demo = demoControls((id) => {
    const url = new URL(location.href);
    url.searchParams.set("recording", id);
    history.replaceState(null, "", url);
    openDemo(id);
  });
  host.append(demo);
  const id = new URL(location.href).searchParams.get("recording");
  if (id) openDemo(id);
  refresh();
  return () => {
    const wallTime = performance.now();
    replay?.tick(wallTime - previousWallTime);
    previousWallTime = wallTime;
    refresh();
  };
}
