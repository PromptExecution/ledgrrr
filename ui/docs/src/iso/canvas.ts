import type { VizManifest, VizManifestEntry, ZLayerName } from './types.js';
import { LAYER_COLORS, LAYER_BASE_Z } from './types.js';

const SVG_NS = 'http://www.w3.org/2000/svg';

const BOX_W      = 40;   // isometric tile width (world units, ~half-width visual)
const BOX_H      = 20;   // depth (world units)
const BOX_Y_SIZE = 16;   // vertical height of the tile face (world units)
const SCALE      = 1.0;
const ORIGIN_X   = 120;
const ORIGIN_Y   = 80;
const TILE_GAP   = 60;   // spacing between tiles in same layer

/** Isometric projection matching Rust iso_project / JS formula */
function isoProject(
  x: number, y: number, z: number,
  scale: number, ox: number, oy: number,
): { sx: number; sy: number } {
  return {
    sx: ox + (x - z) * scale * 0.866,
    sy: oy + (x + z) * scale * 0.5 - y * scale,
  };
}

/** Lighten a hex color by mixing with white */
function lightenColor(hex: string, factor: number): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const lr = Math.round(r + (255 - r) * factor);
  const lg = Math.round(g + (255 - g) * factor);
  const lb = Math.round(b + (255 - b) * factor);
  return `#${lr.toString(16).padStart(2, '0')}${lg.toString(16).padStart(2, '0')}${lb.toString(16).padStart(2, '0')}`;
}

/** Darken a hex color */
function darkenColor(hex: string, factor: number): string {
  return lightenColor(hex, -factor);
}

function svgEl<K extends keyof SVGElementTagNameMap>(tag: K): SVGElementTagNameMap[K] {
  return document.createElementNS(SVG_NS, tag);
}

/**
 * Render one isometric tile (box: top + left + right faces).
 * Returns a <g> element.
 */
function renderTile(
  x: number, z: number,
  color: string,
  entry: VizManifestEntry,
  onSelect: (entry: VizManifestEntry) => void,
): SVGGElement {
  const g = svgEl('g');
  g.setAttribute('class', 'iso-tile');
  g.setAttribute('data-type', entry.type_name);
  g.setAttribute('role', 'button');
  g.setAttribute('tabindex', '0');
  g.setAttribute('aria-label', entry.type_name);

  // Tile corners in world space
  // Center of tile is at (x, 0, z) in world coords
  const hw = BOX_W / 2;   // half-width
  const hd = BOX_H / 2;   // half-depth

  // Top face: diamond (4 corners)
  const tl = isoProject(x - hw, BOX_Y_SIZE, z,      SCALE, ORIGIN_X, ORIGIN_Y);
  const tr = isoProject(x,      BOX_Y_SIZE, z - hd, SCALE, ORIGIN_X, ORIGIN_Y);
  const br = isoProject(x + hw, BOX_Y_SIZE, z,      SCALE, ORIGIN_X, ORIGIN_Y);
  const bl = isoProject(x,      BOX_Y_SIZE, z + hd, SCALE, ORIGIN_X, ORIGIN_Y);

  // Bottom row (y=0)
  const bbl = isoProject(x - hw, 0, z,      SCALE, ORIGIN_X, ORIGIN_Y);
  const bbr = isoProject(x + hw, 0, z,      SCALE, ORIGIN_X, ORIGIN_Y);
  const bbll = isoProject(x,     0, z + hd, SCALE, ORIGIN_X, ORIGIN_Y);

  const topColor   = lightenColor(color, 0.15);
  const leftColor  = lightenColor(color, 0.05);
  const rightColor = darkenColor(color, 0.15);

  // Top face
  const top = svgEl('polygon');
  top.setAttribute('points', `${tl.sx},${tl.sy} ${tr.sx},${tr.sy} ${br.sx},${br.sy} ${bl.sx},${bl.sy}`);
  top.setAttribute('fill', topColor);
  top.setAttribute('stroke', '#0f172a');
  top.setAttribute('stroke-width', '0.5');

  // Left face
  const left = svgEl('polygon');
  left.setAttribute('points', `${tl.sx},${tl.sy} ${bl.sx},${bl.sy} ${bbll.sx},${bbll.sy} ${bbl.sx},${bbl.sy}`);
  left.setAttribute('fill', leftColor);
  left.setAttribute('stroke', '#0f172a');
  left.setAttribute('stroke-width', '0.5');

  // Right face
  const right = svgEl('polygon');
  right.setAttribute('points', `${bl.sx},${bl.sy} ${br.sx},${br.sy} ${bbr.sx},${bbr.sy} ${bbll.sx},${bbll.sy}`);
  right.setAttribute('fill', rightColor);
  right.setAttribute('stroke', '#0f172a');
  right.setAttribute('stroke-width', '0.5');

  g.appendChild(left);
  g.appendChild(right);
  g.appendChild(top);

  // Label inside top face
  const cx = (tl.sx + tr.sx + br.sx + bl.sx) / 4;
  const cy = (tl.sy + tr.sy + br.sy + bl.sy) / 4;
  const label = svgEl('text');
  label.setAttribute('x', String(cx));
  label.setAttribute('y', String(cy + 3));
  label.setAttribute('text-anchor', 'middle');
  label.setAttribute('font-size', '5');
  label.setAttribute('fill', '#f8fafc');
  label.setAttribute('pointer-events', 'none');
  const shortName = entry.type_name.replace(/^PipelineState<(.+)>$/, '$1');
  label.textContent = shortName.length > 12 ? shortName.slice(0, 11) + '…' : shortName;
  g.appendChild(label);

  g.addEventListener('click', () => onSelect(entry));
  g.addEventListener('keydown', e => {
    if (e.key === 'Enter' || e.key === ' ') onSelect(entry);
  });

  return g;
}

/**
 * Render a layer label as SVG text.
 */
function renderLayerLabel(
  layerName: ZLayerName,
  color: string,
  z: number,
): SVGTextElement {
  const labelPos = isoProject(-BOX_W / 2 - 10, BOX_Y_SIZE + 8, z, SCALE, ORIGIN_X, ORIGIN_Y);
  const text = svgEl('text');
  text.setAttribute('class', 'layer-label');
  text.setAttribute('x', String(labelPos.sx));
  text.setAttribute('y', String(labelPos.sy));
  text.setAttribute('fill', color);
  text.setAttribute('font-size', '8');
  text.setAttribute('font-weight', 'bold');
  text.setAttribute('text-anchor', 'end');
  text.textContent = layerName;
  return text;
}

/**
 * Render the full scene into the given SVG element.
 * Tiles are grouped by ZLayer; each layer is a row along the Z axis.
 */
export function renderScene(
  svgEl: SVGSVGElement,
  manifest: VizManifest,
  onSelect: (entry: VizManifestEntry) => void,
): void {
  // Clear previous content
  while (svgEl.firstChild) svgEl.removeChild(svgEl.firstChild);

  if (manifest.objects.length === 0) {
    // Placeholder when manifest is empty
    const msg = document.createElementNS(SVG_NS, 'text');
    msg.setAttribute('x', '50%');
    msg.setAttribute('y', '50%');
    msg.setAttribute('text-anchor', 'middle');
    msg.setAttribute('dominant-baseline', 'middle');
    msg.setAttribute('fill', '#64748b');
    msg.setAttribute('font-size', '14');
    msg.textContent = 'Run `cargo xtask export-viz-manifest` to populate the scene.';
    svgEl.appendChild(msg);
    return;
  }

  // Group by layer
  const byLayer = new Map<ZLayerName, VizManifestEntry[]>();
  for (const entry of manifest.objects) {
    const layer = entry.spec.z_layer as ZLayerName;
    if (!byLayer.has(layer)) byLayer.set(layer, []);
    byLayer.get(layer)!.push(entry);
  }

  // Render layers in z-order (Document first, Attestation last / highest Z)
  const layerOrder: ZLayerName[] = [
    'Document', 'Pipeline', 'Constraint', 'Legal', 'FormalProof', 'Attestation',
  ];

  for (const layerName of layerOrder) {
    const entries = byLayer.get(layerName);
    if (!entries || entries.length === 0) continue;

    const color  = LAYER_COLORS[layerName];
    const baseZ  = LAYER_BASE_Z[layerName] / 136 * TILE_GAP; // scale world Z

    // Layer group
    const layerG = document.createElementNS(SVG_NS, 'g');
    layerG.setAttribute('class', 'iso-layer');
    layerG.setAttribute('data-layer', layerName);

    // Label
    layerG.appendChild(renderLayerLabel(layerName, color, baseZ));

    // Tiles spaced along the X axis
    entries.forEach((entry, i) => {
      const x = i * TILE_GAP;
      const tile = renderTile(x, baseZ, color, entry, onSelect);
      layerG.appendChild(tile);
    });

    svgEl.appendChild(layerG);
  }

  // Resize SVG viewBox to fit content
  const bbox = (svgEl as SVGSVGElement).getBBox?.();
  if (bbox && bbox.width > 0) {
    const pad = 20;
    svgEl.setAttribute(
      'viewBox',
      `${bbox.x - pad} ${bbox.y - pad} ${bbox.width + pad * 2} ${bbox.height + pad * 2}`,
    );
  }
}
