// Repeatable cold layout, live fanout, and cached update measurements (no WebGL).
import { SpaceDataStore } from "../src/space/data.js";
import { processInfo } from "../tests/support/fixtures.js";

const count = 1000;
function processes(fanout: boolean) {
  return Array.from({ length: count }, (_, i) =>
    processInfo({
      identity: { pid: i + 1, start_time_ticks: 1 },
      parent_id: i
        ? { pid: fanout ? 1 : Math.floor((i - 1) / 4) + 1, start_time_ticks: 1 }
        : null,
    }),
  );
}
for (const scenario of ["initial hierarchy", "live fanout", "cached update"]) {
  const samples: number[] = [];
  for (let run = 0; run < 5; run++) {
    const store = new SpaceDataStore();
    const snapshot = {
      processes: processes(scenario !== "initial hierarchy"),
      fd_relations: [],
    };
    if (scenario === "live fanout")
      store.replaceSnapshot({
        processes: [snapshot.processes[0]],
        fd_relations: [],
      });
    if (scenario === "cached update") store.replaceSnapshot(snapshot);
    const started = performance.now();
    store.replaceSnapshot(snapshot);
    samples.push(performance.now() - started);
    if (store.parentRoutes.length !== count - 1)
      throw new Error("Missing family routes");
  }
  samples.sort((a, b) => a - b);
  console.log(
    `${scenario}: median ${samples[2].toFixed(1)} ms, max ${samples[4].toFixed(1)} ms (${count} processes, 5 runs)`,
  );
}
