#!/usr/bin/env node
// Chrome load generator with the larger default external site list.
process.env.WORKLOAD = 'chrome';
if (!process.env.SITE_LIST && !process.argv.slice(2).length) {
  process.env.SITE_LIST = new URL('./chrome_sites.txt', import.meta.url).pathname;
}
process.env.TAB_COUNT ??= '20';
await import('./browser_workload.mjs');
