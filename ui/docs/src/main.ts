import './styles/main.css';

import { TaskbarManager } from './taskbar/manager.js';
import { taskbarBus } from './taskbar/bus.js';
import { FoundryLocalManager } from './foundry-local/manager.js';
import { loadScene } from './iso/scene.js';
import { generate } from './llm/session.js';
import { probeLocalService } from './llm/probe.js';
import type { VizManifestEntry } from './iso/types.js';

// ---- 1. Bootstrap taskbar -----------------------------------------------

const taskbarEl = document.getElementById('taskbar');
if (!taskbarEl) throw new Error('#taskbar element not found');

const taskbar = new TaskbarManager(taskbarEl);
// Initial render so the bar appears even when empty
taskbar.render();

// ---- 1b. Bootstrap Foundry Local status + install-assist -----------------

const foundryLocalEl = document.getElementById('foundry-local');
if (!foundryLocalEl) throw new Error('#foundry-local element not found');

const foundryLocal = new FoundryLocalManager(foundryLocalEl);
foundryLocal.refreshStatus().catch(err => {
  console.error('Failed to load Foundry Local status:', err);
});

// ---- 2. Probe for local service (async, non-blocking) -------------------

probeLocalService().then(service => {
  if (service) {
    taskbarBus.publish({
      type: 'toast',
      toast: {
        id: crypto.randomUUID(),
        message: `Local service found: ${service.name}`,
        level: 'success',
        ttl: 4000,
      },
    });
  } else {
    taskbarBus.publish({
      type: 'toast',
      toast: {
        id: crypto.randomUUID(),
        message: 'No local service detected — in-browser LLM will be used',
        level: 'info',
        ttl: 5000,
      },
    });
  }
}).catch(() => {
  // probe errors are non-fatal
});

// ---- 3. Load ISO scene ---------------------------------------------------

const svgCanvas = document.getElementById('iso-canvas') as SVGSVGElement | null;
if (!svgCanvas) throw new Error('#iso-canvas element not found');

let selectedEntry: VizManifestEntry | null = null;

const simRunBtn = document.getElementById('sim-run') as HTMLButtonElement | null;
const simOutput = document.getElementById('sim-content');
const simPanel  = document.getElementById('sim-output');
const simClose  = document.getElementById('sim-close');
const simTitle  = document.getElementById('sim-title');

function onTileSelect(entry: VizManifestEntry): void {
  selectedEntry = entry;
  if (simTitle) simTitle.textContent = entry.type_name;
  if (simOutput) simOutput.textContent = entry.spec.description;
  if (simPanel)  simPanel.classList.add('sim-output--visible');
  if (simRunBtn) simRunBtn.disabled = false;
}

simClose?.addEventListener('click', () => {
  if (simPanel) simPanel.classList.remove('sim-output--visible');
  selectedEntry = null;
  if (simRunBtn) simRunBtn.disabled = true;
});

loadScene(svgCanvas, onTileSelect).catch(err => {
  console.error('Failed to load scene:', err);
  taskbarBus.publish({
    type: 'toast',
    toast: {
      id: crypto.randomUUID(),
      message: `Scene load failed: ${err instanceof Error ? err.message : String(err)}`,
      level: 'error',
      ttl: 8000,
    },
  });
});

// ---- 4. Simulate button -------------------------------------------------

simRunBtn?.addEventListener('click', async () => {
  if (!selectedEntry || !simOutput) return;

  const entry = selectedEntry;
  const spec  = entry.spec;

  const prompt = [
    `You are narrating the l3dg3rr financial pipeline visualization tool.`,
    `The user clicked on the "${entry.type_name}" object.`,
    ``,
    `Object description: ${spec.description}`,
    `Layer: ${spec.z_layer}`,
    `Semantic type: ${spec.semantic_type}`,
    ``,
    `Rhai DSL example:`,
    '```',
    spec.rhai_dsl,
    '```',
    ``,
    `In 3-4 sentences, explain what this object does in the pipeline and how the Rhai DSL snippet relates to it. Be concise and technical.`,
  ].join('\n');

  simRunBtn.disabled = true;
  simOutput.textContent = '';

  const loadingEl = document.createElement('span');
  loadingEl.className = 'sim-loading';
  loadingEl.textContent = 'Generating…';
  simOutput.appendChild(loadingEl);

  try {
    await generate(prompt, chunk => {
      // Remove loading indicator on first chunk
      const existing = simOutput.querySelector('.sim-loading');
      if (existing) existing.remove();

      simOutput.textContent += chunk;
    });
  } catch (err) {
    simOutput.textContent = `Error: ${err instanceof Error ? err.message : String(err)}`;
    taskbarBus.publish({
      type: 'toast',
      toast: {
        id: crypto.randomUUID(),
        message: 'LLM generation failed',
        level: 'error',
        ttl: 5000,
      },
    });
  } finally {
    simRunBtn.disabled = false;
  }
});
