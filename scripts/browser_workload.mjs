#!/usr/bin/env node
// External-site browser workload. Requires Node.js >=22 and Chrome or Firefox.
import {spawn} from 'node:child_process';
import {mkdtemp, rm, writeFile, readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';

const readSites = async path => (await readFile(path, 'utf8'))
  .split(/\r?\n/).map(line => line.trim()).filter(line => line && !line.startsWith('#'));
const urls = process.argv.slice(2);
if (process.env.SITE_LIST) urls.push(...await readSites(process.env.SITE_LIST));
else if (!urls.length) urls.push(...await readSites(new URL('./browser_sites.txt', import.meta.url)));
if (!urls.length) throw new Error('The site list is empty');
for (const url of urls) {
  if (!['http:', 'https:'].includes(new URL(url).protocol)) throw new Error(`Invalid URL: ${url}`);
}
const tabCount = Number(process.env.TAB_COUNT ?? Math.min(8, urls.length));
if (!Number.isInteger(tabCount) || tabCount < 1 || tabCount > 1000) throw new Error('TAB_COUNT must be an integer between 1 and 1000');
const tabUrls = Array.from({length: tabCount}, (_, index) => urls[index % urls.length]);
const interval = Number(process.env.INTERVAL_SECONDS ?? 10) * 1000;
if (!Number.isFinite(interval) || interval < 1000) throw new Error('INTERVAL_SECONDS must be at least 1');
const browser = process.env.BROWSER ?? 'chrome';
if (!['chrome', 'firefox'].includes(browser)) throw new Error('BROWSER must be chrome or firefox');
const firefox = browser === 'firefox';
const profile = await mkdtemp(join(tmpdir(), `${browser}-reload-profile-`));
const sockets = [];
const pidFile = process.env.PID_FILE;
let ownsPidFile = false;
let stopping = false;
const log = message => console.log(`${new Date().toISOString()} ${message}`);
async function stopChild(child) {
  if (child.exitCode !== null || child.signalCode !== null || !child.pid) return;
  const exited = new Promise(resolve => child.once('exit', resolve));
  child.kill('SIGTERM');
  await Promise.race([exited, delay(3000)]);
  if (child.exitCode === null && child.signalCode === null) { child.kill('SIGKILL'); await exited; }
}
if (firefox) {
  await writeFile(join(profile, 'user.js'), [
    'user_pref("browser.shell.checkDefaultBrowser", false);',
    'user_pref("browser.aboutwelcome.enabled", false);',
    'user_pref("browser.startup.homepage_override.mstone", "ignore");',
    'user_pref("datareporting.policy.dataSubmissionEnabled", false);',
    'user_pref("toolkit.telemetry.enabled", false);',
    'user_pref("app.update.auto", false);',
  ].join('\n'));
}
const child = spawn(firefox ? (process.env.FIREFOX || '/opt/firefox/firefox') : (process.env.CHROME || '/usr/bin/google-chrome'), firefox ? [
  '--headless', '--no-remote', '--profile', profile, '--remote-debugging-port=0', 'about:blank',
] : [
  ...(process.env.DISPLAY || process.env.WAYLAND_DISPLAY ? [] : ['--headless=new']),
  '--disable-gpu', '--disable-dev-shm-usage', '--no-first-run', '--no-default-browser-check', '--remote-debugging-address=127.0.0.1',
  '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
], {stdio: ['ignore', 'pipe', 'pipe']});
let output = '';
for (const stream of [child.stdout, child.stderr]) stream.on('data', data => { output = (output + data.toString()).slice(-20000); });
let launchError;
child.on('error', error => { launchError = error; });
let browserExited = false;
child.on('exit', (code, signal) => {
  browserExited = true;
  if (!stopping) {
    launchError = new Error(`${browser} exited unexpectedly (code=${code}, signal=${signal}): ${output}`);
    process.exitCode = 1;
  }
  stopping = true;
});
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => { stopping = true; });

async function connect(url) {
  const ws = new WebSocket(url);
  sockets.push(ws);
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`${browser} connection timed out`)), 15000);
    ws.onopen = () => { clearTimeout(timer); resolve(); };
    ws.onerror = () => { clearTimeout(timer); reject(new Error(`${browser} connection failed`)); };
  });
  let sequence = 0;
  const pending = new Map();
  ws.onmessage = event => {
    const message = JSON.parse(event.data);
    if (!message.id) return;
    const task = pending.get(message.id);
    if (!task) return;
    pending.delete(message.id);
    clearTimeout(task.timer);
    message.error ? task.reject(new Error(JSON.stringify(message.error))) : task.resolve(message.result);
  };
  ws.onclose = () => {
    for (const task of pending.values()) { clearTimeout(task.timer); task.reject(new Error(`${browser} connection closed`)); }
    pending.clear();
  };
  return (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timed out`)); }, 15000);
    pending.set(id, {resolve, reject, timer});
    ws.send(JSON.stringify({id, method, params}));
  });
}

try {
  if (pidFile) {
    await writeFile(pidFile, `${process.pid}\n`, {flag: 'wx'});
    ownsPidFile = true;
  }
  const deadline = Date.now() + 20000;
  let endpoint;
  while (!(endpoint = output.match(firefox ? /WebDriver BiDi listening on (ws:\/\/127\.0\.0\.1:\d+)/ : /(ws:\/\/127\.0\.0\.1:\d+\/devtools\/browser\/[^\s]+)/)?.[1])) {
    if (launchError || browserExited || stopping || Date.now() > deadline) throw new Error(`${browser} failed to start: ${launchError || output}`);
    await delay(100);
  }
  log(`controller PID=${process.pid}, ${browser} PID=${child.pid}, interval=${interval / 1000}s`);
  const send = await connect(firefox ? `${endpoint}/session` : endpoint);
  if (firefox) await send('session.new', {capabilities: {}});
  const tabs = [];
  if (firefox) {
    const initial = (await send('browsingContext.getTree')).contexts;
    for (const url of tabUrls) {
      const {context} = await send('browsingContext.create', {type: 'tab'});
      await send('browsingContext.navigate', {context, url, wait: 'none'});
      tabs.push({url, reload: () => send('browsingContext.reload', {context, ignoreCache: true, wait: 'none'})});
      log(`opened tab ${tabs.length}/${tabCount} ${url}`);
    }
    for (const {context} of initial) await send('browsingContext.close', {context});
  } else {
    const initialTargets = (await send('Target.getTargets')).targetInfos.filter(target => target.type === 'page' && target.url === 'about:blank');
    const targets = [];
    for (const url of tabUrls) {
      if (stopping) break;
      const {targetId} = await send('Target.createTarget', {url: 'about:blank'});
      targets.push({url, targetId});
      log(`opened tab ${targets.length}/${tabCount} ${url}`);
    }
    const pages = await (await fetch(`http://127.0.0.1:${new URL(endpoint).port}/json/list`)).json();
    const navigations = [];
    for (const {url, targetId} of targets) {
      if (stopping) break;
      const page = pages.find(page => page.id === targetId);
      if (!page) throw new Error(`${browser} tab disappeared: ${url}`);
      const pageCdp = await connect(page.webSocketDebuggerUrl);
      await pageCdp('Page.enable');
      await pageCdp('Network.enable');
      await pageCdp('Network.setCacheDisabled', {cacheDisabled: true});
      navigations.push(pageCdp('Page.navigate', {url}).then(result => {
        if (result.errorText) log(`navigation error ${url}: ${result.errorText}`);
      }).catch(error => log(`navigation error ${url}: ${error.message}`)));
      tabs.push({url, reload: () => pageCdp('Page.reload', {ignoreCache: true})});
    }
    await Promise.all(navigations);
    for (const target of initialTargets) await send('Target.closeTarget', {targetId: target.targetId});
  }
  log(`ready: ${tabs.length} page tabs`);
  let round = 0;
  let next = Date.now() + interval;
  while (!stopping) {
    await delay(Math.min(200, Math.max(0, next - Date.now())));
    if (stopping || Date.now() < next) continue;
    round++;
    await Promise.all(tabs.map(async ({url, reload}) => {
      await reload();
      log(`reload ${round} ${url}`);
    }));
    log(`reload round ${round} complete: ${tabs.length} tabs`);
    next += interval;
    if (next < Date.now()) next = Date.now() + interval;
  }
} catch (error) {
  console.error(error);
  process.exitCode = 1;
} finally {
  stopping = true;
  for (const ws of sockets) ws.close();
  await stopChild(child);
  await rm(profile, {recursive: true, force: true});
  if (ownsPidFile && (await readFile(pidFile, 'utf8').catch(() => '')).trim() === String(process.pid)) await rm(pidFile, {force: true});
  log('stopped');
}
