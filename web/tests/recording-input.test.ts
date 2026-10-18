import { expect, test } from "vitest";
import { gzipSync } from "node:zlib";
import { readFileSync } from "node:fs";
import { readRecording } from "../src/space/recording-input.js";
import { parseRecording } from "../src/space/recording.js";

const json = readFileSync(
  new URL("../replay-public/recordings/demo.json", import.meta.url),
  "utf8",
);
function chunks(bytes: Uint8Array, size = 1024) {
  let offset = 0;
  return new ReadableStream<Uint8Array<ArrayBuffer>>({
    pull(controller) {
      if (offset === bytes.length) controller.close();
      else {
        controller.enqueue(
          new Uint8Array(bytes.subarray(offset, offset + size)),
        );
        offset = Math.min(offset + size, bytes.length);
      }
    },
  });
}
test("plain JSON and gzip expand to identical recordings, including split magic bytes", async () => {
  const plain = new TextEncoder().encode(json);
  expect(await readRecording(chunks(plain))).toBe(json);
  for (const size of [1, 1024]) {
    const decoded = await readRecording(chunks(gzipSync(plain), size));
    expect(decoded).toBe(json);
    expect(parseRecording(decoded)).toEqual(parseRecording(json));
  }
  // This is also the stream fetch exposes after HTTP Content-Encoding decoding.
  expect(await readRecording(chunks(plain))).toBe(json);
});
test("truncated gzip and corrupt gzip checksums are rejected", async () => {
  const zipped = gzipSync(json);
  await expect(readRecording(chunks(zipped.subarray(0, -4)))).rejects.toThrow();
  zipped[zipped.length - 8] ^= 0xff;
  await expect(readRecording(chunks(zipped))).rejects.toThrow();
});
test("both input and inflated bytes have a streaming size limit", async () => {
  const text = new TextEncoder().encode("a".repeat(8192));
  for (const bytes of [text, gzipSync(text)])
    await expect(readRecording(chunks(bytes), 4096)).rejects.toThrow("128 MiB");
  expect(await readRecording(chunks(text), text.length)).toBe("a".repeat(8192));
});
test("invalid JSON remains subject to the shared recording parser", async () => {
  for (const invalid of ["{", "{}"])
    expect(() => parseRecording(invalid)).toThrow();
  const text = await readRecording(chunks(gzipSync("{}")));
  expect(() => parseRecording(text)).toThrow("Invalid SPACE recording");
});
