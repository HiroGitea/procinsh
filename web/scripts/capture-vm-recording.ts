// Use the existing browser recorder through QEMU hostfwd; no alternate JSON writer.
import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gzipSync } from "node:zlib";
import { connectPage, pages } from "../tests/browser/harness.js";
import { launch, until, delay } from "../tests/support/runtime.js";
import { parseRecording, MAX_RECORDING_BYTES } from "../src/space/recording.js";
import { PAGES_MAX_ASSET_BYTES } from "../vite.replay.config.js";

const browser = process.env.BROWSER ?? "chrome";
assert.ok(browser === "chrome" || browser === "firefox");
const browserName = browser === "firefox" ? "Firefox" : "Chrome";
function isBrowserProcess(process: {
  name: string;
  command_line?: string[] | null;
}) {
  return (
    process.name.toLowerCase().includes(browser) ||
    !!process.command_line?.[0]?.toLowerCase().includes(browser)
  );
}
const port = Number(process.env.HOST_PORT ?? 10090);
const seconds = Number(process.env.RECORD_SECONDS ?? 15);
assert.ok(Number.isInteger(port) && port > 0 && port <= 65535);
assert.ok(Number.isFinite(seconds) && seconds >= 1 && seconds <= 300);
const output = process.env.RECORD_OUTPUT ?? `/tmp/${browser}-vm.json.gz`;
const base = `http://127.0.0.1:${port}`;
const profile = await mkdtemp(join(tmpdir(), "space-vm-capture-"));
const errors: string[] = [];
const chrome = launch(process.env.CHROME || "/usr/bin/google-chrome", [
  "--headless=new",
  "--use-gl=angle",
  "--use-angle=swiftshader",
  "--enable-unsafe-swiftshader",
  "--disable-dev-shm-usage",
  "--remote-debugging-port=0",
  ...(process.getuid?.() === 0 ? ["--no-sandbox"] : []),
  `--user-data-dir=${profile}`,
  "about:blank",
]);
let socket: WebSocket | undefined;
try {
  const endpoint = await until(
    () =>
      chrome.output.match(
        /ws:\/\/127\.0\.0\.1:\d+\/devtools\/browser\/[\w-]+/,
      )?.[0],
    "Chrome debugger",
  );
  const page = (await pages(new URL(endpoint).port)).find(
    (p) => p.type === "page",
  )!;
  const connection = await connectPage(page.webSocketDebuggerUrl, errors);
  socket = connection.socket;
  const { cdp, evaluate, waitFor } = connection;
  await cdp("Runtime.enable");
  await cdp("Page.enable");
  await until(
    async () => {
      const response = await fetch(`${base}/api/processes`);
      assert.ok(
        response.ok,
        `VM process discovery failed (${response.status})`,
      );
      const processes = (await response.json()) as {
        name: string;
        command_line?: string[] | null;
      }[];
      return processes.filter(isBrowserProcess).length >= 4;
    },
    `${browserName} workload processes`,
    45000,
  );
  console.log(`Opening ${base}/space`);
  await cdp("Page.navigate", { url: `${base}/space` });
  await waitFor(
    "document.querySelector('#record')?.disabled===false",
    "VM recording baseline",
  );
  // Let the lazy live collector refresh a snapshot retained while it was idle.
  await delay(2000);
  const baseline = await cdp("Runtime.getHeapUsage");
  console.log(`Recording ${seconds} seconds of VM activity`);
  await evaluate(`window.recordingBlob=null;
    URL.createObjectURL=blob=>{window.recordingBlob=blob;return 'blob:recording'};
    HTMLAnchorElement.prototype.click=function(){};
    document.querySelector('#record').click()`);
  const recordingHeap = [];
  for (let elapsed = 0; elapsed < seconds; elapsed++) {
    await delay(1000);
    recordingHeap.push(await cdp("Runtime.getHeapUsage"));
    assert.match(
      await evaluate<string>(
        "document.querySelector('#recording-status').textContent",
      ),
      /REC/,
      "Recorder must remain continuous",
    );
  }
  await evaluate("document.querySelector('#download-recording').click()");
  const text = await evaluate<string>("window.recordingBlob.text()");
  assert.ok(Buffer.byteLength(text) <= MAX_RECORDING_BYTES);
  const recording = parseRecording(text);
  console.log(
    `Baseline: ${recording.initial_snapshot.processes.length} processes, ${recording.events.length} events`,
  );
  const zipped = gzipSync(text, { level: 9 });
  assert.ok(zipped.length <= PAGES_MAX_ASSET_BYTES);
  const browserPids = new Set(
    recording.initial_snapshot.processes
      .filter(isBrowserProcess)
      .map((p) => p.identity.pid),
  );
  assert.ok(browserPids.size >= 4, `Real ${browserName} process tree required`);
  const activities = recording.events.filter(
    (event) => event.type === "activity",
  );
  assert.ok(activities.length > 0, "Activity events required");
  const browserObservations = {
    cpu: activities
      .flatMap((event) => event.data.cpu ?? [])
      .filter(
        (sample) =>
          browserPids.has(sample.process_id.pid) && sample.runtime_ns > 0,
      ).length,
    ipc: activities
      .flatMap((event) => event.data.ipc ?? [])
      .filter(
        (sample) => browserPids.has(sample.process_id.pid) && sample.bytes > 0,
      ).length,
    files: activities
      .flatMap((event) => event.data.files ?? [])
      .filter(
        (sample) => browserPids.has(sample.process_id.pid) && sample.bytes > 0,
      ).length,
  };
  for (const [sensor, count] of Object.entries(browserObservations))
    assert.ok(count > 0, `${browserName} ${sensor} activity required`);
  await writeFile(output, zipped, { flag: "wx" });
  await cdp("Page.navigate", { url: `${base}/replay/space/` });
  await waitFor("document.querySelector('#replay-status')", "VM replay page");
  await evaluate(`(() => {
    const bytes=Uint8Array.from(atob(${JSON.stringify(zipped.toString("base64"))}), c=>c.charCodeAt(0));
    const transfer=new DataTransfer();transfer.items.add(new File([bytes],'browser-vm.json.gz'));
    const input=document.querySelector('input[type=file]');input.files=transfer.files;input.dispatchEvent(new Event('change'));
  })()`);
  await waitFor(
    "document.querySelector('#replay-status')?.hidden && document.querySelector('#play').textContent==='Pause'",
    "Real gzip replays",
  );
  const replayHeap = await cdp("Runtime.getHeapUsage");
  await writeFile(
    output + ".metrics.json",
    JSON.stringify(
      {
        started_at: recording.started_at,
        duration_ms: recording.duration_ms,
        json_bytes: Buffer.byteLength(text),
        gzip_bytes: zipped.length,
        browser,
        browser_processes: browserPids.size,
        activity_events: activities.length,
        browser_observations: browserObservations,
        baseline_heap: baseline,
        recording_heap_samples: recordingHeap,
        replay_heap: replayHeap,
        browser_errors: errors,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(
    `Saved ${output}: ${Buffer.byteLength(text)} JSON bytes, ${zipped.length} gzip bytes, ${browserPids.size} ${browserName} processes`,
  );
} finally {
  socket?.close();
  chrome.kill("SIGTERM");
  await until(
    () => chrome.exitCode !== null || chrome.signalCode !== null,
    "capture Chrome exit",
  );
  await rm(profile, { recursive: true, force: true });
}
