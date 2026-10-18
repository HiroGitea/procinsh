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

const port = Number(process.env.HOST_PORT ?? 10090);
const seconds = Number(process.env.RECORD_SECONDS ?? 15);
assert.ok(Number.isInteger(port) && port > 0 && port <= 65535);
assert.ok(Number.isFinite(seconds) && seconds >= 1 && seconds <= 300);
const output = process.env.RECORD_OUTPUT ?? "/tmp/chrome-vm.json.gz";
const base = `http://127.0.0.1:${port}`;
const profile = await mkdtemp(join(tmpdir(), "space-vm-capture-"));
const errors: string[] = [];
const chrome = launch(process.env.CHROME || "/usr/bin/google-chrome", [
  "--headless=new",
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
  await cdp("Page.navigate", { url: `${base}/space` });
  await waitFor(
    "document.querySelector('#record')?.disabled===false",
    "VM recording baseline",
  );
  const baseline = await cdp("Runtime.getHeapUsage");
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
  const zipped = gzipSync(text, { level: 9 });
  assert.ok(zipped.length <= PAGES_MAX_ASSET_BYTES);
  const chromePids = new Set(
    recording.initial_snapshot.processes
      .filter((p) => /chrome/.test(p.name))
      .map((p) => p.identity.pid),
  );
  assert.ok(chromePids.size >= 4, "Real Chrome process tree required");
  const activities = recording.events.filter(
    (event) => event.type === "activity",
  );
  assert.ok(activities.length > 0, "Activity events required");
  await writeFile(output, zipped, { flag: "wx" });
  await cdp("Page.navigate", { url: `${base}/replay/space/` });
  await waitFor("document.querySelector('#replay-status')", "VM replay page");
  await evaluate(`(() => {
    const bytes=Uint8Array.from(atob(${JSON.stringify(zipped.toString("base64"))}), c=>c.charCodeAt(0));
    const transfer=new DataTransfer();transfer.items.add(new File([bytes],'chrome-vm.json.gz'));
    const input=document.querySelector('input[type=file]');input.files=transfer.files;input.dispatchEvent(new Event('change'));
  })()`);
  await waitFor(
    "document.querySelector('#replay-status').textContent.includes('chrome-vm.json.gz') && document.querySelector('#play').textContent==='Pause'",
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
        chrome_processes: chromePids.size,
        activity_events: activities.length,
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
    `Saved ${output}: ${Buffer.byteLength(text)} JSON bytes, ${zipped.length} gzip bytes, ${chromePids.size} Chrome processes`,
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
