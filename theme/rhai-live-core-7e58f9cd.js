(function (root, factory) {
    const api = factory();
    if (typeof module !== "undefined" && module.exports) {
        module.exports = api;
    }
    if (root) {
        root.RhaiLiveCore = api;
    }
})(typeof globalThis !== "undefined" ? globalThis : this, function () {
    const ISO_SETTINGS = {
        levelGap: 192,
        laneGap: 136,
        decisionLift: 34,
        reviewLift: 18,
        commitLift: 12,
        scale: 1,
        margin: 64,
        cardWidth: 122,
        cardHeight: 68,
        cardDepth: 20,
        animationMs: 460,
    };

    function regularPolygon(sides, radius, rotation = 0) {
        const points = [];
        for (let i = 0; i < sides; i++) {
            const angle = (i / sides) * Math.PI * 2 - Math.PI / 2 + rotation;
            points.push([Number((Math.cos(angle) * radius).toFixed(4)), Number((Math.sin(angle) * radius).toFixed(4))]);
        }
        return points;
    }

    // --- Visual Legend System (Base Idioms) ---
    
    const SEMANTIC_CATEGORIES = {
        data: { emoji: "📄", color: "#334155", polygon: regularPolygon(12, 0.8) },
        intelligence: { emoji: "🧠", color: "#0284c7", polygon: regularPolygon(8, 0.9) },
        rule: { emoji: "⚖️", color: "#b91c1c", polygon: regularPolygon(6, 0.9, Math.PI / 6) },
        security: { emoji: "🛡️", color: "#0f766e", polygon: [[0, -0.9], [0.8, -0.5], [0.8, 0.3], [0, 0.9], [-0.8, 0.3], [-0.8, -0.5]] },
        human: { emoji: "👤", color: "#b45309", polygon: regularPolygon(16, 0.85) },
        logic: { emoji: "❓", color: "#b91c1c", polygon: regularPolygon(4, 0.95) },
        storage: { emoji: "💾", color: "#15803d", polygon: regularPolygon(24, 0.8) },
        report: { emoji: "📊", color: "#166534", polygon: [[-0.9, -0.6], [0.9, -0.6], [0.7, 0.8], [-0.7, 0.8]] },
        task: { emoji: "⚙️", color: "#475569", polygon: [[-0.9, -0.6], [0.9, -0.6], [0.9, 0.6], [-0.9, 0.6]] },
        event: { emoji: "📅", color: "#7e22ce", polygon: [[-0.8, -0.6], [-0.5, -0.6], [-0.5, -0.9], [-0.3, -0.9], [-0.3, -0.6], [0.3, -0.6], [0.3, -0.9], [0.5, -0.9], [0.5, -0.6], [0.8, -0.6], [0.8, 0.8], [-0.8, 0.8]] },
        decision: { emoji: "❓", color: "#b91c1c", polygon: regularPolygon(4, 0.95) },
        
        // Process Idioms (Promoted to categories for unique shapes)
        ingest: { 
            emoji: "📥", 
            color: "#1d4ed8", 
            polygon: [[-0.8, -0.4], [0.4, -0.4], [0.4, -0.7], [0.9, 0], [0.4, 0.7], [0.4, 0.4], [-0.8, 0.4]] 
        },
        classify: { 
            emoji: "🏷️", 
            color: "#7c3aed", 
            polygon: regularPolygon(5, 0.85) 
        },
        reconcile: { 
            emoji: "🔄", 
            color: "#2563eb", 
            polygon: [[-0.8, -0.8], [0.8, -0.8], [0, 0], [0.8, 0.8], [-0.8, 0.8], [0, 0]] 
        },
    };

    const SPECIALIZED_ROLES = {
        load: { category: "ingest" },
        parse: { category: "ingest" },
        extract: { category: "ingest" },
        docling: { category: "ingest" },
        bridge: { category: "ingest" },
        
        label: { category: "classify" },
        tag: { category: "classify" },
        waterfall: { category: "classify" },
        engine: { category: "classify" },
        
        match: { category: "reconcile" },
        balance: { category: "reconcile" },
        ledger: { category: "reconcile" },
        catalog: { category: "reconcile" },
        xero: { category: "reconcile" },
    };

    function resolveSemanticMetadata(role) {
        const specialized = SPECIALIZED_ROLES[role];
        const categoryId = specialized ? specialized.category : role;
        const category = SEMANTIC_CATEGORIES[categoryId] || SEMANTIC_CATEGORIES.task;
        
        return {
            name: role,
            emoji: (specialized && specialized.emoji) || category.emoji,
            color: (specialized && specialized.color) || category.color,
            polygon: (specialized && specialized.polygon) || category.polygon,
        };
    }

    function sanitizeId(raw) { return raw.replace(/[^A-Za-z0-9_]/g, "_"); }
    function escapeHtml(raw) { return String(raw).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }
    function escapeLabel(raw) { return escapeHtml(raw).replace(/"/g, "&quot;"); }

    function inferSemanticType(label, kind) {
        // Normalise underscores → spaces so that \b word boundaries fire on each
        // segment of compound identifiers like "document_ingestion" or "cpa_review".
        const normalized = String(label || "").toLowerCase().replace(/_/g, " ");
        if (kind === "decision" || kind === "match") return "decision";
        if (/\b(source|input|file|statement|blake3|routing|filename)\b/.test(normalized)) return "data";
        if (/\b(llm|ai|gpt|phi|reasoning|model|runtime)\b/.test(normalized)) return "intelligence";
        if (/\b(legal|rule|constraint|hard|law|code|solver|registry)\b/.test(normalized)) return "rule";
        // Prefix "validat" catches both "validate" and "validation" (noun form).
        if (/\bvalidat|\b(verify|check|guard|candidate|gate)\b/.test(normalized)) return "security";
        if (/\b(review|approve|manual|operator|human|cpa|flags)\b/.test(normalized)) return "human";
        // "audit" and "history" map to storage — these are terminal archive nodes.
        if (/\b(commit|publish|write|persist|done|finish|committed|sidecar|audit|history)\b/.test(normalized)) return "storage";
        if (/\b(export|workbook|excel|xlsx|output)\b/.test(normalized)) return "report";
        if (/\b(calendar|schedule|due|deadline)\b/.test(normalized)) return "event";

        // Prefix matches handle noun/gerund forms: "ingestion", "classification",
        // "reconciliation" alongside the base verb forms.
        if (/\bingest|\b(load|parse|extract|docling|bridge)\b/.test(normalized)) return "ingest";
        if (/\bclassif|\b(label|tag|map|route|waterfall|selector|engine)\b/.test(normalized)) return "classify";
        if (/\breconcil|\b(match|balance|ledger|catalog|xero)\b/.test(normalized)) return "reconcile";

        return "task";
    }

    function parseRhaiDiagram(source) {
        const graph = { order: [], nodes: new Map(), edges: [] };
        const pipelineEdges = []; const conditionals = []; const matchArms = [];
        const diagnostics = [];

        function addNode(id, label, kind, meta) {
            if (!graph.nodes.has(id)) {
                graph.order.push(id);
                graph.nodes.set(id, Object.assign({
                    id,
                    identityKey: id,
                    label,
                    kind,
                    semanticType: inferSemanticType(label, kind),
                    armIndex: null,
                    isDefault: false,
                }, meta || {}));
            }
        }

        function addEdge(from, to, label, meta) {
            graph.edges.push(Object.assign({ from, to, label, armIndex: null, isDefault: false }, meta || {}));
        }

        function parseCondition(expr, target) {
            const ops = [">=", "<=", "!=", ">", "<", "=="];
            for (const op of ops) {
                const i = expr.indexOf(op);
                if (i !== -1) {
                    const lhs = expr.slice(0, i).trim();
                    const rhs = expr.slice(i + op.length).trim();
                    if (lhs && rhs) return { lhs, op, rhs, target };
                }
            }
            return null;
        }

        function addDiagnostic(kind, index, message, line) {
            diagnostics.push({ kind, line: index + 1, message, source: line });
        }

        function opToWord(op) {
            return { ">": "gt", "<": "lt", ">=": "gte", "<=": "lte", "==": "eq", "!=": "ne" }[op] || "op";
        }

        function valToSafe(value) {
            return String(value).replace(".", "_");
        }

        function isDefaultArm(arm) {
            return ["_", "else", "otherwise", "default"].includes(String(arm || "").trim());
        }

        function emitThresholdChain(lhs, op, thresholds) {
            const nodeIds = thresholds.map(function ([value]) {
                return sanitizeId(`${lhs}_${opToWord(op)}_${valToSafe(value)}`);
            });

            thresholds.forEach(function ([value, target], i) {
                const nodeId = nodeIds[i];
                const label = `${lhs} ${op} ${value}`;
                const targetId = sanitizeId(target);
                addNode(nodeId, label, "decision");
                addNode(targetId, target, "step");
                addEdge(nodeId, targetId, "true");
                if (i + 1 < nodeIds.length) addEdge(nodeId, nodeIds[i + 1], "false");
            });
        }

        String(source).split("\n").forEach(function (raw, index) {
            const line = raw.split("//")[0].trim(); if (!line) return;
            if (line.startsWith("fn ")) {
                const p = line.slice(3).split("->");
                if (p.length === 2) {
                    const n = p[0].trim().replace(/\(\)\s*$/, "");
                    const t = p[1].trim();
                    if (n && t) { pipelineEdges.push([n, t]); return; }
                }
                addDiagnostic("error", index, "Malformed fn statement; expected `fn source() -> target`.", line);
                return;
            }
            if (line.startsWith("if ")) {
                const p = line.slice(3).split("->");
                if (p.length === 2) {
                    const e = p[0].trim(); const t = p[1].trim();
                    if (e && t) {
                        const parsed = parseCondition(e, t);
                        if (parsed) conditionals.push(parsed);
                        else conditionals.push({ lhs: sanitizeId(e), op: "", rhs: "", target: sanitizeId(t) });
                        return;
                    }
                }
                addDiagnostic("error", index, "Malformed if statement; expected `if expression -> target`.", line);
                return;
            }
            if (line.startsWith("match ")) {
                const p1 = line.slice(6).split("=>");
                if (p1.length === 2) {
                    const p2 = p1[1].split("->");
                    if (p2.length === 2) { matchArms.push({ expr: p1[0].trim(), arm: p2[0].trim(), target: p2[1].trim() }); return; }
                }
                addDiagnostic("error", index, "Malformed match statement; expected `match expr => Arm -> target`.", line);
                return;
            }
            addDiagnostic("info", index, "Ignored non-diagram Rhai line.", line);
        });

        pipelineEdges.forEach(function ([n, t]) {
            const ni = sanitizeId(n); const ti = sanitizeId(t);
            addNode(ni, n, "step"); addNode(ti, t, "step"); addEdge(ni, ti, null);
        });

        const gtGroups = new Map();
        const ltGroups = new Map();
        const plainConds = [];
        conditionals.forEach(function (c) {
            if ((c.op === ">" || c.op === "<") && Number.isFinite(Number(c.rhs))) {
                const groups = c.op === ">" ? gtGroups : ltGroups;
                const group = groups.get(c.lhs) || [];
                group.push([Number(c.rhs), c.target]);
                groups.set(c.lhs, group);
            } else {
                plainConds.push(c);
            }
        });

        gtGroups.forEach(function (thresholds, lhs) {
            thresholds.sort(function (a, b) { return b[0] - a[0]; });
            emitThresholdChain(lhs, ">", thresholds);
        });

        ltGroups.forEach(function (thresholds, lhs) {
            thresholds.sort(function (a, b) { return a[0] - b[0]; });
            emitThresholdChain(lhs, "<", thresholds);
        });

        plainConds.forEach(function (c) {
            if (!c.op) {
                const ci = c.lhs; const ti = sanitizeId(c.target);
                addNode(ci, c.lhs, "decision"); addNode(ti, c.target, "step");
                addEdge(ci, ti, null);
            } else {
                const label = `${c.lhs} ${c.op} ${c.rhs}`;
                const ci = sanitizeId(label); const ti = sanitizeId(c.target);
                addNode(ci, label, "decision"); addNode(ti, c.target, "step");
                addEdge(ci, ti, "true");
            }
        });

        const matchGroups = new Map();
        matchArms.forEach(function (m) {
            const group = matchGroups.get(m.expr) || [];
            group.push(m);
            matchGroups.set(m.expr, group);
        });

        matchGroups.forEach(function (arms, expr) {
            const mi = "match_" + sanitizeId(expr);
            addNode(mi, "match " + expr, "match");
            arms.forEach(function (m, armIndex) {
                const ti = sanitizeId(m.target);
                const defaultArm = isDefaultArm(m.arm);
                addNode(ti, m.target, "step", { armIndex, isDefault: defaultArm });
                addEdge(mi, ti, m.arm, { armIndex, isDefault: defaultArm });
            });
        });

        return { graph, diagnostics };
    }

    function graphToMermaid(graph) {
        const lines = ["flowchart TD"];
        graph.order.forEach(function (id) {
            const n = graph.nodes.get(id); if (!n) return;
            const label = escapeLabel(n.label);
            if (n.kind === "decision" || n.kind === "match") lines.push(`    ${n.id}{"${label}"}`);
            else lines.push(`    ${n.id}["${label}"]`);
        });
        graph.edges.forEach(function (e) {
            const label = e.label && e.isDefault ? `${e.label} (default)` : e.label;
            if (label) lines.push(`    ${e.from} -->|"${escapeLabel(label)}"|${e.to}`);
            else lines.push(`    ${e.from} --> ${e.to}`);
        });
        return lines.join("\n");
    }

    function solveConstraintLayout(graph, options) {
        const settings = Object.assign({}, ISO_SETTINGS, options || {});
        const incoming = new Map(); graph.order.forEach(function (id) { incoming.set(id, []); });
        graph.edges.forEach(function (e) { if (incoming.has(e.to)) incoming.get(e.to).push(e); });
        const levels = new Map();
        graph.order.forEach(function (id) {
            const p = incoming.get(id);
            levels.set(id, p.length ? Math.max.apply(null, p.map(function (e) { return (levels.get(e.from) || 0) + 1; })) : 0);
        });
        const grouped = new Map();
        graph.order.forEach(function (id) {
            const l = levels.get(id) || 0; const list = grouped.get(l) || []; list.push(id); grouped.set(l, list);
        });
        const positions = new Map();
        grouped.forEach(function (ids, l) {
            const center = (ids.length - 1) / 2;
            ids.forEach(function (id, i) {
                const n = graph.nodes.get(id);
                const lift = (n.semanticType === "logic" || n.kind === "decision") ? settings.decisionLift : (n.semanticType === "human" ? settings.reviewLift : 0);
                positions.set(id, { x: l * settings.levelGap, y: lift, z: (i - center) * settings.laneGap });
            });
        });

        graph.order.forEach(function (id) {
            const node = graph.nodes.get(id);
            if (!node || node.kind !== "match") return;
            const matchPos = positions.get(id) || { x: 0, y: settings.decisionLift, z: 0 };
            const arms = graph.edges
                .filter(function (e) { return e.from === id; })
                .sort(function (a, b) { return (a.armIndex || 0) - (b.armIndex || 0); });
            arms.forEach(function (edge, index) {
                const target = graph.nodes.get(edge.to);
                if (!target) return;
                const lift = target.semanticType === "human" ? settings.reviewLift : (target.semanticType === "storage" ? settings.commitLift : 0);
                const armIndex = edge.armIndex == null ? index : edge.armIndex;
                positions.set(edge.to, {
                    x: matchPos.x + settings.levelGap,
                    y: lift,
                    z: matchPos.z + armIndex * settings.laneGap,
                });
            });
        });

        graph.order.forEach(function (id) {
            const parents = incoming.get(id) || [];
            if (parents.length < 2) return;
            const sourceMatchIds = parents
                .map(function (edge) { return (incoming.get(edge.from) || []).find(function (parentEdge) {
                    const parentNode = graph.nodes.get(parentEdge.from);
                    return parentNode && parentNode.kind === "match";
                }); })
                .filter(Boolean)
                .map(function (edge) { return edge.from; });
            if (!sourceMatchIds.length) return;
            const first = sourceMatchIds[0];
            if (!sourceMatchIds.every(function (matchId) { return matchId === first; })) return;
            const matchPos = positions.get(first);
            if (!matchPos) return;
            const widestParentX = Math.max.apply(null, parents.map(function (edge) {
                const pos = positions.get(edge.from);
                return pos ? pos.x : 0;
            }));
            const current = positions.get(id) || { y: 0 };
            positions.set(id, {
                x: widestParentX + settings.levelGap,
                y: current.y,
                z: matchPos.z,
            });
        });

        return { positions, levels, settings };
    }

    function isoProject(pt, scale, origin) {
        return { x: origin.x + (pt.x - pt.z) * scale * 0.866, y: origin.y + (pt.x + pt.z) * scale * 0.5 - pt.y * scale };
    }

    function base64Encode(raw) {
        if (typeof Buffer !== "undefined") return Buffer.from(raw, "utf8").toString("base64");
        return btoa(unescape(encodeURIComponent(raw)));
    }

    function autogeneratedModelUri(node) {
        const gltf = {
            asset: { version: "2.0", generator: "l3dg3rr-rhai-live-core" },
            scene: 0,
            scenes: [{ nodes: [0] }],
            nodes: [{ mesh: 0, name: node.id }],
            meshes: [{
                name: `${node.semanticType}-node`,
                primitives: [{
                    attributes: {},
                    mode: 4,
                    extras: { autogenerated: true, semanticType: node.semanticType },
                }],
            }],
        };
        return "data:model/gltf+json;base64," + base64Encode(JSON.stringify(gltf));
    }

    function buildVisualizationModel(graph, options) {
        const layout = solveConstraintLayout(graph, options);
        const s = layout.settings;
        const origin = { x: s.margin + s.cardWidth / 2, y: s.margin + 140 };
        const projected = graph.order.map(function (id) { return { id, screen: isoProject(layout.positions.get(id), s.scale, origin) }; });
        const minX = Math.min.apply(null, projected.map(function (e) { return e.screen.x - s.cardWidth; }));
        const minY = Math.min.apply(null, projected.map(function (e) { return e.screen.y - s.cardHeight; }));
        const maxX = Math.max.apply(null, projected.map(function (e) { return e.screen.x + s.cardWidth; }));
        const maxY = Math.max.apply(null, projected.map(function (e) { return e.screen.y + s.cardHeight + s.cardDepth; }));
        const offset = { x: s.margin - minX, y: s.margin - minY };

        const nodes = graph.order.map(function (id) {
            const n = graph.nodes.get(id); const pt = layout.positions.get(id);
            const meta = resolveSemanticMetadata(n.semanticType);
            const node = {
                id: n.id, label: n.label, semanticType: n.semanticType,
                x: pt.x, y: pt.y, z: pt.z,
                armIndex: n.armIndex, isDefault: n.isDefault,
                screen: isoProject(pt, s.scale, { x: origin.x + offset.x, y: origin.y + offset.y }),
                color: meta.color, emoji: meta.emoji, polygon: meta.polygon
            };
            node.modelUri = autogeneratedModelUri(node);
            return node;
        });

        const nodeById = new Map(nodes.map(function (n) { return [n.id, n]; }));
        const edges = graph.edges.map(function (e, i) {
            const f = nodeById.get(e.from); const t = nodeById.get(e.to);
            const midX = (f.screen.x + t.screen.x) / 2;
            const bend = Math.min(72, Math.max(32, Math.abs(t.screen.x - f.screen.x) * 0.22));
            const path = `M ${f.screen.x.toFixed(1)} ${f.screen.y.toFixed(1)} C ${(midX - bend).toFixed(1)} ${(f.screen.y + 12).toFixed(1)}, ${(midX + bend).toFixed(1)} ${(t.screen.y - 12).toFixed(1)}, ${t.screen.x.toFixed(1)} ${t.screen.y.toFixed(1)}`;
            return { id: `edge-${i}`, path, label: e.label, labelPoint: { x: midX, y: (f.screen.y + t.screen.y) / 2 - 14 } };
        });

        return { width: Math.ceil(maxX - minX + s.margin * 2), height: Math.ceil(maxY - minY + s.margin * 2), nodes, edges, settings: s };
    }

    function renderNodeIcon(emoji, size) {
        return `<text x="0" y="0" font-size="${size * 0.75}px" text-anchor="middle" dominant-baseline="central" font-family="'Apple Color Emoji', 'Segoe UI Emoji', 'Noto Color Emoji', sans-serif" stroke="none" fill="currentColor">${emoji}</text>`;
    }

    function tint(color, amount) {
        const rgb = [0, 2, 4].map(function (o) { return parseInt(color.replace("#", "").slice(o, o + 2), 16); });
        return "#" + rgb.map(function (v) { return Math.min(255, Math.max(0, Math.round(v + (255 - v) * amount))).toString(16).padStart(2, "0"); }).join("");
    }

    function darken(color, amount) {
        const rgb = [0, 2, 4].map(function (o) { return parseInt(color.replace("#", "").slice(o, o + 2), 16); });
        return "#" + rgb.map(function (v) { return Math.min(255, Math.max(0, Math.round(v * (1 - amount)))).toString(16).padStart(2, "0"); }).join("");
    }

    function renderIsometricNode(n, prev, s) {
        const dx = s.cardDepth * 0.86; const dy = s.cardDepth * -0.48;
        const pts = n.polygon.map(function (p) { return { x: p[0] * s.cardWidth * 0.5, y: p[1] * s.cardHeight * 0.5 }; });
        const fill = n.color; const stroke = darken(fill, 0.34);
        const depthFaces = pts.map(function (p1, i) {
            const p2 = pts[(i + 1) % pts.length]; const nx = p2.y - p1.y; const ny = -(p2.x - p1.x);
            if (nx * dx + ny * dy < 0) return "";
            return `<path d="M ${p1.x} ${p1.y} L ${p1.x + dx} ${p1.y + dy} L ${p2.x + dx} ${p2.y + dy} L ${p2.x} ${p2.y} Z" fill="${ny < 0 ? tint(fill, 0.18) : darken(fill, 0.16)}" stroke="${stroke}" stroke-width="1.2" />`;
        }).join("");
        const front = `<polygon points="${pts.map(function (p) { return `${p.x},${p.y}`; }).join(" ")}" fill="${tint(fill, 0.08)}" stroke="${stroke}" stroke-width="1.4" />`;
        const iconY = -2;
        const movement = prev && (prev.screen.x !== n.screen.x || prev.screen.y !== n.screen.y)
            ? `<animateTransform attributeName="transform" attributeType="XML" type="translate" from="${prev.screen.x.toFixed(1)} ${prev.screen.y.toFixed(1)}" to="${n.screen.x.toFixed(1)} ${n.screen.y.toFixed(1)}" dur="${s.animationMs}ms" begin="0s" fill="freeze" />`
            : "";

        return `<g class="rhai-iso-node" data-model-uri="${n.modelUri}" transform="translate(${n.screen.x.toFixed(1)} ${n.screen.y.toFixed(1)})">${movement}
            <g class="rhai-iso-volume">${depthFaces}${front}</g>
            <circle cx="0" cy="${iconY}" r="14" fill="${fill}" />
            <g class="rhai-iso-icon" fill="#f8fafc">
                <g transform="translate(0 ${iconY})">${renderNodeIcon(n.emoji, 24)}</g>
            </g>
            <text class="rhai-iso-label rhai-iso-label-center" x="0" y="${s.cardHeight / 2 + 18}">${escapeHtml(n.label)}</text>
            <text class="rhai-iso-subtitle rhai-iso-label-center" x="0" y="${s.cardHeight / 2 + 34}">${n.semanticType}</text>
        </g>`;
    }

    function sceneToIsometricSvg(scene, previousScene) {
        const previousById = new Map((previousScene && previousScene.nodes ? previousScene.nodes : []).map(function (n) { return [n.id, n]; }));
        const defs = `<defs><pattern id="rhai-iso-grid" width="48" height="24" patternUnits="userSpaceOnUse" patternTransform="skewX(-30)"><path d="M 0 0 L 0 24 M 0 0 L 48 0" stroke="rgba(15, 23, 42, 0.08)" stroke-width="1" fill="none" /></pattern><marker id="rhai-iso-arrow" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0,0 L9,4.5 L0,9 z" fill="#475569" /></marker></defs>`;
        const edges = scene.edges.map(function (e) {
            const labelWidth = e.label ? Math.max(60, String(e.label).length * 7 + 24) : 60;
            const label = e.label ? `<g class="rhai-iso-edge-label"><rect x="${(e.labelPoint.x - labelWidth / 2).toFixed(1)}" y="${(e.labelPoint.y - 11).toFixed(1)}" width="${labelWidth.toFixed(1)}" height="18" rx="9" fill="rgba(248,250,252,0.9)" /><text x="${e.labelPoint.x.toFixed(1)}" y="${(e.labelPoint.y + 1).toFixed(1)}" font-size="10" text-anchor="middle" dominant-baseline="middle">${escapeHtml(e.label)}</text></g>` : "";
            return `<g class="rhai-iso-edge"><path d="${e.path}" fill="none" stroke="#475569" stroke-width="2" marker-end="url(#rhai-iso-arrow)" />${label}</g>`;
        }).join("");
        const nodes = scene.nodes.map(function (n) { return renderIsometricNode(n, previousById.get(n.id), scene.settings); }).join("");
        return `<svg class="rhai-isometric-scene" viewBox="0 0 ${scene.width} ${scene.height}" xmlns="http://www.w3.org/2000/svg">${defs}<rect width="100%" height="100%" rx="24" fill="url(#rhai-iso-grid)" /><g class="rhai-iso-shadow-plane" transform="translate(24 ${scene.height - 96})"><path d="M0 24 L${scene.width - 96} 24 L${scene.width - 48} 0 L48 0 Z" fill="rgba(15,23,42,0.06)" /></g><g class="rhai-iso-layer-edges">${edges}</g><g class="rhai-iso-layer-nodes">${nodes}</g></svg>`;
    }

    function buildRenderFailure(error, viewMode) {
        const message = error && error.message ? error.message : "Unknown render failure.";
        if (viewMode === "mermaid-2d") {
            return {
                title: "Mermaid render failed",
                hint: "Switch to isometric-3d to keep inspecting the workflow while Mermaid is unavailable.",
                detail: message,
            };
        }
        return {
            title: "Isometric render failed",
            hint: "The graph parsed, but the isometric scene could not be generated. Mermaid remains available as a fallback.",
            detail: message,
        };
    }

    const DEFAULT_RHAI_MUTATION_MODEL = "phi-4-mini-reasoning";
    function normalizeInstruction(i) { return String(i || "").trim(); }

    function buildRhaiMutationPrompt(source, instruction, options) {
        const modelName = options && options.modelName ? String(options.modelName).trim() : DEFAULT_RHAI_MUTATION_MODEL;
        const request = normalizeInstruction(instruction) || "Add a review-safe classification branch.";
        return [`Model target: ${modelName}`, "", "You are editing the l3dg3rr documentation Rhai diagram DSL.", "Return a replacement DSL block first, then a concise explanation.", "Use only supported lines:", "- fn source() -> target", "- if expression -> target", "- match expr => Arm -> target", "", "Mutation request:", request, "", "Current DSL:", String(source || "").trim()].join("\n");
    }

    function draftRhaiMutationFromChat(source, instruction, options) {
        const text = String(source || "").trim();
        const request = normalizeInstruction(instruction).toLowerCase();
        const modelName = options && options.modelName ? String(options.modelName).trim() : DEFAULT_RHAI_MUTATION_MODEL;
        const lines = text ? text.split(/\r?\n/).filter(function (line) { return line.trim(); }) : [];
        let addition; let explanation;
        if (request.includes("xero")) {
            addition = ["fn reconcile_rows() -> xero_match", "if xero_match.confidence > 0.90 -> commit_workbook", "if xero_match.confidence <= 0.90 -> operator_review", "fn operator_review() -> commit_workbook"];
            explanation = "The draft inserts a supervised Xero reconciliation gate and keeps workbook commit behind either high-confidence match evidence or operator review.";
        } else if (request.includes("match") || request.includes("disposition")) {
            addition = ["fn verify_result() -> match_disposition", "match result.disposition => Disposition::Unrecoverable -> halt_pipeline", "match result.disposition => Disposition::Recoverable -> repair_and_retry", "match result.disposition => Disposition::Advisory -> record_note"];
            explanation = "The draft turns validation disposition into explicit match arms, which makes halt, repair, and advisory paths visible in both Mermaid and isometric views.";
        } else {
            addition = ["if confidence > 0.85 -> commit_workbook", "if confidence > 0.60 -> review_flag", "if confidence <= 0.60 -> escalate_operator", "fn review_flag() -> commit_workbook"];
            explanation = "The draft adds a medium-confidence review lane and keeps low-confidence classifications out of commit until an operator handles the escalation.";
        }
        const merged = lines.concat(addition.filter(function (line) { return !lines.includes(line); })).join("\n");
        return { modelName, source: merged, explanation, prompt: buildRhaiMutationPrompt(source, instruction, { modelName }) };
    }

    return { sanitizeId, escapeHtml, parseRhaiDiagram, graphToMermaid, buildVisualizationModel, sceneToIsometricSvg, buildRenderFailure, buildRhaiMutationPrompt, draftRhaiMutationFromChat, isoProject };
});
