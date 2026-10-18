#!/usr/bin/env node
// Offline CPU, network IPC and IndexedDB workload for Chrome or Firefox.
import {createServer} from 'node:http';
import {spawn} from 'node:child_process';
import {mkdtemp, writeFile, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';

const html = `<!doctype html><title>SPACE browser workload</title>
<h1>SPACE browser workload</h1><canvas width="320" height="200"></canvas>
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
const reload=Number(new URL(location.href).searchParams.get('reload'));
if(reload>=1000)setTimeout(()=>location.reload(),reload);
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
const browser = process.env.BROWSER ?? 'chrome';
const interval = Number(process.env.INTERVAL_SECONDS ?? 10) * 1000;
if (!['chrome', 'firefox'].includes(browser) || !Number.isFinite(interval) || interval < 1000) {
  server.close();
  throw new Error('BROWSER must be chrome or firefox and INTERVAL_SECONDS at least 1');
}
const urls = Array.from({length: count}, (_, i) => `http://tab${i}.localhost:${server.address().port}/?reload=${browser === 'firefox' ? interval : 0}`);
let profile;
let executable = process.execPath;
let args = [new URL('./chrome_load.mjs', import.meta.url).pathname, ...urls];
if (browser === 'firefox') {
  profile = await mkdtemp(join(tmpdir(), 'firefox-workload-'));
  const prefs = {
    'browser.shell.checkDefaultBrowser': false,
    'browser.aboutwelcome.enabled': false,
    'browser.startup.homepage_override.mstone': 'ignore',
    'browser.newtabpage.enabled': false,
    'browser.newtabpage.activity-stream.feeds.telemetry': false,
    'datareporting.policy.dataSubmissionEnabled': false,
    'toolkit.telemetry.enabled': false,
    'app.update.auto': false,
    'network.captive-portal-service.enabled': false,
    'network.connectivity-service.enabled': false,
    'browser.safebrowsing.downloads.enabled': false,
    'browser.safebrowsing.malware.enabled': false,
    'browser.safebrowsing.phishing.enabled': false,
    'dom.ipc.processCount': count,
  };
  await writeFile(join(profile, 'user.js'), Object.entries(prefs).map(([key, value]) => `user_pref(${JSON.stringify(key)}, ${JSON.stringify(value)});`).join('\n'));
  executable = process.env.FIREFOX || '/opt/firefox/firefox';
  args = ['--headless', '--no-remote', '--profile', profile, ...urls];
}
const child = spawn(executable, args, {stdio: 'inherit', env: {...process.env, LOCAL_WORKLOAD: '1', TAB_COUNT: String(count)}});
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => child.kill(signal));
child.on('error', async error => { console.error(error); server.close(); if (profile) await rm(profile, {recursive:true, force:true}); process.exitCode = 1; });
child.on('exit', async (code, signal) => { server.close(); if (profile) await rm(profile, {recursive:true, force:true}); process.exitCode = signal ? 1 : code; });
