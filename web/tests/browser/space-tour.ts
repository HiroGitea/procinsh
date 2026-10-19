// Exercise tour ownership through real browser input in live and static replay SPACE.
import assert from "node:assert/strict";
import type { Cdp, Evaluate } from "./harness.js";
import { delay, until } from "../support/runtime.js";

export async function checkSpaceTour(cdp: Cdp, evaluate: Evaluate) {
  const active = () =>
    evaluate("document.querySelector('#tour').getAttribute('aria-pressed')");
  const start = () => evaluate("document.querySelector('#tour').click()");
  assert.equal(await active(), "false");
  await start();
  assert.equal(await active(), "true");
  assert.equal(
    await evaluate("document.querySelector('#tour').textContent"),
    "Exit Tour",
  );
  await delay(200);
  await start();
  assert.equal(await active(), "false");
  await start();
  await cdp("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "Escape",
    code: "Escape",
  });
  await cdp("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "Escape",
    code: "Escape",
  });
  assert.equal(await active(), "false");
  for (const type of ["mouseWheel", "mousePressed"] as const) {
    await start();
    if (type === "mouseWheel") {
      await cdp("Input.dispatchMouseEvent", {
        type,
        x: 700,
        y: 500,
        deltaX: 0,
        deltaY: 60,
      });
    } else {
      await cdp("Input.dispatchMouseEvent", {
        type,
        x: 700,
        y: 500,
        button: "left",
        clickCount: 1,
      });
      await cdp("Input.dispatchMouseEvent", {
        type: "mouseMoved",
        x: 710,
        y: 505,
        button: "left",
        buttons: 1,
      });
      await cdp("Input.dispatchMouseEvent", {
        type: "mouseReleased",
        x: 710,
        y: 505,
        button: "left",
        clickCount: 1,
      });
    }
    assert.equal(await active(), "false", `${type} returns camera ownership`);
  }
  for (const action of [
    "document.querySelector('#reset').click()",
    "document.querySelector('#rearrange').click()",
    "document.querySelector('#search').dispatchEvent(new Event('input'))",
  ]) {
    await start();
    await evaluate(action);
    assert.equal(await active(), "false", action);
  }
  await start();
  await cdp("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-reduced-motion", value: "reduce" }],
  });
  await until(
    () => evaluate("document.querySelector('#tour').disabled"),
    "reduced-motion change stops the tour",
  );
  assert.equal(await active(), "false");
  assert.equal(
    await evaluate("document.querySelector('#tour').disabled"),
    true,
  );
  await start();
  assert.equal(await active(), "false");
  await cdp("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-reduced-motion", value: "no-preference" }],
  });
  await until(
    () => evaluate("!document.querySelector('#tour').disabled"),
    "tour is available after motion preference changes",
  );
  assert.equal(
    await evaluate("document.querySelector('#tour').disabled"),
    false,
  );
}
