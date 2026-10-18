#!/usr/bin/env node
// Offline CPU, network IPC and IndexedDB workload, using chrome_load's tab controller.
import {createServer} from 'node:http';
import {spawn} from 'node:child_process';

const html = `<!doctype html><title>SPACE Chrome workload</title>
<h1>SPACE Chrome workload</h1><canvas width="320" height="200"></canvas>
<script>
const worker = new Worker(URL.createObjectURL(new Blob([
  'onmessage=()=>{const end=performance.now()+20;let n=0;while(performance.now()<end)n+=Math.sqrt(n+Math.random());postMessage(n)}'
], {type:'text/javascript'})));
const canvas=document.querySelector('canvas').getContext('2d');
worker.onmessage=()=>{canvas.fillStyle='hsl('+Math.random()*360+',60%,50%)';canvas.fillRect(Math.random()*300,Math.random()*180,20,20)};
const open=indexedDB.open('space-workload',1);
open.onupgradeneeded=()=>open.result.createObjectStore('samples');
open.onsuccess=()=>setInterval(async()=>{
  worker.postMessage('sample');
  const bytes=await (await fetch('/payload',{cache:'no-store'})).arrayBuffer();
  open.result.transaction('samples','readwrite').objectStore('samples').put(bytes,Date.now()%32);
},500);
</script>`;
const server = createServer((request, response) => {
  response.setHeader('Cache-Control', 'no-store');
  response.setHeader('Content-Type', request.url === '/payload' ? 'application/octet-stream' : 'text/html');
  response.end(request.url === '/payload' ? Buffer.alloc(64 * 1024, 42) : html);
});
await new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', resolve);
});
const count = Number(process.env.TAB_COUNT ?? 8);
if (!Number.isInteger(count) || count < 1 || count > 1000) {
  server.close();
  throw new Error('TAB_COUNT must be an integer between 1 and 1000');
}
const child = spawn(process.execPath, [new URL('./chrome_load.mjs', import.meta.url).pathname,
  ...Array.from({length: count}, (_, i) => `http://tab${i}.localhost:${server.address().port}/`),
], {stdio: 'inherit', env: {...process.env, LOCAL_WORKLOAD: '1', TAB_COUNT: String(count)}});
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => child.kill(signal));
child.on('error', error => { console.error(error); server.close(); process.exitCode = 1; });
child.on('exit', (code, signal) => { server.close(); process.exitCode = signal ? 1 : code; });
