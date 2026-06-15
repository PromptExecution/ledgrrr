#!/usr/bin/env node
// UX test: autorun countdown timer in main-legacy.js
// Usage: node test-autorun.js [--autorun-ms=<N>]
//   --autorun-ms=0   → autorun disabled (no switch)
//   --autorun-ms=N   → switch after N ms (default: 1000 for fast CI test)
//
// Tests:
//   1. startAutorun() fires showPanel(DASH) after AUTORUN_MS
//   2. clearAutorun() prevents the switch
//   3. AUTORUN_MS=0 disables autorun entirely

const assert = require('assert');

// --- minimal DOM/browser shim ---
let _statusBarText = '';
let _activePanel = 0;
let _panelSwitches = [];
let _timerCallbacks = [];
let _time = 0;

function advanceFakeTime(ms) {
  _time += ms;
  // fire all pending timeouts whose deadline <= _time
  let fired = true;
  while (fired) {
    fired = false;
    for (let i = 0; i < _timerCallbacks.length; i++) {
      const cb = _timerCallbacks[i];
      if (cb && cb.at <= _time) {
        _timerCallbacks.splice(i, 1);
        cb.fn();
        fired = true;
        break;
      }
    }
  }
}

global.window = {
  __TAURI__: null,
  location: { search: '' },
};
global.location = { search: '' };
global.document = {
  getElementById: function(id) {
    if (id === 'status-bar') {
      return {
        get textContent() { return _statusBarText; },
        set textContent(v) { _statusBarText = v; },
        dataset: {},
        indexOf: undefined,
      };
    }
    return null;
  },
  querySelectorAll: () => ({ forEach: () => {} }),
};
let _nextTimerId = 1; // start at 1; 0 is falsy and breaks clearTimeout guards
global.setTimeout = function(fn, delay) {
  const id = _nextTimerId++;
  _timerCallbacks.push({ at: _time + delay, fn, id });
  return id;
};
global.clearTimeout = function(id) {
  for (let i = 0; i < _timerCallbacks.length; i++) {
    if (_timerCallbacks[i] && _timerCallbacks[i].id === id) {
      _timerCallbacks.splice(i, 1);
      return;
    }
  }
};
global.URLSearchParams = URLSearchParams;

// Parse test config from CLI
const args = Object.fromEntries(
  process.argv.slice(2).map(a => a.replace(/^--/, '').split('='))
);
const testAutorunMs = parseInt(args['autorun-ms'] ?? '1000', 10);

// --- inject JS under test (inline minimal copy of relevant logic) ---
// We can't directly require main-legacy.js (it has no module.exports and uses
// browser globals), so we eval the relevant portions.

const fs = require('fs');
const src = fs.readFileSync(__dirname + '/main-legacy.js', 'utf8');

// Set AUTORUN_MS via location.search before eval
global.location.search = `?autorun=${testAutorunMs}`;

// Stub functions called by startAutorun/clearAutorun that don't exist in shim
global.refreshDashboard = function() {};
global.showPanel = function(i) { _activePanel = i; _panelSwitches.push(i); };
global.PANELS = [
  {id:'chat'},{id:'logs'},{id:'dash'},{id:'settings'},{id:'docs'},{id:'viz'}
];
global.DASH_PANEL_INDEX = 2;

// Extract autorun block: from '// ?autorun=' comment through end of clearAutorun()
function extractAutorunBlock(src) {
  const start = src.indexOf('// ?autorun=');
  if (start === -1) return null;
  const fnStart = src.indexOf('function clearAutorun(){');
  if (fnStart === -1) return null;
  let depth = 0, i = fnStart;
  for (; i < src.length; i++) {
    if (src[i] === '{') depth++;
    else if (src[i] === '}') { depth--; if (depth === 0) { i++; break; } }
  }
  return src.slice(start, i);
}
const autorunBlock = extractAutorunBlock(src);
if (!autorunBlock) { console.error('FAIL: could not extract autorun block from source'); process.exit(1); }
eval(autorunBlock);  // defines AUTORUN_MS, startAutorun, clearAutorun

// --- TEST 1: autorun fires after AUTORUN_MS ---
if (testAutorunMs === 0) { console.log('  SKIP test 1+2: AUTORUN_MS=0, running disabled test only'); }
if (testAutorunMs > 0) (function testAutorunFires() {
  _activePanel = 0; _panelSwitches = []; _statusBarText = ''; _timerCallbacks = []; _time = 0; _nextTimerId = 1;
  startAutorun();

  // Before time elapses — no switch
  advanceFakeTime(testAutorunMs - 1);
  assert.strictEqual(_panelSwitches.length, 0, 'should not switch before timeout');
  // Countdown text visible (only if testAutorunMs > 1000)
  if (testAutorunMs > 1000) {
    assert.ok(_statusBarText.includes('⏱'), 'should show countdown in status bar');
  }

  // After full elapsed time — switch fires
  advanceFakeTime(2000); // overshoot by 2s to handle tick rounding
  assert.ok(_panelSwitches.includes(DASH_PANEL_INDEX), 'should switch to dash panel');
  assert.ok(!_statusBarText.includes('⏱'), 'status bar should be cleared after switch');
  console.log('  PASS test 1: autorun fires after', testAutorunMs, 'ms');
})();

// --- TEST 2: clearAutorun() cancels the switch ---
if (testAutorunMs > 0) (function testClearAutorunCancels() {
  _activePanel = 0; _panelSwitches = []; _statusBarText = ''; _timerCallbacks = []; _time = 0; _nextTimerId = 1;
  startAutorun();
  advanceFakeTime(testAutorunMs / 2); // halfway through
  clearAutorun();
  advanceFakeTime(testAutorunMs * 2); // well past deadline
  assert.strictEqual(_panelSwitches.length, 0, 'clearAutorun should prevent panel switch');
  console.log('  PASS test 2: clearAutorun() cancels');
})();

// --- TEST 3: AUTORUN_MS=0 disables autorun ---
// Run with: node test-autorun.js --autorun-ms=0
if (testAutorunMs === 0) {
  (function testAutorunZeroDisabled() {
    _activePanel = 0; _panelSwitches = []; _statusBarText = ''; _timerCallbacks = []; _time = 0; _nextTimerId = 1;
    startAutorun();
    advanceFakeTime(60000);
    assert.strictEqual(_timerCallbacks.length, 0, 'no timers when AUTORUN_MS=0');
    assert.strictEqual(_panelSwitches.length, 0, 'no panel switch when AUTORUN_MS=0');
    console.log('  PASS test 3: AUTORUN_MS=0 disables');
  })();
} else {
  console.log('  SKIP test 3: run with --autorun-ms=0 to test disabled mode');
}

console.log('All autorun UX tests passed.');
