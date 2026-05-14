// main-legacy.js
function invoke(cmd, args) {
  var api = window.__TAURI__;
  if (!api) return Promise.reject(new Error("no __TAURI__"));
  if (!api.core) return Promise.reject(new Error("no .core"));
  return api.core.invoke(cmd, args);
}
function listen(e, h) {
  var api = window.__TAURI__;
  if (!api) return Promise.reject(new Error("no __TAURI__"));
  return api.event.listen(e, h);
}
var PANELS = [
  { id: "chat", icon: "AI", label: "Chat" },
  { id: "logs", icon: "LG", label: "Logs" },
  { id: "dash", icon: "DB", label: "Dashboard" },
  { id: "settings", icon: "ST", label: "Settings" },
  { id: "docs", icon: "DK", label: "Docs Playbook" },
  { id: "viz", icon: "VZ", label: "Viz" }
];
var activePanel = 0;
var DASH_PANEL_INDEX = PANELS.findIndex(function(p) {
  return p.id === "dash";
});
var VIZ_PANEL_INDEX = PANELS.findIndex(function(p) {
  return p.id === "viz";
});
var _vizInitialized = false;
var _vizAllElements = [];
var _vizActiveGraph = "type";
var VIZ_FIT_PADDING = 72;
function showPanel(i) {
  activePanel = i;
  PANELS.forEach(function(p, j) {
    var el = document.getElementById("panel-" + p.id);
    if (el) el.classList.toggle("hidden", j !== i);
  });
  document.querySelectorAll(".nav-item[data-panel-index]").forEach(function(b, j) {
    b.classList.toggle("active", j === i);
  });
  if (DASH_PANEL_INDEX !== -1 && i === DASH_PANEL_INDEX) refreshDashboard();
  if (VIZ_PANEL_INDEX !== -1 && i === VIZ_PANEL_INDEX) initVizPanel();
}
function panelTemplate(id) {
  var t = {};
  t.chat = '<div class="panel-header"><span class="panel-title">Chat</span><div id="model-badge" class="model-badge phi"><span id="model-badge-icon">&#9889;</span><span id="model-badge-text">No model</span></div></div><div class="model-bar"><span class="model-bar-label">Model:</span><button id="pill-phi" class="model-pill">&#9889; Phi-4</button><button id="pill-foundry" class="model-pill">Windows AI</button><button id="pill-cloud" class="model-pill">&#9729; Cloud</button><span id="cloud-hint" class="cloud-hint hidden">edit in Settings</span></div><div id="transcript-wrap" class="transcript-wrap"><div class="log-label">Transcript</div><div id="transcript" class="transcript-content"></div></div><div class="input-area"><textarea id="draft-input" rows="5"></textarea><div class="input-actions"><button id="send-btn">Send</button><button id="rhai-btn">Rhai Rule</button></div></div>';
  t.logs = '<div class="panel-title-row"><span class="panel-title">Logs</span></div><div class="log-tabs"><button class="log-tab active" data-log="0">Transport</button><button class="log-tab" data-log="1">Review</button></div><div id="log-panel-0" class="log-subpanel transport-bg"><div class="log-label">Transport</div><div id="rig-log" class="log-content"></div></div><div id="log-panel-1" class="log-subpanel review-bg hidden"><div class="log-label review-label">Diffsets</div><div id="review-log" class="log-content"></div></div></div>';
  t.dash = '<span class="panel-title">Dashboard</span><div id="evidence-summary" class="evidence-summary"><div class="ev-card ev-card-blocked"><div class="ev-card-value" id="blocked-value">-</div><div class="ev-card-label">Blocked</div></div><div class="ev-card ev-card-ready"><div class="ev-card-value" id="ready-value">-</div><div class="ev-card-label">Ready</div></div><div class="ev-card ev-card-exported"><div class="ev-card-value" id="exported-value">-</div><div class="ev-card-label">Exported</div></div><div class="ev-card ev-card-issues"><div class="ev-card-value" id="issues-value">-</div><div class="ev-card-label">Issues</div></div></div><div class="ev-section"><div class="ev-section-title">Last Action</div><div id="ev-last-action" class="ev-last-action">Loading...</div></div><div class="ev-section"><div class="ev-section-title">Next Actions</div><ul id="ev-next-actions" class="ev-next-actions"></ul></div><div class="ev-section"><div class="ev-section-title">Providers</div><div id="ev-provider-status" class="ev-provider-status">Loading...</div></div><div class="ev-refresh-row"><button id="btn-refresh-dashboard">Refresh</button></div>';
  t.settings = '<span class="panel-title">Settings</span><label class="field-label" for="input-endpoint">Endpoint</label><input id="input-endpoint" type="text" class="field-input"/><label class="field-label" for="input-model">Model</label><input id="input-model" type="text" class="field-input"/><label class="field-label" for="input-api-key">Key</label><input id="input-api-key" type="text" class="field-input"/><label class="field-label" for="input-system-prompt">System Prompt</label><textarea id="input-system-prompt" class="field-input system-prompt-area" rows="6"></textarea><div class="settings-actions"><button id="btn-use-phi">Use Phi-4</button><button id="btn-use-foundry">Use Win AI</button><button id="btn-use-cloud">Use Cloud</button><button id="btn-save-settings">Save</button></div>';
  t.viz = '<div class="panel-title-row viz-title-row"><div class="viz-tabs"><button id="btn-viz-tab-type" class="viz-tab active">Type Graph</button><button id="btn-viz-tab-pipeline" class="viz-tab">Pipeline</button></div><span class="panel-title">Ontology Viz</span><div class="viz-toolbar"><button id="btn-viz-zoom-out" title="Zoom out">-</button><button id="btn-viz-zoom-in" title="Zoom in">+</button><button id="btn-viz-fit" title="Fit graph">Fit</button><button id="btn-viz-reset" title="Reset zoom">1:1</button><button id="btn-viz-layout" title="Run layout">Layout</button><button id="btn-viz-labels" title="Toggle node labels">Labels</button><button id="btn-viz-edge-labels" title="Toggle relationship labels">Edges</button><input id="viz-search" class="viz-search" type="search" placeholder="Find type"/><select id="viz-edge-filter" class="viz-select"><option value="">All relations</option></select><button id="btn-viz-clear" title="Clear filters">Clear</button><button id="btn-viz-refresh" title="Reload graph">Refresh</button></div></div><div class="viz-body"><div id="cy" class="viz-canvas"></div><aside id="viz-detail" class="viz-detail"><div class="viz-detail-title">Selection</div><div id="viz-detail-body" class="viz-detail-body">Select a node or relationship.</div></aside></div>';
  t.docs = '<span class="panel-title">Docs Playbook</span><p id="docs-status-text" class="docs-status"></p><div class="docs-actions"><button id="btn-open-docs">Open Docs</button><button id="btn-load-rhai-mutation">Load Rhai</button></div><div class="docs-preview-wrap"><div id="docs-rig-log" class="log-content"></div></div>';
  return t[id] || "";
}
function buildUI() {
  try {
    var nav = document.getElementById("nav-items");
    var pc = document.getElementById("panel-container");
    if (!nav || !pc) return;
    PANELS.forEach(function(p, i) {
      var btn = document.createElement("button");
      btn.className = "nav-item";
      btn.dataset.panelIndex = i;
      btn.innerHTML = '<span class="mark">' + p.icon + '</span><span class="label">' + p.label + "</span>";
      (function(idx) {
        btn.addEventListener("click", function() {
          showPanel(idx);
        });
      })(i);
      nav.appendChild(btn);
      var div = document.createElement("div");
      div.id = "panel-" + p.id;
      div.className = "panel card" + (i === 0 ? "" : " hidden");
      if (p.id === "settings") div.classList.add("settings-bg");
      div.innerHTML = panelTemplate(p.id);
      pc.appendChild(div);
    });
    showPanel(0);
  } catch (e) {
    console.error("[ui] buildUI err:", e);
  }
}
function readinessLabel(r) {
  if (!r) return "Unknown";
  if (r === "ready") return "Ready";
  if (r.setup_needed) return "Setup needed";
  if (r.unavailable) return "Unavailable";
  if (r.diagnostic) return "Diagnostic";
  return String(r);
}
function setTextSafe(el, text) {
  if (el) el.textContent = text != null ? String(text) : "";
}
function refreshDashboard() {
  var api = window.__TAURI__;
  if (!api) return;
  api.core.invoke("get_evidence_dashboard").then(function(p) {
    var q = p.today_queue || {};
    setTextSafe(document.getElementById("blocked-value"), q.blocked ?? "-");
    setTextSafe(document.getElementById("ready-value"), q.ready_to_review ?? "-");
    setTextSafe(document.getElementById("exported-value"), q.exported ?? "-");
    setTextSafe(document.getElementById("issues-value"), q.with_validation_issues ?? "-");
    setTextSafe(document.getElementById("ev-last-action"), q.last_action_summary ?? "");
    var na = document.getElementById("ev-next-actions");
    if (na) {
      na.innerHTML = "";
      (q.next_actions || []).forEach(function(a) {
        var li = document.createElement("li");
        li.textContent = a;
        na.appendChild(li);
      });
    }
    var ps = document.getElementById("ev-provider-status");
    if (ps) {
      ps.innerHTML = "";
      (q.providers || []).forEach(function(prov) {
        var d = document.createElement("div");
        d.className = "ev-provider-line";
        d.textContent = `${prov.display_name || prov.label}: ${readinessLabel(prov.readiness)}`;
        ps.appendChild(d);
      });
    }
  }).catch(function(err) {
    var sb = document.getElementById("status-bar");
    if (sb) sb.textContent = "Dashboard refresh failed: " + (err && err.message || err || "unknown error");
  });
}
function setVal(id, v) {
  var el = document.getElementById(id);
  if (el) el.value = v != null ? String(v) : "";
}
function updateModelBadge(model, apiKey) {
  var isPhi = apiKey === "local-tool-tray";
  var isFoundry = apiKey === "local-foundry";
  var badge = document.getElementById("model-badge");
  var icon = document.getElementById("model-badge-icon");
  var text = document.getElementById("model-badge-text");
  if (!badge) return;
  badge.className = "model-badge " + (isPhi ? "phi" : isFoundry ? "foundry" : "cloud");
  if (icon) icon.textContent = isPhi ? "\u26A1" : isFoundry ? "WA" : "\u2601";
  if (text) text.textContent = model || "No model \u2014 go to Settings";
  var pillPhi = document.getElementById("pill-phi");
  var pillFoundry = document.getElementById("pill-foundry");
  var pillCloud = document.getElementById("pill-cloud");
  if (pillPhi) pillPhi.classList.toggle("active", isPhi);
  if (pillFoundry) pillFoundry.classList.toggle("active", isFoundry);
  if (pillCloud) pillCloud.classList.toggle("active", !isPhi && !isFoundry && model !== "");
  var ch = document.getElementById("cloud-hint");
  if (ch) ch.classList.toggle("hidden", isPhi || isFoundry || model === "");
}
function setBusy(busy) {
  var sb = document.getElementById("send-btn");
  if (sb) sb.disabled = busy;
  if (sb) sb.textContent = busy ? "Sending\u2026" : "Send";
  [
    "draft-input",
    "rhai-btn",
    "pill-phi",
    "pill-foundry",
    "pill-cloud",
    "btn-use-phi",
    "btn-use-foundry",
    "btn-use-cloud",
    "btn-open-docs",
    "btn-load-rhai-mutation"
  ].forEach(function(id) {
    var el = document.getElementById(id);
    if (el) el.disabled = busy;
  });
  ["input-endpoint", "input-model", "input-api-key", "input-system-prompt"].forEach(function(id) {
    var el = document.getElementById(id);
    if (el) el.disabled = busy;
  });
  var saveBtn = document.getElementById("btn-save-settings");
  if (saveBtn) saveBtn.textContent = busy ? "Working\u2026" : "Save";
}
function applySettings(p) {
  setVal("input-endpoint", p.endpoint_text);
  setVal("input-model", p.model_text);
  setVal("input-api-key", p.api_key_text);
  setVal("input-system-prompt", p.system_prompt_text);
  setTextSafe(document.getElementById("status-bar"), p.status_text);
  updateModelBadge(p.model_text, p.api_key_text);
}
document.addEventListener("DOMContentLoaded", function() {
  buildUI();
  invoke("get_initial_state").then(function(s) {
    setTextSafe(document.getElementById("version-text"), s.version_text);
    setTextSafe(document.getElementById("status-bar"), s.status_text);
    setVal("input-endpoint", s.endpoint_text);
    setVal("input-model", s.model_text);
    setVal("input-api-key", s.api_key_text);
    setVal("input-system-prompt", s.system_prompt_text);
    setTextSafe(document.getElementById("transcript"), s.transcript_text);
    setTextSafe(document.getElementById("rig-log"), s.rig_log_text);
    setTextSafe(document.getElementById("review-log"), s.review_log_text);
    setVal("draft-input", s.draft_message_text);
    setTextSafe(document.getElementById("docs-status-text"), s.docs_status_text);
    updateModelBadge(s.model_text, s.api_key_text);
  }).catch(function() {
  });
  listen("chat-update", function(ev) {
    var d = ev.payload;
    setTextSafe(document.getElementById("transcript"), d.transcript_text);
    setTextSafe(document.getElementById("rig-log"), d.rig_log_text);
    if (d.review_log_text != null) setTextSafe(document.getElementById("review-log"), d.review_log_text);
    setVal("draft-input", d.draft_message_text);
    setTextSafe(document.getElementById("status-bar"), d.status_text);
    setBusy(!!d.busy);
  }).catch(function() {
  });
  var colBtn = document.getElementById("collapse-btn");
  if (colBtn) colBtn.addEventListener("click", function() {
    var sb = document.getElementById("sidebar");
    if (!sb) return;
    var collapsed = sb.classList.toggle("collapsed");
    var mark = colBtn.querySelector(".mark");
    if (mark) mark.textContent = collapsed ? ">" : "<";
  });
  refreshDashboard();
  var dr = document.getElementById("btn-refresh-dashboard");
  if (dr) dr.addEventListener("click", refreshDashboard);
  var sendBtn = document.getElementById("send-btn");
  if (sendBtn) sendBtn.addEventListener("click", function() {
    invoke("send_message", {
      draft: document.getElementById("draft-input")?.value || "",
      endpoint: document.getElementById("input-endpoint")?.value || "",
      model: document.getElementById("input-model")?.value || "",
      apiKey: document.getElementById("input-api-key")?.value || "",
      systemPrompt: document.getElementById("input-system-prompt")?.value || ""
    }).then(function(s) {
      setTextSafe(document.getElementById("status-bar"), s);
    }).catch(function(e) {
      setTextSafe(document.getElementById("status-bar"), "Send failed: " + (e && e.message || e || "unknown"));
    });
  });
  var rhaiBtn = document.getElementById("rhai-btn");
  if (rhaiBtn) rhaiBtn.addEventListener("click", function() {
    invoke("load_rhai_rule_prompt", {
      currentModel: document.getElementById("input-model")?.value || "",
      currentSystemPrompt: document.getElementById("input-system-prompt")?.value || ""
    }).then(function(p) {
      setVal("input-system-prompt", p.system_prompt);
      if (p.suggested_model) setVal("input-model", p.suggested_model);
      setVal("draft-input", p.draft_message);
      setTextSafe(document.getElementById("review-log"), p.review_log_text);
      setTextSafe(document.getElementById("status-bar"), p.status);
    }).catch(function() {
    });
  });
  var pillPhi = document.getElementById("pill-phi");
  if (pillPhi) pillPhi.addEventListener("click", function() {
    invoke("use_internal_phi", { systemPrompt: document.getElementById("input-system-prompt")?.value || "" }).then(applySettings).catch(function() {
    });
  });
  var pillFoundry = document.getElementById("pill-foundry");
  if (pillFoundry) pillFoundry.addEventListener("click", function() {
    invoke("use_foundry_local", { systemPrompt: document.getElementById("input-system-prompt")?.value || "" }).then(applySettings).catch(function() {
    });
  });
  var pillCloud = document.getElementById("pill-cloud");
  if (pillCloud) pillCloud.addEventListener("click", function() {
    invoke("use_cloud_model", { systemPrompt: document.getElementById("input-system-prompt")?.value || "" }).then(applySettings).catch(function() {
      setTextSafe(document.getElementById("cloud-hint"), "edit endpoint/key in Settings");
      var ch = document.getElementById("cloud-hint");
      if (ch) ch.classList.remove("hidden");
    });
  });
  var usePhi = document.getElementById("btn-use-phi");
  if (usePhi) usePhi.addEventListener("click", function() {
    invoke("use_internal_phi", { systemPrompt: document.getElementById("input-system-prompt")?.value || "" }).then(applySettings).catch(function() {
    });
  });
  var useFoundry = document.getElementById("btn-use-foundry");
  if (useFoundry) useFoundry.addEventListener("click", function() {
    invoke("use_foundry_local", { systemPrompt: document.getElementById("input-system-prompt")?.value || "" }).then(applySettings).catch(function() {
    });
  });
  var useCloud = document.getElementById("btn-use-cloud");
  if (useCloud) useCloud.addEventListener("click", function() {
    invoke("use_cloud_model", { systemPrompt: document.getElementById("input-system-prompt")?.value || "" }).then(applySettings).catch(function() {
    });
  });
  var sf = document.getElementById("btn-save-settings");
  if (sf) sf.addEventListener("click", function() {
    invoke("save_settings", {
      endpoint: document.getElementById("input-endpoint")?.value || "",
      model: document.getElementById("input-model")?.value || "",
      apiKey: document.getElementById("input-api-key")?.value || "",
      systemPrompt: document.getElementById("input-system-prompt")?.value || ""
    }).then(function(s) {
      setTextSafe(document.getElementById("status-bar"), s);
    }).catch(function() {
    });
  });
  var od = document.getElementById("btn-open-docs");
  if (od) od.addEventListener("click", function() {
    invoke("open_docs_playbook").then(function(s) {
      setTextSafe(document.getElementById("docs-status-text"), s);
      setTextSafe(document.getElementById("docs-rig-log"), s);
    }).catch(function() {
    });
  });
  var lr = document.getElementById("btn-load-rhai-mutation");
  if (lr) lr.addEventListener("click", function() {
    var chatIdx = PANELS.findIndex(function(p) {
      return p.id === "chat";
    });
    if (chatIdx !== -1) showPanel(chatIdx);
    invoke("load_rhai_rule_prompt", {
      currentModel: document.getElementById("input-model")?.value || "",
      currentSystemPrompt: document.getElementById("input-system-prompt")?.value || ""
    }).then(function(p) {
      setTextSafe(document.getElementById("docs-rig-log"), p.review_log_text);
      setTextSafe(document.getElementById("docs-status-text"), p.status);
      setVal("draft-input", p.draft_message);
      setVal("input-system-prompt", p.system_prompt);
      if (p.suggested_model) setVal("input-model", p.suggested_model);
    }).catch(function() {
    });
  });
  document.querySelectorAll(".log-tab").forEach(function(tab) {
    tab.addEventListener("click", function() {
      var idx = tab.dataset.log;
      document.querySelectorAll(".log-tab").forEach(function(t) {
        t.classList.remove("active");
      });
      tab.classList.add("active");
      document.getElementById("log-panel-0").classList.toggle("hidden", idx !== "0");
      document.getElementById("log-panel-1").classList.toggle("hidden", idx !== "1");
    });
  });
});
function initVizPanel() {
  if (_vizInitialized) return;
  var cy_div = document.getElementById("cy");
  if (!cy_div || typeof cytoscape === "undefined") return;
  var graphCmd = _vizActiveGraph === "type" ? "get_type_graph" : "get_holon_viz_graph";
  invoke(graphCmd).then(function(data) {
    var elements = [];
    (data.nodes || []).forEach(function(n) {
      elements.push({ data: n.data });
    });
    (data.edges || []).forEach(function(e) {
      elements.push({ data: e.data });
    });
    _vizAllElements = elements;
    window._cy = cytoscape({
      container: cy_div,
      elements,
      minZoom: 0.18,
      maxZoom: 3,
      layout: { name: "dagre", rankDir: "TB", nodeSep: 50, rankSep: 70, animate: false },
      style: [
        { selector: "node", style: {
          "label": "data(label)",
          "background-color": "#1a6fa8",
          "color": "#fff",
          "text-valign": "center",
          "text-halign": "center",
          "font-size": "11px",
          "width": "label",
          "height": "label",
          "padding": "8px",
          "shape": "roundrectangle",
          "border-width": 1,
          "border-color": "#0b4f71"
        } },
        { selector: "edge", style: {
          "curve-style": "bezier",
          "target-arrow-shape": "triangle",
          "line-color": "#6f8794",
          "target-arrow-color": "#6f8794",
          "width": 1.5
        } },
        { selector: ".faded", style: { "opacity": 0.18, "text-opacity": 0.12 } },
        { selector: ".hidden-filter", style: { "display": "none" } },
        { selector: ".matched", style: { "border-width": 3, "border-color": "#f28c28", "z-index": 999 } },
        { selector: ".hide-label", style: { "label": "" } },
        { selector: ":selected", style: { "border-width": 3, "border-color": "#f28c28", "line-color": "#f28c28", "target-arrow-color": "#f28c28" } },
        { selector: 'node[kind="CapsuleGroup"]', style: { "background-color": "#5a3e8a" } },
        { selector: 'node[kind="AuditEvent"]', style: { "background-color": "#7a3030" } },
        { selector: 'node[kind="OwlClass"]', style: { "background-color": "#2e6e45" } },
        { selector: 'node[kind="trait"]', style: { "background-color": "#5a3e8a", "shape": "hexagon" } },
        { selector: 'node[kind="enum"]', style: { "background-color": "#2e6e45", "shape": "diamond" } },
        { selector: 'node[kind="mcp_tool"]', style: { "background-color": "#8a6b1f", "shape": "tag" } },
        { selector: 'node[kind="tauri_command"]', style: { "background-color": "#7a3030", "shape": "roundrectangle" } },
        { selector: 'node[kind="abstract_trait"]', style: { "background-color": "#003b5c", "shape": "hexagon" } },
        { selector: 'node[kind="contract_type"],node[kind="dsl_contract"]', style: { "background-color": "#005d7f", "shape": "roundrectangle" } },
        { selector: 'node[kind="metamodel_enum"],node[kind="ontology_enum"]', style: { "background-color": "#007c89", "shape": "diamond" } },
        { selector: 'node[kind="z_document"]', style: { "background-color": "#5f7480", "shape": "roundrectangle" } },
        { selector: 'node[kind="z_pipeline"],node[kind="pipeline_state"]', style: { "background-color": "#0073a8", "shape": "roundrectangle" } },
        { selector: 'node[kind="z_constraint"],node[kind="constraint_type"]', style: { "background-color": "#00a0af", "shape": "roundrectangle" } },
        { selector: 'node[kind="z_legal"],node[kind="legal_type"]', style: { "background-color": "#c3482f", "shape": "roundrectangle" } },
        { selector: 'node[kind="z_proof"],node[kind="proof_result"]', style: { "background-color": "#00856f", "shape": "roundrectangle" } },
        { selector: 'node[kind="z_attestation"],node[kind="attestation_type"]', style: { "background-color": "#f28c28", "shape": "roundrectangle", "color": "#172b3a" } },
        { selector: 'node[kind="solver_type"]', style: { "background-color": "#00856f", "shape": "barrel" } },
        { selector: 'node[kind="result_type"]', style: { "background-color": "#0097a9", "shape": "round-diamond" } },
        { selector: 'node[kind="issue_type"],node[kind="review_state"]', style: { "background-color": "#c3482f", "shape": "octagon" } },
        { selector: 'node[kind="gate_type"]', style: { "background-color": "#f28c28", "shape": "vee", "color": "#172b3a" } },
        { selector: 'node[kind="evidence_graph"],node[kind="evidence_node"]', style: { "background-color": "#6f8794", "shape": "roundrectangle" } },
        { selector: 'node[kind="workbook_projection"]', style: { "background-color": "#5aa646", "shape": "tag" } },
        { selector: 'node[kind="taxonomy_type"]', style: { "background-color": "#7fbf3f", "shape": "diamond", "color": "#172b3a" } },
        { selector: 'node[kind="workflow_type"]', style: { "background-color": "#005d7f", "shape": "rhomboid" } },
        { selector: 'node[z_layer="Pipeline"]', style: { "background-color": "#0073a8" } },
        { selector: 'node[z_layer="Constraint"]', style: { "background-color": "#00a0af" } },
        { selector: 'node[z_layer="Legal"]', style: { "background-color": "#c3482f" } },
        { selector: 'node[z_layer="FormalProof"]', style: { "background-color": "#00856f" } },
        { selector: 'node[z_layer="Attestation"]', style: { "background-color": "#f28c28", "color": "#172b3a" } },
        { selector: 'node[z_layer="Document"]', style: { "background-color": "#5f7480" } },
        { selector: "edge", style: { "label": "data(label)", "font-size": "9px", "color": "#173b4a", "text-background-color": "#ffffff", "text-background-opacity": 0.92, "text-background-padding": "2px" } }
      ]
    });
    _vizInitialized = true;
    setupVizControls();
    setVizDetail(null);
    window._cy.ready(function() {
      setTimeout(function() {
        if (window._cy) window._cy.fit(window._cy.elements().not(".hidden-filter"), VIZ_FIT_PADDING);
      }, 300);
    });
    var btn = document.getElementById("btn-viz-refresh");
    if (btn) btn.addEventListener("click", function() {
      _vizInitialized = false;
      window._cy && window._cy.destroy();
      initVizPanel();
    });
  }).catch(function(e) {
    console.error("[viz] " + graphCmd + " failed:", e);
  });
}
function runVizLayout() {
  if (!window._cy) return;
  var layout = window._cy.layout({ name: "dagre", rankDir: "TB", nodeSep: 50, rankSep: 70, animate: false });
  window._cy.one("layoutstop", function() {
    window._cy.fit(window._cy.elements().not(".hidden-filter"), VIZ_FIT_PADDING);
  });
  layout.run();
}
function zoomVizBy(factor) {
  var cy = window._cy;
  if (!cy) return;
  cy.zoom({ level: cy.zoom() * factor, renderedPosition: { x: cy.width() / 2, y: cy.height() / 2 } });
}
function populateVizFilters() {
  var edgeSel = document.getElementById("viz-edge-filter");
  if (!edgeSel || !window._cy) return;
  edgeSel.innerHTML = '<option value="">All relations</option>';
  var labels = {};
  window._cy.edges().forEach(function(e) {
    labels[e.data("label")] = true;
  });
  Object.keys(labels).sort().forEach(function(l) {
    if (!l) return;
    var o = document.createElement("option");
    o.value = l;
    o.textContent = l;
    edgeSel.appendChild(o);
  });
}
function applyVizFilters() {
  var cy = window._cy;
  if (!cy) return;
  var query = (document.getElementById("viz-search")?.value || "").toLowerCase().trim();
  var edgeLabel = document.getElementById("viz-edge-filter")?.value || "";
  cy.elements().removeClass("hidden-filter matched faded");
  cy.nodes().forEach(function(n) {
    var label = String(n.data("label") || "").toLowerCase();
    var id = String(n.data("id") || "").toLowerCase();
    var searchOk = !query || label.indexOf(query) !== -1 || id.indexOf(query) !== -1;
    if (!searchOk) n.addClass("hidden-filter");
    else if (query) n.addClass("matched");
  });
  cy.edges().forEach(function(e) {
    if (edgeLabel && e.data("label") !== edgeLabel) e.addClass("hidden-filter");
    if (e.source().hasClass("hidden-filter") || e.target().hasClass("hidden-filter")) e.addClass("hidden-filter");
  });
  var visible = cy.elements().not(".hidden-filter");
  if (query || edgeLabel) {
    cy.elements().not(visible).addClass("faded");
    if (visible.length > 0) cy.fit(visible, VIZ_FIT_PADDING);
  }
}
function setVizDetail(ele) {
  var body = document.getElementById("viz-detail-body");
  if (!body) return;
  if (!ele) {
    body.textContent = "Select a node or relationship.";
    return;
  }
  if (ele.isNode && ele.isNode()) {
    body.innerHTML = "<div><b>" + escapeHtml(ele.data("label") || "") + "</b></div><div>" + escapeHtml(ele.data("id") || "") + '</div><div class="viz-detail-chip">' + escapeHtml(ele.data("kind") || "") + "</div>";
  } else {
    body.innerHTML = "<div><b>" + escapeHtml(ele.data("label") || "relationship") + "</b></div><div>" + escapeHtml(ele.data("source") || "") + "</div><div>\u2192</div><div>" + escapeHtml(ele.data("target") || "") + "</div>";
  }
}
function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, function(c) {
    return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
  });
}
function setupVizControls() {
  var cy = window._cy;
  if (!cy) return;
  populateVizFilters();
  document.getElementById("btn-viz-zoom-in")?.addEventListener("click", function() {
    zoomVizBy(1.2);
  });
  document.getElementById("btn-viz-zoom-out")?.addEventListener("click", function() {
    zoomVizBy(0.83);
  });
  document.getElementById("btn-viz-fit")?.addEventListener("click", function() {
    cy.fit(cy.elements().not(".hidden-filter"), VIZ_FIT_PADDING);
  });
  document.getElementById("btn-viz-reset")?.addEventListener("click", function() {
    cy.zoom(1);
    cy.center();
  });
  document.getElementById("btn-viz-layout")?.addEventListener("click", function() {
    runVizLayout();
  });
  document.getElementById("btn-viz-labels")?.addEventListener("click", function() {
    cy.nodes().toggleClass("hide-label");
  });
  document.getElementById("btn-viz-edge-labels")?.addEventListener("click", function() {
    cy.edges().toggleClass("hide-label");
  });
  document.getElementById("viz-search")?.addEventListener("input", applyVizFilters);
  document.getElementById("viz-edge-filter")?.addEventListener("change", applyVizFilters);
  document.getElementById("btn-viz-clear")?.addEventListener("click", function() {
    var s = document.getElementById("viz-search");
    if (s) s.value = "";
    var e = document.getElementById("viz-edge-filter");
    if (e) e.value = "";
    applyVizFilters();
    cy.fit(void 0, VIZ_FIT_PADDING);
  });
  var tabType = document.getElementById("btn-viz-tab-type");
  var tabPipeline = document.getElementById("btn-viz-tab-pipeline");
  if (tabType) tabType.addEventListener("click", function() {
    if (_vizActiveGraph === "type") return;
    _vizActiveGraph = "type";
    tabType.classList.add("active");
    if (tabPipeline) tabPipeline.classList.remove("active");
    _vizInitialized = false;
    window._cy && window._cy.destroy();
    initVizPanel();
  });
  if (tabPipeline) tabPipeline.addEventListener("click", function() {
    if (_vizActiveGraph === "pipeline") return;
    _vizActiveGraph = "pipeline";
    tabPipeline.classList.add("active");
    if (tabType) tabType.classList.remove("active");
    _vizInitialized = false;
    window._cy && window._cy.destroy();
    initVizPanel();
  });
  cy.on("tap", "node,edge", function(evt) {
    setVizDetail(evt.target);
  });
  cy.on("tap", function(evt) {
    if (evt.target === cy) setVizDetail(null);
  });
}
//# sourceMappingURL=data:application/json;base64,ewogICJ2ZXJzaW9uIjogMywKICAic291cmNlcyI6IFsibWFpbi1sZWdhY3kuanMiXSwKICAic291cmNlc0NvbnRlbnQiOiBbImZ1bmN0aW9uIHRhdXJpQXBpKCl7cmV0dXJuIHdpbmRvdy5fX1RBVVJJX199XG5mdW5jdGlvbiBpbnZva2UoY21kLGFyZ3Mpe3ZhciBhcGk9d2luZG93Ll9fVEFVUklfXztpZighYXBpKXJldHVybiBQcm9taXNlLnJlamVjdChuZXcgRXJyb3IoJ25vIF9fVEFVUklfXycpKTtpZighYXBpLmNvcmUpcmV0dXJuIFByb21pc2UucmVqZWN0KG5ldyBFcnJvcignbm8gLmNvcmUnKSk7cmV0dXJuIGFwaS5jb3JlLmludm9rZShjbWQsYXJncyl9XG5mdW5jdGlvbiBsaXN0ZW4oZSxoKXt2YXIgYXBpPXdpbmRvdy5fX1RBVVJJX187aWYoIWFwaSlyZXR1cm4gUHJvbWlzZS5yZWplY3QobmV3IEVycm9yKCdubyBfX1RBVVJJX18nKSk7cmV0dXJuIGFwaS5ldmVudC5saXN0ZW4oZSxoKX1cblxudmFyIFBBTkVMUz1bXG4gIHtpZDonY2hhdCcsaWNvbjonQUknLGxhYmVsOidDaGF0J30sXG4gIHtpZDonbG9ncycsaWNvbjonTEcnLGxhYmVsOidMb2dzJ30sXG4gIHtpZDonZGFzaCcsaWNvbjonREInLGxhYmVsOidEYXNoYm9hcmQnfSxcbiAge2lkOidzZXR0aW5ncycsaWNvbjonU1QnLGxhYmVsOidTZXR0aW5ncyd9LFxuICB7aWQ6J2RvY3MnLGljb246J0RLJyxsYWJlbDonRG9jcyBQbGF5Ym9vayd9LFxuICB7aWQ6J3ZpeicsaWNvbjonVlonLGxhYmVsOidWaXonfSxcbl07XG52YXIgYWN0aXZlUGFuZWw9MDtcbnZhciBEQVNIX1BBTkVMX0lOREVYPVBBTkVMUy5maW5kSW5kZXgoZnVuY3Rpb24ocCl7cmV0dXJuIHAuaWQ9PT0nZGFzaCd9KTtcbnZhciBWSVpfUEFORUxfSU5ERVg9UEFORUxTLmZpbmRJbmRleChmdW5jdGlvbihwKXtyZXR1cm4gcC5pZD09PSd2aXonfSk7XG52YXIgX3ZpekluaXRpYWxpemVkPWZhbHNlO1xudmFyIF92aXpBbGxFbGVtZW50cz1bXTtcbnZhciBfdml6QWN0aXZlR3JhcGg9J3R5cGUnOyAvLyAndHlwZScgfCAncGlwZWxpbmUnXG52YXIgVklaX0ZJVF9QQURESU5HPTcyO1xuXG5mdW5jdGlvbiBzaG93UGFuZWwoaSl7XG4gIGFjdGl2ZVBhbmVsPWk7XG4gIFBBTkVMUy5mb3JFYWNoKGZ1bmN0aW9uKHAsail7XG4gICAgdmFyIGVsPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdwYW5lbC0nK3AuaWQpO1xuICAgIGlmKGVsKWVsLmNsYXNzTGlzdC50b2dnbGUoJ2hpZGRlbicsaiE9PWkpO1xuICB9KTtcbiAgZG9jdW1lbnQucXVlcnlTZWxlY3RvckFsbCgnLm5hdi1pdGVtW2RhdGEtcGFuZWwtaW5kZXhdJykuZm9yRWFjaChmdW5jdGlvbihiLGope1xuICAgIGIuY2xhc3NMaXN0LnRvZ2dsZSgnYWN0aXZlJyxqPT09aSk7XG4gIH0pO1xuICBpZihEQVNIX1BBTkVMX0lOREVYIT09LTEmJmk9PT1EQVNIX1BBTkVMX0lOREVYKXJlZnJlc2hEYXNoYm9hcmQoKTtcbiAgaWYoVklaX1BBTkVMX0lOREVYIT09LTEmJmk9PT1WSVpfUEFORUxfSU5ERVgpaW5pdFZpelBhbmVsKCk7XG59XG5cbmZ1bmN0aW9uIHBhbmVsVGVtcGxhdGUoaWQpe1xuICB2YXIgdD17fVxuICB0LmNoYXQ9JzxkaXYgY2xhc3M9XCJwYW5lbC1oZWFkZXJcIj48c3BhbiBjbGFzcz1cInBhbmVsLXRpdGxlXCI+Q2hhdDwvc3Bhbj48ZGl2IGlkPVwibW9kZWwtYmFkZ2VcIiBjbGFzcz1cIm1vZGVsLWJhZGdlIHBoaVwiPjxzcGFuIGlkPVwibW9kZWwtYmFkZ2UtaWNvblwiPiYjOTg4OTs8L3NwYW4+PHNwYW4gaWQ9XCJtb2RlbC1iYWRnZS10ZXh0XCI+Tm8gbW9kZWw8L3NwYW4+PC9kaXY+PC9kaXY+PGRpdiBjbGFzcz1cIm1vZGVsLWJhclwiPjxzcGFuIGNsYXNzPVwibW9kZWwtYmFyLWxhYmVsXCI+TW9kZWw6PC9zcGFuPjxidXR0b24gaWQ9XCJwaWxsLXBoaVwiIGNsYXNzPVwibW9kZWwtcGlsbFwiPiYjOTg4OTsgUGhpLTQ8L2J1dHRvbj48YnV0dG9uIGlkPVwicGlsbC1mb3VuZHJ5XCIgY2xhc3M9XCJtb2RlbC1waWxsXCI+V2luZG93cyBBSTwvYnV0dG9uPjxidXR0b24gaWQ9XCJwaWxsLWNsb3VkXCIgY2xhc3M9XCJtb2RlbC1waWxsXCI+JiM5NzI5OyBDbG91ZDwvYnV0dG9uPjxzcGFuIGlkPVwiY2xvdWQtaGludFwiIGNsYXNzPVwiY2xvdWQtaGludCBoaWRkZW5cIj5lZGl0IGluIFNldHRpbmdzPC9zcGFuPjwvZGl2PjxkaXYgaWQ9XCJ0cmFuc2NyaXB0LXdyYXBcIiBjbGFzcz1cInRyYW5zY3JpcHQtd3JhcFwiPjxkaXYgY2xhc3M9XCJsb2ctbGFiZWxcIj5UcmFuc2NyaXB0PC9kaXY+PGRpdiBpZD1cInRyYW5zY3JpcHRcIiBjbGFzcz1cInRyYW5zY3JpcHQtY29udGVudFwiPjwvZGl2PjwvZGl2PjxkaXYgY2xhc3M9XCJpbnB1dC1hcmVhXCI+PHRleHRhcmVhIGlkPVwiZHJhZnQtaW5wdXRcIiByb3dzPVwiNVwiPjwvdGV4dGFyZWE+PGRpdiBjbGFzcz1cImlucHV0LWFjdGlvbnNcIj48YnV0dG9uIGlkPVwic2VuZC1idG5cIj5TZW5kPC9idXR0b24+PGJ1dHRvbiBpZD1cInJoYWktYnRuXCI+UmhhaSBSdWxlPC9idXR0b24+PC9kaXY+PC9kaXY+JztcbiAgdC5sb2dzPSc8ZGl2IGNsYXNzPVwicGFuZWwtdGl0bGUtcm93XCI+PHNwYW4gY2xhc3M9XCJwYW5lbC10aXRsZVwiPkxvZ3M8L3NwYW4+PC9kaXY+PGRpdiBjbGFzcz1cImxvZy10YWJzXCI+PGJ1dHRvbiBjbGFzcz1cImxvZy10YWIgYWN0aXZlXCIgZGF0YS1sb2c9XCIwXCI+VHJhbnNwb3J0PC9idXR0b24+PGJ1dHRvbiBjbGFzcz1cImxvZy10YWJcIiBkYXRhLWxvZz1cIjFcIj5SZXZpZXc8L2J1dHRvbj48L2Rpdj48ZGl2IGlkPVwibG9nLXBhbmVsLTBcIiBjbGFzcz1cImxvZy1zdWJwYW5lbCB0cmFuc3BvcnQtYmdcIj48ZGl2IGNsYXNzPVwibG9nLWxhYmVsXCI+VHJhbnNwb3J0PC9kaXY+PGRpdiBpZD1cInJpZy1sb2dcIiBjbGFzcz1cImxvZy1jb250ZW50XCI+PC9kaXY+PC9kaXY+PGRpdiBpZD1cImxvZy1wYW5lbC0xXCIgY2xhc3M9XCJsb2ctc3VicGFuZWwgcmV2aWV3LWJnIGhpZGRlblwiPjxkaXYgY2xhc3M9XCJsb2ctbGFiZWwgcmV2aWV3LWxhYmVsXCI+RGlmZnNldHM8L2Rpdj48ZGl2IGlkPVwicmV2aWV3LWxvZ1wiIGNsYXNzPVwibG9nLWNvbnRlbnRcIj48L2Rpdj48L2Rpdj48L2Rpdj4nO1xuICB0LmRhc2g9JzxzcGFuIGNsYXNzPVwicGFuZWwtdGl0bGVcIj5EYXNoYm9hcmQ8L3NwYW4+PGRpdiBpZD1cImV2aWRlbmNlLXN1bW1hcnlcIiBjbGFzcz1cImV2aWRlbmNlLXN1bW1hcnlcIj48ZGl2IGNsYXNzPVwiZXYtY2FyZCBldi1jYXJkLWJsb2NrZWRcIj48ZGl2IGNsYXNzPVwiZXYtY2FyZC12YWx1ZVwiIGlkPVwiYmxvY2tlZC12YWx1ZVwiPi08L2Rpdj48ZGl2IGNsYXNzPVwiZXYtY2FyZC1sYWJlbFwiPkJsb2NrZWQ8L2Rpdj48L2Rpdj48ZGl2IGNsYXNzPVwiZXYtY2FyZCBldi1jYXJkLXJlYWR5XCI+PGRpdiBjbGFzcz1cImV2LWNhcmQtdmFsdWVcIiBpZD1cInJlYWR5LXZhbHVlXCI+LTwvZGl2PjxkaXYgY2xhc3M9XCJldi1jYXJkLWxhYmVsXCI+UmVhZHk8L2Rpdj48L2Rpdj48ZGl2IGNsYXNzPVwiZXYtY2FyZCBldi1jYXJkLWV4cG9ydGVkXCI+PGRpdiBjbGFzcz1cImV2LWNhcmQtdmFsdWVcIiBpZD1cImV4cG9ydGVkLXZhbHVlXCI+LTwvZGl2PjxkaXYgY2xhc3M9XCJldi1jYXJkLWxhYmVsXCI+RXhwb3J0ZWQ8L2Rpdj48L2Rpdj48ZGl2IGNsYXNzPVwiZXYtY2FyZCBldi1jYXJkLWlzc3Vlc1wiPjxkaXYgY2xhc3M9XCJldi1jYXJkLXZhbHVlXCIgaWQ9XCJpc3N1ZXMtdmFsdWVcIj4tPC9kaXY+PGRpdiBjbGFzcz1cImV2LWNhcmQtbGFiZWxcIj5Jc3N1ZXM8L2Rpdj48L2Rpdj48L2Rpdj48ZGl2IGNsYXNzPVwiZXYtc2VjdGlvblwiPjxkaXYgY2xhc3M9XCJldi1zZWN0aW9uLXRpdGxlXCI+TGFzdCBBY3Rpb248L2Rpdj48ZGl2IGlkPVwiZXYtbGFzdC1hY3Rpb25cIiBjbGFzcz1cImV2LWxhc3QtYWN0aW9uXCI+TG9hZGluZy4uLjwvZGl2PjwvZGl2PjxkaXYgY2xhc3M9XCJldi1zZWN0aW9uXCI+PGRpdiBjbGFzcz1cImV2LXNlY3Rpb24tdGl0bGVcIj5OZXh0IEFjdGlvbnM8L2Rpdj48dWwgaWQ9XCJldi1uZXh0LWFjdGlvbnNcIiBjbGFzcz1cImV2LW5leHQtYWN0aW9uc1wiPjwvdWw+PC9kaXY+PGRpdiBjbGFzcz1cImV2LXNlY3Rpb25cIj48ZGl2IGNsYXNzPVwiZXYtc2VjdGlvbi10aXRsZVwiPlByb3ZpZGVyczwvZGl2PjxkaXYgaWQ9XCJldi1wcm92aWRlci1zdGF0dXNcIiBjbGFzcz1cImV2LXByb3ZpZGVyLXN0YXR1c1wiPkxvYWRpbmcuLi48L2Rpdj48L2Rpdj48ZGl2IGNsYXNzPVwiZXYtcmVmcmVzaC1yb3dcIj48YnV0dG9uIGlkPVwiYnRuLXJlZnJlc2gtZGFzaGJvYXJkXCI+UmVmcmVzaDwvYnV0dG9uPjwvZGl2Pic7XG4gIHQuc2V0dGluZ3M9JzxzcGFuIGNsYXNzPVwicGFuZWwtdGl0bGVcIj5TZXR0aW5nczwvc3Bhbj48bGFiZWwgY2xhc3M9XCJmaWVsZC1sYWJlbFwiIGZvcj1cImlucHV0LWVuZHBvaW50XCI+RW5kcG9pbnQ8L2xhYmVsPjxpbnB1dCBpZD1cImlucHV0LWVuZHBvaW50XCIgdHlwZT1cInRleHRcIiBjbGFzcz1cImZpZWxkLWlucHV0XCIvPjxsYWJlbCBjbGFzcz1cImZpZWxkLWxhYmVsXCIgZm9yPVwiaW5wdXQtbW9kZWxcIj5Nb2RlbDwvbGFiZWw+PGlucHV0IGlkPVwiaW5wdXQtbW9kZWxcIiB0eXBlPVwidGV4dFwiIGNsYXNzPVwiZmllbGQtaW5wdXRcIi8+PGxhYmVsIGNsYXNzPVwiZmllbGQtbGFiZWxcIiBmb3I9XCJpbnB1dC1hcGkta2V5XCI+S2V5PC9sYWJlbD48aW5wdXQgaWQ9XCJpbnB1dC1hcGkta2V5XCIgdHlwZT1cInRleHRcIiBjbGFzcz1cImZpZWxkLWlucHV0XCIvPjxsYWJlbCBjbGFzcz1cImZpZWxkLWxhYmVsXCIgZm9yPVwiaW5wdXQtc3lzdGVtLXByb21wdFwiPlN5c3RlbSBQcm9tcHQ8L2xhYmVsPjx0ZXh0YXJlYSBpZD1cImlucHV0LXN5c3RlbS1wcm9tcHRcIiBjbGFzcz1cImZpZWxkLWlucHV0IHN5c3RlbS1wcm9tcHQtYXJlYVwiIHJvd3M9XCI2XCI+PC90ZXh0YXJlYT48ZGl2IGNsYXNzPVwic2V0dGluZ3MtYWN0aW9uc1wiPjxidXR0b24gaWQ9XCJidG4tdXNlLXBoaVwiPlVzZSBQaGktNDwvYnV0dG9uPjxidXR0b24gaWQ9XCJidG4tdXNlLWZvdW5kcnlcIj5Vc2UgV2luIEFJPC9idXR0b24+PGJ1dHRvbiBpZD1cImJ0bi11c2UtY2xvdWRcIj5Vc2UgQ2xvdWQ8L2J1dHRvbj48YnV0dG9uIGlkPVwiYnRuLXNhdmUtc2V0dGluZ3NcIj5TYXZlPC9idXR0b24+PC9kaXY+JztcbiAgdC52aXo9JzxkaXYgY2xhc3M9XCJwYW5lbC10aXRsZS1yb3cgdml6LXRpdGxlLXJvd1wiPjxkaXYgY2xhc3M9XCJ2aXotdGFic1wiPjxidXR0b24gaWQ9XCJidG4tdml6LXRhYi10eXBlXCIgY2xhc3M9XCJ2aXotdGFiIGFjdGl2ZVwiPlR5cGUgR3JhcGg8L2J1dHRvbj48YnV0dG9uIGlkPVwiYnRuLXZpei10YWItcGlwZWxpbmVcIiBjbGFzcz1cInZpei10YWJcIj5QaXBlbGluZTwvYnV0dG9uPjwvZGl2PjxzcGFuIGNsYXNzPVwicGFuZWwtdGl0bGVcIj5PbnRvbG9neSBWaXo8L3NwYW4+PGRpdiBjbGFzcz1cInZpei10b29sYmFyXCI+PGJ1dHRvbiBpZD1cImJ0bi12aXotem9vbS1vdXRcIiB0aXRsZT1cIlpvb20gb3V0XCI+LTwvYnV0dG9uPjxidXR0b24gaWQ9XCJidG4tdml6LXpvb20taW5cIiB0aXRsZT1cIlpvb20gaW5cIj4rPC9idXR0b24+PGJ1dHRvbiBpZD1cImJ0bi12aXotZml0XCIgdGl0bGU9XCJGaXQgZ3JhcGhcIj5GaXQ8L2J1dHRvbj48YnV0dG9uIGlkPVwiYnRuLXZpei1yZXNldFwiIHRpdGxlPVwiUmVzZXQgem9vbVwiPjE6MTwvYnV0dG9uPjxidXR0b24gaWQ9XCJidG4tdml6LWxheW91dFwiIHRpdGxlPVwiUnVuIGxheW91dFwiPkxheW91dDwvYnV0dG9uPjxidXR0b24gaWQ9XCJidG4tdml6LWxhYmVsc1wiIHRpdGxlPVwiVG9nZ2xlIG5vZGUgbGFiZWxzXCI+TGFiZWxzPC9idXR0b24+PGJ1dHRvbiBpZD1cImJ0bi12aXotZWRnZS1sYWJlbHNcIiB0aXRsZT1cIlRvZ2dsZSByZWxhdGlvbnNoaXAgbGFiZWxzXCI+RWRnZXM8L2J1dHRvbj48aW5wdXQgaWQ9XCJ2aXotc2VhcmNoXCIgY2xhc3M9XCJ2aXotc2VhcmNoXCIgdHlwZT1cInNlYXJjaFwiIHBsYWNlaG9sZGVyPVwiRmluZCB0eXBlXCIvPjxzZWxlY3QgaWQ9XCJ2aXotZWRnZS1maWx0ZXJcIiBjbGFzcz1cInZpei1zZWxlY3RcIj48b3B0aW9uIHZhbHVlPVwiXCI+QWxsIHJlbGF0aW9uczwvb3B0aW9uPjwvc2VsZWN0PjxidXR0b24gaWQ9XCJidG4tdml6LWNsZWFyXCIgdGl0bGU9XCJDbGVhciBmaWx0ZXJzXCI+Q2xlYXI8L2J1dHRvbj48YnV0dG9uIGlkPVwiYnRuLXZpei1yZWZyZXNoXCIgdGl0bGU9XCJSZWxvYWQgZ3JhcGhcIj5SZWZyZXNoPC9idXR0b24+PC9kaXY+PC9kaXY+PGRpdiBjbGFzcz1cInZpei1ib2R5XCI+PGRpdiBpZD1cImN5XCIgY2xhc3M9XCJ2aXotY2FudmFzXCI+PC9kaXY+PGFzaWRlIGlkPVwidml6LWRldGFpbFwiIGNsYXNzPVwidml6LWRldGFpbFwiPjxkaXYgY2xhc3M9XCJ2aXotZGV0YWlsLXRpdGxlXCI+U2VsZWN0aW9uPC9kaXY+PGRpdiBpZD1cInZpei1kZXRhaWwtYm9keVwiIGNsYXNzPVwidml6LWRldGFpbC1ib2R5XCI+U2VsZWN0IGEgbm9kZSBvciByZWxhdGlvbnNoaXAuPC9kaXY+PC9hc2lkZT48L2Rpdj4nO1xuICB0LmRvY3M9JzxzcGFuIGNsYXNzPVwicGFuZWwtdGl0bGVcIj5Eb2NzIFBsYXlib29rPC9zcGFuPjxwIGlkPVwiZG9jcy1zdGF0dXMtdGV4dFwiIGNsYXNzPVwiZG9jcy1zdGF0dXNcIj48L3A+PGRpdiBjbGFzcz1cImRvY3MtYWN0aW9uc1wiPjxidXR0b24gaWQ9XCJidG4tb3Blbi1kb2NzXCI+T3BlbiBEb2NzPC9idXR0b24+PGJ1dHRvbiBpZD1cImJ0bi1sb2FkLXJoYWktbXV0YXRpb25cIj5Mb2FkIFJoYWk8L2J1dHRvbj48L2Rpdj48ZGl2IGNsYXNzPVwiZG9jcy1wcmV2aWV3LXdyYXBcIj48ZGl2IGlkPVwiZG9jcy1yaWctbG9nXCIgY2xhc3M9XCJsb2ctY29udGVudFwiPjwvZGl2PjwvZGl2Pic7XG4gIHJldHVybiB0W2lkXXx8Jyc7XG59XG5cbmZ1bmN0aW9uIGJ1aWxkVUkoKXtcbiAgdHJ5e1xuICAgIHZhciBuYXY9ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ25hdi1pdGVtcycpO1xuICAgIHZhciBwYz1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgncGFuZWwtY29udGFpbmVyJyk7XG4gICAgaWYoIW5hdnx8IXBjKXJldHVybjtcbiAgICBQQU5FTFMuZm9yRWFjaChmdW5jdGlvbihwLGkpe1xuICAgICAgdmFyIGJ0bj1kb2N1bWVudC5jcmVhdGVFbGVtZW50KCdidXR0b24nKTtidG4uY2xhc3NOYW1lPSduYXYtaXRlbSc7YnRuLmRhdGFzZXQucGFuZWxJbmRleD1pO1xuICAgICAgYnRuLmlubmVySFRNTD0nPHNwYW4gY2xhc3M9XCJtYXJrXCI+JytwLmljb24rJzwvc3Bhbj48c3BhbiBjbGFzcz1cImxhYmVsXCI+JytwLmxhYmVsKyc8L3NwYW4+JztcbiAgICAgIChmdW5jdGlvbihpZHgpe2J0bi5hZGRFdmVudExpc3RlbmVyKCdjbGljaycsZnVuY3Rpb24oKXtzaG93UGFuZWwoaWR4KTt9KTt9KShpKTtcbiAgICAgIG5hdi5hcHBlbmRDaGlsZChidG4pO1xuICAgICAgdmFyIGRpdj1kb2N1bWVudC5jcmVhdGVFbGVtZW50KCdkaXYnKTtkaXYuaWQ9J3BhbmVsLScrcC5pZDtcbiAgICAgIGRpdi5jbGFzc05hbWU9J3BhbmVsIGNhcmQnKyhpPT09MD8nJzonIGhpZGRlbicpO1xuICAgICAgaWYocC5pZD09PSdzZXR0aW5ncycpZGl2LmNsYXNzTGlzdC5hZGQoJ3NldHRpbmdzLWJnJyk7XG4gICAgICBkaXYuaW5uZXJIVE1MPXBhbmVsVGVtcGxhdGUocC5pZCk7XG4gICAgICBwYy5hcHBlbmRDaGlsZChkaXYpO1xuICAgIH0pO1xuICAgIHNob3dQYW5lbCgwKTtcbiAgfWNhdGNoKGUpe2NvbnNvbGUuZXJyb3IoJ1t1aV0gYnVpbGRVSSBlcnI6JyxlKX1cbn1cblxuZnVuY3Rpb24gcmVhZGluZXNzTGFiZWwocil7XG4gIGlmKCFyKXJldHVybidVbmtub3duJztcbiAgaWYocj09PSdyZWFkeScpcmV0dXJuJ1JlYWR5JztcbiAgaWYoci5zZXR1cF9uZWVkZWQpcmV0dXJuJ1NldHVwIG5lZWRlZCc7XG4gIGlmKHIudW5hdmFpbGFibGUpcmV0dXJuJ1VuYXZhaWxhYmxlJztcbiAgaWYoci5kaWFnbm9zdGljKXJldHVybidEaWFnbm9zdGljJztcbiAgcmV0dXJuIFN0cmluZyhyKTtcbn1cblxuZnVuY3Rpb24gc2V0VGV4dFNhZmUoZWwsdGV4dCl7XG4gIGlmKGVsKWVsLnRleHRDb250ZW50PXRleHQhPW51bGw/U3RyaW5nKHRleHQpOicnO1xufVxuXG5mdW5jdGlvbiByZWZyZXNoRGFzaGJvYXJkKCl7XG4gIHZhciBhcGk9d2luZG93Ll9fVEFVUklfXztcbiAgaWYoIWFwaSlyZXR1cm47XG4gIGFwaS5jb3JlLmludm9rZSgnZ2V0X2V2aWRlbmNlX2Rhc2hib2FyZCcpLnRoZW4oZnVuY3Rpb24ocCl7XG4gICAgdmFyIHE9cC50b2RheV9xdWV1ZXx8e307XG4gICAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2Jsb2NrZWQtdmFsdWUnKSxxLmJsb2NrZWQ/PyctJyk7XG4gICAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3JlYWR5LXZhbHVlJykscS5yZWFkeV90b19yZXZpZXc/PyctJyk7XG4gICAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2V4cG9ydGVkLXZhbHVlJykscS5leHBvcnRlZD8/Jy0nKTtcbiAgICBzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnaXNzdWVzLXZhbHVlJykscS53aXRoX3ZhbGlkYXRpb25faXNzdWVzPz8nLScpO1xuICAgIHNldFRleHRTYWZlKGRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdldi1sYXN0LWFjdGlvbicpLHEubGFzdF9hY3Rpb25fc3VtbWFyeT8/JycpO1xuICAgIHZhciBuYT1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnZXYtbmV4dC1hY3Rpb25zJyk7XG4gICAgaWYobmEpe1xuICAgICAgbmEuaW5uZXJIVE1MPScnO1xuICAgICAgKHEubmV4dF9hY3Rpb25zfHxbXSkuZm9yRWFjaChmdW5jdGlvbihhKXtcbiAgICAgICAgdmFyIGxpPWRvY3VtZW50LmNyZWF0ZUVsZW1lbnQoJ2xpJyk7XG4gICAgICAgIGxpLnRleHRDb250ZW50PWE7XG4gICAgICAgIG5hLmFwcGVuZENoaWxkKGxpKTtcbiAgICAgIH0pO1xuICAgIH1cbiAgICB2YXIgcHM9ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2V2LXByb3ZpZGVyLXN0YXR1cycpO1xuICAgIGlmKHBzKXtcbiAgICAgIHBzLmlubmVySFRNTD0nJztcbiAgICAgIChxLnByb3ZpZGVyc3x8W10pLmZvckVhY2goZnVuY3Rpb24ocHJvdil7XG4gICAgICAgIHZhciBkPWRvY3VtZW50LmNyZWF0ZUVsZW1lbnQoJ2RpdicpO1xuICAgICAgICBkLmNsYXNzTmFtZT0nZXYtcHJvdmlkZXItbGluZSc7XG4gICAgICAgIGQudGV4dENvbnRlbnQ9YCR7cHJvdi5kaXNwbGF5X25hbWV8fHByb3YubGFiZWx9OiAke3JlYWRpbmVzc0xhYmVsKHByb3YucmVhZGluZXNzKX1gO1xuICAgICAgICBwcy5hcHBlbmRDaGlsZChkKTtcbiAgICAgIH0pO1xuICAgIH1cbiAgfSkuY2F0Y2goZnVuY3Rpb24oZXJyKXtcbiAgICB2YXIgc2I9ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3N0YXR1cy1iYXInKTtcbiAgICBpZihzYilzYi50ZXh0Q29udGVudD0nRGFzaGJvYXJkIHJlZnJlc2ggZmFpbGVkOiAnKyhlcnImJmVyci5tZXNzYWdlfHxlcnJ8fCd1bmtub3duIGVycm9yJyk7XG4gIH0pO1xufVxuXG5mdW5jdGlvbiBzZXRWYWwoaWQsdil7dmFyIGVsPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKGlkKTtpZihlbCllbC52YWx1ZT12IT1udWxsP1N0cmluZyh2KTonJzt9XG5cbmZ1bmN0aW9uIHVwZGF0ZU1vZGVsQmFkZ2UobW9kZWwsYXBpS2V5KXtcbiAgdmFyIGlzUGhpPWFwaUtleT09PSdsb2NhbC10b29sLXRyYXknO1xuICB2YXIgaXNGb3VuZHJ5PWFwaUtleT09PSdsb2NhbC1mb3VuZHJ5JztcbiAgdmFyIGJhZGdlPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdtb2RlbC1iYWRnZScpO1xuICB2YXIgaWNvbj1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnbW9kZWwtYmFkZ2UtaWNvbicpO1xuICB2YXIgdGV4dD1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnbW9kZWwtYmFkZ2UtdGV4dCcpO1xuICBpZighYmFkZ2UpcmV0dXJuO1xuICBiYWRnZS5jbGFzc05hbWU9J21vZGVsLWJhZGdlICcrKGlzUGhpPydwaGknOmlzRm91bmRyeT8nZm91bmRyeSc6J2Nsb3VkJyk7XG4gIGlmKGljb24paWNvbi50ZXh0Q29udGVudD1pc1BoaT8nXHUyNkExJzppc0ZvdW5kcnk/J1dBJzonXHUyNjAxJztcbiAgaWYodGV4dCl0ZXh0LnRleHRDb250ZW50PW1vZGVsfHwnTm8gbW9kZWwgXHUyMDE0IGdvIHRvIFNldHRpbmdzJztcbiAgLy8gVXBkYXRlIHBpbGwgYWN0aXZlIHN0YXRlc1xuICB2YXIgcGlsbFBoaT1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgncGlsbC1waGknKTtcbiAgdmFyIHBpbGxGb3VuZHJ5PWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdwaWxsLWZvdW5kcnknKTtcbiAgdmFyIHBpbGxDbG91ZD1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgncGlsbC1jbG91ZCcpO1xuICBpZihwaWxsUGhpKXBpbGxQaGkuY2xhc3NMaXN0LnRvZ2dsZSgnYWN0aXZlJyxpc1BoaSk7XG4gIGlmKHBpbGxGb3VuZHJ5KXBpbGxGb3VuZHJ5LmNsYXNzTGlzdC50b2dnbGUoJ2FjdGl2ZScsaXNGb3VuZHJ5KTtcbiAgaWYocGlsbENsb3VkKXBpbGxDbG91ZC5jbGFzc0xpc3QudG9nZ2xlKCdhY3RpdmUnLCFpc1BoaSYmIWlzRm91bmRyeSYmbW9kZWwhPT0nJyk7XG4gIC8vIENsb3VkIGhpbnQ6IHNob3cgd2hlbiBjbG91ZCBpcyBhY3RpdmVcbiAgdmFyIGNoPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdjbG91ZC1oaW50Jyk7XG4gIGlmKGNoKWNoLmNsYXNzTGlzdC50b2dnbGUoJ2hpZGRlbicsaXNQaGl8fGlzRm91bmRyeXx8bW9kZWw9PT0nJyk7XG59XG5cbmZ1bmN0aW9uIHNldEJ1c3koYnVzeSl7XG4gIHZhciBzYj1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnc2VuZC1idG4nKTtpZihzYilzYi5kaXNhYmxlZD1idXN5O1xuICBpZihzYilzYi50ZXh0Q29udGVudD1idXN5PydTZW5kaW5nXHUyMDI2JzonU2VuZCc7XG4gIFsnZHJhZnQtaW5wdXQnLCdyaGFpLWJ0bicsJ3BpbGwtcGhpJywncGlsbC1mb3VuZHJ5JywncGlsbC1jbG91ZCcsXG4gICAnYnRuLXVzZS1waGknLCdidG4tdXNlLWZvdW5kcnknLCdidG4tdXNlLWNsb3VkJyxcbiAgICdidG4tb3Blbi1kb2NzJywnYnRuLWxvYWQtcmhhaS1tdXRhdGlvbiddLmZvckVhY2goZnVuY3Rpb24oaWQpe1xuICAgIHZhciBlbD1kb2N1bWVudC5nZXRFbGVtZW50QnlJZChpZCk7aWYoZWwpZWwuZGlzYWJsZWQ9YnVzeTtcbiAgfSk7XG4gIFsnaW5wdXQtZW5kcG9pbnQnLCdpbnB1dC1tb2RlbCcsJ2lucHV0LWFwaS1rZXknLCdpbnB1dC1zeXN0ZW0tcHJvbXB0J10uZm9yRWFjaChmdW5jdGlvbihpZCl7XG4gICAgdmFyIGVsPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKGlkKTtpZihlbCllbC5kaXNhYmxlZD1idXN5O1xuICB9KTtcbiAgdmFyIHNhdmVCdG49ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2J0bi1zYXZlLXNldHRpbmdzJyk7XG4gIGlmKHNhdmVCdG4pc2F2ZUJ0bi50ZXh0Q29udGVudD1idXN5PydXb3JraW5nXHUyMDI2JzonU2F2ZSc7XG59XG5cbmZ1bmN0aW9uIGFwcGx5U2V0dGluZ3MocCl7XG4gIHNldFZhbCgnaW5wdXQtZW5kcG9pbnQnLHAuZW5kcG9pbnRfdGV4dCk7XG4gIHNldFZhbCgnaW5wdXQtbW9kZWwnLHAubW9kZWxfdGV4dCk7XG4gIHNldFZhbCgnaW5wdXQtYXBpLWtleScscC5hcGlfa2V5X3RleHQpO1xuICBzZXRWYWwoJ2lucHV0LXN5c3RlbS1wcm9tcHQnLHAuc3lzdGVtX3Byb21wdF90ZXh0KTtcbiAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3N0YXR1cy1iYXInKSxwLnN0YXR1c190ZXh0KTtcbiAgdXBkYXRlTW9kZWxCYWRnZShwLm1vZGVsX3RleHQscC5hcGlfa2V5X3RleHQpO1xufVxuXG5kb2N1bWVudC5hZGRFdmVudExpc3RlbmVyKCdET01Db250ZW50TG9hZGVkJyxmdW5jdGlvbigpe1xuICBidWlsZFVJKCk7XG5cbiAgLy8gUG9wdWxhdGUgaW5pdGlhbCBzdGF0ZSBmcm9tIGJhY2tlbmRcbiAgaW52b2tlKCdnZXRfaW5pdGlhbF9zdGF0ZScpLnRoZW4oZnVuY3Rpb24ocyl7XG4gICAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3ZlcnNpb24tdGV4dCcpLHMudmVyc2lvbl90ZXh0KTtcbiAgICBzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnc3RhdHVzLWJhcicpLHMuc3RhdHVzX3RleHQpO1xuICAgIHNldFZhbCgnaW5wdXQtZW5kcG9pbnQnLHMuZW5kcG9pbnRfdGV4dCk7XG4gICAgc2V0VmFsKCdpbnB1dC1tb2RlbCcscy5tb2RlbF90ZXh0KTtcbiAgICBzZXRWYWwoJ2lucHV0LWFwaS1rZXknLHMuYXBpX2tleV90ZXh0KTtcbiAgICBzZXRWYWwoJ2lucHV0LXN5c3RlbS1wcm9tcHQnLHMuc3lzdGVtX3Byb21wdF90ZXh0KTtcbiAgICBzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgndHJhbnNjcmlwdCcpLHMudHJhbnNjcmlwdF90ZXh0KTtcbiAgICBzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgncmlnLWxvZycpLHMucmlnX2xvZ190ZXh0KTtcbiAgICBzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgncmV2aWV3LWxvZycpLHMucmV2aWV3X2xvZ190ZXh0KTtcbiAgICBzZXRWYWwoJ2RyYWZ0LWlucHV0JyxzLmRyYWZ0X21lc3NhZ2VfdGV4dCk7XG4gICAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2RvY3Mtc3RhdHVzLXRleHQnKSxzLmRvY3Nfc3RhdHVzX3RleHQpO1xuICAgIHVwZGF0ZU1vZGVsQmFkZ2Uocy5tb2RlbF90ZXh0LHMuYXBpX2tleV90ZXh0KTtcbiAgfSkuY2F0Y2goZnVuY3Rpb24oKXt9KTtcblxuICAvLyBMaXN0ZW4gZm9yIGNoYXQtdXBkYXRlIGV2ZW50cyBmcm9tIHNlbmRfbWVzc2FnZVxuICBsaXN0ZW4oJ2NoYXQtdXBkYXRlJyxmdW5jdGlvbihldil7XG4gICAgdmFyIGQ9ZXYucGF5bG9hZDtcbiAgICBzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgndHJhbnNjcmlwdCcpLGQudHJhbnNjcmlwdF90ZXh0KTtcbiAgICBzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgncmlnLWxvZycpLGQucmlnX2xvZ190ZXh0KTtcbiAgICBpZihkLnJldmlld19sb2dfdGV4dCE9bnVsbClzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgncmV2aWV3LWxvZycpLGQucmV2aWV3X2xvZ190ZXh0KTtcbiAgICBzZXRWYWwoJ2RyYWZ0LWlucHV0JyxkLmRyYWZ0X21lc3NhZ2VfdGV4dCk7XG4gICAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3N0YXR1cy1iYXInKSxkLnN0YXR1c190ZXh0KTtcbiAgICBzZXRCdXN5KCEhZC5idXN5KTtcbiAgfSkuY2F0Y2goZnVuY3Rpb24oKXt9KTtcblxuICAvLyBTaWRlYmFyIGNvbGxhcHNlXG4gIHZhciBjb2xCdG49ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2NvbGxhcHNlLWJ0bicpO1xuICBpZihjb2xCdG4pY29sQnRuLmFkZEV2ZW50TGlzdGVuZXIoJ2NsaWNrJyxmdW5jdGlvbigpe1xuICAgIHZhciBzYj1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnc2lkZWJhcicpO1xuICAgIGlmKCFzYilyZXR1cm47XG4gICAgdmFyIGNvbGxhcHNlZD1zYi5jbGFzc0xpc3QudG9nZ2xlKCdjb2xsYXBzZWQnKTtcbiAgICB2YXIgbWFyaz1jb2xCdG4ucXVlcnlTZWxlY3RvcignLm1hcmsnKTtcbiAgICBpZihtYXJrKW1hcmsudGV4dENvbnRlbnQ9Y29sbGFwc2VkPyc+JzonPCc7XG4gIH0pO1xuXG4gIC8vIERhc2hib2FyZCByZWZyZXNoXG4gIHJlZnJlc2hEYXNoYm9hcmQoKTtcbiAgdmFyIGRyPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdidG4tcmVmcmVzaC1kYXNoYm9hcmQnKTtcbiAgaWYoZHIpZHIuYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLHJlZnJlc2hEYXNoYm9hcmQpO1xuXG4gIC8vIENoYXQ6IHNlbmQgbWVzc2FnZVxuICB2YXIgc2VuZEJ0bj1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnc2VuZC1idG4nKTtcbiAgaWYoc2VuZEJ0bilzZW5kQnRuLmFkZEV2ZW50TGlzdGVuZXIoJ2NsaWNrJyxmdW5jdGlvbigpe1xuICAgIGludm9rZSgnc2VuZF9tZXNzYWdlJyx7XG4gICAgICBkcmFmdDpkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnZHJhZnQtaW5wdXQnKT8udmFsdWV8fCcnLFxuICAgICAgZW5kcG9pbnQ6ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2lucHV0LWVuZHBvaW50Jyk/LnZhbHVlfHwnJyxcbiAgICAgIG1vZGVsOmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1tb2RlbCcpPy52YWx1ZXx8JycsXG4gICAgICBhcGlLZXk6ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2lucHV0LWFwaS1rZXknKT8udmFsdWV8fCcnLFxuICAgICAgc3lzdGVtUHJvbXB0OmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1zeXN0ZW0tcHJvbXB0Jyk/LnZhbHVlfHwnJ1xuICAgIH0pLnRoZW4oZnVuY3Rpb24ocyl7c2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3N0YXR1cy1iYXInKSxzKTt9KS5jYXRjaChmdW5jdGlvbihlKXtcbiAgICAgIHNldFRleHRTYWZlKGRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdzdGF0dXMtYmFyJyksJ1NlbmQgZmFpbGVkOiAnKyhlJiZlLm1lc3NhZ2V8fGV8fCd1bmtub3duJykpO1xuICAgIH0pO1xuICB9KTtcblxuICAvLyBDaGF0OiBsb2FkIFJoYWkgcHJvbXB0IHNlZWRcbiAgdmFyIHJoYWlCdG49ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3JoYWktYnRuJyk7XG4gIGlmKHJoYWlCdG4pcmhhaUJ0bi5hZGRFdmVudExpc3RlbmVyKCdjbGljaycsZnVuY3Rpb24oKXtcbiAgICBpbnZva2UoJ2xvYWRfcmhhaV9ydWxlX3Byb21wdCcse1xuICAgICAgY3VycmVudE1vZGVsOmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1tb2RlbCcpPy52YWx1ZXx8JycsXG4gICAgICBjdXJyZW50U3lzdGVtUHJvbXB0OmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1zeXN0ZW0tcHJvbXB0Jyk/LnZhbHVlfHwnJ1xuICAgIH0pLnRoZW4oZnVuY3Rpb24ocCl7XG4gICAgICBzZXRWYWwoJ2lucHV0LXN5c3RlbS1wcm9tcHQnLHAuc3lzdGVtX3Byb21wdCk7XG4gICAgICBpZihwLnN1Z2dlc3RlZF9tb2RlbClzZXRWYWwoJ2lucHV0LW1vZGVsJyxwLnN1Z2dlc3RlZF9tb2RlbCk7XG4gICAgICBzZXRWYWwoJ2RyYWZ0LWlucHV0JyxwLmRyYWZ0X21lc3NhZ2UpO1xuICAgICAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3Jldmlldy1sb2cnKSxwLnJldmlld19sb2dfdGV4dCk7XG4gICAgICBzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnc3RhdHVzLWJhcicpLHAuc3RhdHVzKTtcbiAgICB9KS5jYXRjaChmdW5jdGlvbigpe30pO1xuICB9KTtcblxuICAvLyBDaGF0IG1vZGVsIHBpbGxzXG4gIHZhciBwaWxsUGhpPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdwaWxsLXBoaScpO1xuICBpZihwaWxsUGhpKXBpbGxQaGkuYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7XG4gICAgaW52b2tlKCd1c2VfaW50ZXJuYWxfcGhpJyx7c3lzdGVtUHJvbXB0OmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1zeXN0ZW0tcHJvbXB0Jyk/LnZhbHVlfHwnJ30pLnRoZW4oYXBwbHlTZXR0aW5ncykuY2F0Y2goZnVuY3Rpb24oKXt9KTtcbiAgfSk7XG4gIHZhciBwaWxsRm91bmRyeT1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgncGlsbC1mb3VuZHJ5Jyk7XG4gIGlmKHBpbGxGb3VuZHJ5KXBpbGxGb3VuZHJ5LmFkZEV2ZW50TGlzdGVuZXIoJ2NsaWNrJyxmdW5jdGlvbigpe1xuICAgIGludm9rZSgndXNlX2ZvdW5kcnlfbG9jYWwnLHtzeXN0ZW1Qcm9tcHQ6ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2lucHV0LXN5c3RlbS1wcm9tcHQnKT8udmFsdWV8fCcnfSkudGhlbihhcHBseVNldHRpbmdzKS5jYXRjaChmdW5jdGlvbigpe30pO1xuICB9KTtcbiAgdmFyIHBpbGxDbG91ZD1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgncGlsbC1jbG91ZCcpO1xuICBpZihwaWxsQ2xvdWQpcGlsbENsb3VkLmFkZEV2ZW50TGlzdGVuZXIoJ2NsaWNrJyxmdW5jdGlvbigpe1xuICAgIGludm9rZSgndXNlX2Nsb3VkX21vZGVsJyx7c3lzdGVtUHJvbXB0OmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1zeXN0ZW0tcHJvbXB0Jyk/LnZhbHVlfHwnJ30pLnRoZW4oYXBwbHlTZXR0aW5ncykuY2F0Y2goZnVuY3Rpb24oKXtcbiAgICAgIHNldFRleHRTYWZlKGRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdjbG91ZC1oaW50JyksJ2VkaXQgZW5kcG9pbnQva2V5IGluIFNldHRpbmdzJyk7XG4gICAgICB2YXIgY2g9ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2Nsb3VkLWhpbnQnKTtpZihjaCljaC5jbGFzc0xpc3QucmVtb3ZlKCdoaWRkZW4nKTtcbiAgICB9KTtcbiAgfSk7XG5cbiAgLy8gU2V0dGluZ3M6IG1vZGVsIHByZXNldCBidXR0b25zXG4gIHZhciB1c2VQaGk9ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2J0bi11c2UtcGhpJyk7XG4gIGlmKHVzZVBoaSl1c2VQaGkuYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7XG4gICAgaW52b2tlKCd1c2VfaW50ZXJuYWxfcGhpJyx7c3lzdGVtUHJvbXB0OmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1zeXN0ZW0tcHJvbXB0Jyk/LnZhbHVlfHwnJ30pLnRoZW4oYXBwbHlTZXR0aW5ncykuY2F0Y2goZnVuY3Rpb24oKXt9KTtcbiAgfSk7XG4gIHZhciB1c2VGb3VuZHJ5PWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdidG4tdXNlLWZvdW5kcnknKTtcbiAgaWYodXNlRm91bmRyeSl1c2VGb3VuZHJ5LmFkZEV2ZW50TGlzdGVuZXIoJ2NsaWNrJyxmdW5jdGlvbigpe1xuICAgIGludm9rZSgndXNlX2ZvdW5kcnlfbG9jYWwnLHtzeXN0ZW1Qcm9tcHQ6ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2lucHV0LXN5c3RlbS1wcm9tcHQnKT8udmFsdWV8fCcnfSkudGhlbihhcHBseVNldHRpbmdzKS5jYXRjaChmdW5jdGlvbigpe30pO1xuICB9KTtcbiAgdmFyIHVzZUNsb3VkPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdidG4tdXNlLWNsb3VkJyk7XG4gIGlmKHVzZUNsb3VkKXVzZUNsb3VkLmFkZEV2ZW50TGlzdGVuZXIoJ2NsaWNrJyxmdW5jdGlvbigpe1xuICAgIGludm9rZSgndXNlX2Nsb3VkX21vZGVsJyx7c3lzdGVtUHJvbXB0OmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1zeXN0ZW0tcHJvbXB0Jyk/LnZhbHVlfHwnJ30pLnRoZW4oYXBwbHlTZXR0aW5ncykuY2F0Y2goZnVuY3Rpb24oKXt9KTtcbiAgfSk7XG5cbiAgLy8gU2V0dGluZ3M6IHNhdmVcbiAgdmFyIHNmPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdidG4tc2F2ZS1zZXR0aW5ncycpO1xuICBpZihzZilzZi5hZGRFdmVudExpc3RlbmVyKCdjbGljaycsZnVuY3Rpb24oKXtcbiAgICBpbnZva2UoJ3NhdmVfc2V0dGluZ3MnLHtcbiAgICAgIGVuZHBvaW50OmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1lbmRwb2ludCcpPy52YWx1ZXx8JycsXG4gICAgICBtb2RlbDpkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnaW5wdXQtbW9kZWwnKT8udmFsdWV8fCcnLFxuICAgICAgYXBpS2V5OmRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdpbnB1dC1hcGkta2V5Jyk/LnZhbHVlfHwnJyxcbiAgICAgIHN5c3RlbVByb21wdDpkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnaW5wdXQtc3lzdGVtLXByb21wdCcpPy52YWx1ZXx8JydcbiAgICB9KS50aGVuKGZ1bmN0aW9uKHMpe3NldFRleHRTYWZlKGRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdzdGF0dXMtYmFyJykscyk7fSkuY2F0Y2goZnVuY3Rpb24oKXt9KTtcbiAgfSk7XG5cbiAgLy8gRG9jczogb3BlbiBhbmQgbG9hZCByaGFpXG4gIHZhciBvZD1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnYnRuLW9wZW4tZG9jcycpO1xuICBpZihvZClvZC5hZGRFdmVudExpc3RlbmVyKCdjbGljaycsZnVuY3Rpb24oKXtcbiAgICBpbnZva2UoJ29wZW5fZG9jc19wbGF5Ym9vaycpLnRoZW4oZnVuY3Rpb24ocyl7XG4gICAgICBzZXRUZXh0U2FmZShkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnZG9jcy1zdGF0dXMtdGV4dCcpLHMpO1xuICAgICAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2RvY3MtcmlnLWxvZycpLHMpO1xuICAgIH0pLmNhdGNoKGZ1bmN0aW9uKCl7fSk7XG4gIH0pO1xuICB2YXIgbHI9ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2J0bi1sb2FkLXJoYWktbXV0YXRpb24nKTtcbiAgaWYobHIpbHIuYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7XG4gICAgdmFyIGNoYXRJZHg9UEFORUxTLmZpbmRJbmRleChmdW5jdGlvbihwKXtyZXR1cm4gcC5pZD09PSdjaGF0J30pO1xuICAgIGlmKGNoYXRJZHghPT0tMSlzaG93UGFuZWwoY2hhdElkeCk7XG4gICAgaW52b2tlKCdsb2FkX3JoYWlfcnVsZV9wcm9tcHQnLHtcbiAgICAgIGN1cnJlbnRNb2RlbDpkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnaW5wdXQtbW9kZWwnKT8udmFsdWV8fCcnLFxuICAgICAgY3VycmVudFN5c3RlbVByb21wdDpkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnaW5wdXQtc3lzdGVtLXByb21wdCcpPy52YWx1ZXx8JydcbiAgICB9KS50aGVuKGZ1bmN0aW9uKHApe1xuICAgICAgc2V0VGV4dFNhZmUoZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2RvY3MtcmlnLWxvZycpLHAucmV2aWV3X2xvZ190ZXh0KTtcbiAgICAgIHNldFRleHRTYWZlKGRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdkb2NzLXN0YXR1cy10ZXh0JykscC5zdGF0dXMpO1xuICAgICAgc2V0VmFsKCdkcmFmdC1pbnB1dCcscC5kcmFmdF9tZXNzYWdlKTtcbiAgICAgIHNldFZhbCgnaW5wdXQtc3lzdGVtLXByb21wdCcscC5zeXN0ZW1fcHJvbXB0KTtcbiAgICAgIGlmKHAuc3VnZ2VzdGVkX21vZGVsKXNldFZhbCgnaW5wdXQtbW9kZWwnLHAuc3VnZ2VzdGVkX21vZGVsKTtcbiAgICB9KS5jYXRjaChmdW5jdGlvbigpe30pO1xuICB9KTtcblxuICAvLyBMb2cgdGFic1xuICBkb2N1bWVudC5xdWVyeVNlbGVjdG9yQWxsKCcubG9nLXRhYicpLmZvckVhY2goZnVuY3Rpb24odGFiKXtcbiAgICB0YWIuYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7XG4gICAgICB2YXIgaWR4PXRhYi5kYXRhc2V0LmxvZztcbiAgICAgIGRvY3VtZW50LnF1ZXJ5U2VsZWN0b3JBbGwoJy5sb2ctdGFiJykuZm9yRWFjaChmdW5jdGlvbih0KXt0LmNsYXNzTGlzdC5yZW1vdmUoJ2FjdGl2ZScpO30pO1xuICAgICAgdGFiLmNsYXNzTGlzdC5hZGQoJ2FjdGl2ZScpO1xuICAgICAgZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2xvZy1wYW5lbC0wJykuY2xhc3NMaXN0LnRvZ2dsZSgnaGlkZGVuJyxpZHghPT0nMCcpO1xuICAgICAgZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2xvZy1wYW5lbC0xJykuY2xhc3NMaXN0LnRvZ2dsZSgnaGlkZGVuJyxpZHghPT0nMScpO1xuICAgIH0pO1xuICB9KTtcbn0pO1xuXG5mdW5jdGlvbiBpbml0Vml6UGFuZWwoKXtcbiAgaWYoX3ZpekluaXRpYWxpemVkKXJldHVybjtcbiAgdmFyIGN5X2Rpdj1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnY3knKTtcbiAgaWYoIWN5X2Rpdnx8dHlwZW9mIGN5dG9zY2FwZT09PSd1bmRlZmluZWQnKXJldHVybjtcbiAgdmFyIGdyYXBoQ21kPV92aXpBY3RpdmVHcmFwaD09PSd0eXBlJz8nZ2V0X3R5cGVfZ3JhcGgnOidnZXRfaG9sb25fdml6X2dyYXBoJztcbiAgaW52b2tlKGdyYXBoQ21kKS50aGVuKGZ1bmN0aW9uKGRhdGEpe1xuICAgIHZhciBlbGVtZW50cz1bXTtcbiAgICAoZGF0YS5ub2Rlc3x8W10pLmZvckVhY2goZnVuY3Rpb24obil7ZWxlbWVudHMucHVzaCh7ZGF0YTpuLmRhdGF9KTt9KTtcbiAgICAoZGF0YS5lZGdlc3x8W10pLmZvckVhY2goZnVuY3Rpb24oZSl7ZWxlbWVudHMucHVzaCh7ZGF0YTplLmRhdGF9KTt9KTtcbiAgICBfdml6QWxsRWxlbWVudHM9ZWxlbWVudHM7XG4gICAgd2luZG93Ll9jeT1jeXRvc2NhcGUoe1xuICAgICAgY29udGFpbmVyOmN5X2RpdixcbiAgICAgIGVsZW1lbnRzOmVsZW1lbnRzLFxuICAgICAgbWluWm9vbTowLjE4LFxuICAgICAgbWF4Wm9vbTozLjAsXG4gICAgICBsYXlvdXQ6e25hbWU6J2RhZ3JlJyxyYW5rRGlyOidUQicsbm9kZVNlcDo1MCxyYW5rU2VwOjcwLGFuaW1hdGU6ZmFsc2V9LFxuICAgICAgc3R5bGU6W1xuICAgICAgICB7c2VsZWN0b3I6J25vZGUnLHN0eWxlOnsnbGFiZWwnOidkYXRhKGxhYmVsKScsJ2JhY2tncm91bmQtY29sb3InOicjMWE2ZmE4JywnY29sb3InOicjZmZmJyxcbiAgICAgICAgICAndGV4dC12YWxpZ24nOidjZW50ZXInLCd0ZXh0LWhhbGlnbic6J2NlbnRlcicsJ2ZvbnQtc2l6ZSc6JzExcHgnLFxuICAgICAgICAgICd3aWR0aCc6J2xhYmVsJywnaGVpZ2h0JzonbGFiZWwnLCdwYWRkaW5nJzonOHB4Jywnc2hhcGUnOidyb3VuZHJlY3RhbmdsZScsXG4gICAgICAgICAgJ2JvcmRlci13aWR0aCc6MSwnYm9yZGVyLWNvbG9yJzonIzBiNGY3MSd9fSxcbiAgICAgICAge3NlbGVjdG9yOidlZGdlJyxzdHlsZTp7J2N1cnZlLXN0eWxlJzonYmV6aWVyJywndGFyZ2V0LWFycm93LXNoYXBlJzondHJpYW5nbGUnLFxuICAgICAgICAgICdsaW5lLWNvbG9yJzonIzZmODc5NCcsJ3RhcmdldC1hcnJvdy1jb2xvcic6JyM2Zjg3OTQnLCd3aWR0aCc6MS41fX0sXG4gICAgICAgIHtzZWxlY3RvcjonLmZhZGVkJyxzdHlsZTp7J29wYWNpdHknOjAuMTgsJ3RleHQtb3BhY2l0eSc6MC4xMn19LFxuICAgICAgICB7c2VsZWN0b3I6Jy5oaWRkZW4tZmlsdGVyJyxzdHlsZTp7J2Rpc3BsYXknOidub25lJ319LFxuICAgICAgICB7c2VsZWN0b3I6Jy5tYXRjaGVkJyxzdHlsZTp7J2JvcmRlci13aWR0aCc6MywnYm9yZGVyLWNvbG9yJzonI2YyOGMyOCcsJ3otaW5kZXgnOjk5OX19LFxuICAgICAgICB7c2VsZWN0b3I6Jy5oaWRlLWxhYmVsJyxzdHlsZTp7J2xhYmVsJzonJ319LFxuICAgICAgICB7c2VsZWN0b3I6JzpzZWxlY3RlZCcsc3R5bGU6eydib3JkZXItd2lkdGgnOjMsJ2JvcmRlci1jb2xvcic6JyNmMjhjMjgnLCdsaW5lLWNvbG9yJzonI2YyOGMyOCcsJ3RhcmdldC1hcnJvdy1jb2xvcic6JyNmMjhjMjgnfX0sXG4gICAgICAgIHtzZWxlY3Rvcjonbm9kZVtraW5kPVwiQ2Fwc3VsZUdyb3VwXCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjNWEzZThhJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVba2luZD1cIkF1ZGl0RXZlbnRcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyM3YTMwMzAnfX0sXG4gICAgICAgIHtzZWxlY3Rvcjonbm9kZVtraW5kPVwiT3dsQ2xhc3NcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyMyZTZlNDUnfX0sXG4gICAgICAgIHtzZWxlY3Rvcjonbm9kZVtraW5kPVwidHJhaXRcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyM1YTNlOGEnLCdzaGFwZSc6J2hleGFnb24nfX0sXG4gICAgICAgIHtzZWxlY3Rvcjonbm9kZVtraW5kPVwiZW51bVwiXScsc3R5bGU6eydiYWNrZ3JvdW5kLWNvbG9yJzonIzJlNmU0NScsJ3NoYXBlJzonZGlhbW9uZCd9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW2tpbmQ9XCJtY3BfdG9vbFwiXScsc3R5bGU6eydiYWNrZ3JvdW5kLWNvbG9yJzonIzhhNmIxZicsJ3NoYXBlJzondGFnJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVba2luZD1cInRhdXJpX2NvbW1hbmRcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyM3YTMwMzAnLCdzaGFwZSc6J3JvdW5kcmVjdGFuZ2xlJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVba2luZD1cImFic3RyYWN0X3RyYWl0XCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjMDAzYjVjJywnc2hhcGUnOidoZXhhZ29uJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVba2luZD1cImNvbnRyYWN0X3R5cGVcIl0sbm9kZVtraW5kPVwiZHNsX2NvbnRyYWN0XCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjMDA1ZDdmJywnc2hhcGUnOidyb3VuZHJlY3RhbmdsZSd9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW2tpbmQ9XCJtZXRhbW9kZWxfZW51bVwiXSxub2RlW2tpbmQ9XCJvbnRvbG9neV9lbnVtXCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjMDA3Yzg5Jywnc2hhcGUnOidkaWFtb25kJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVba2luZD1cInpfZG9jdW1lbnRcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyM1Zjc0ODAnLCdzaGFwZSc6J3JvdW5kcmVjdGFuZ2xlJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVba2luZD1cInpfcGlwZWxpbmVcIl0sbm9kZVtraW5kPVwicGlwZWxpbmVfc3RhdGVcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyMwMDczYTgnLCdzaGFwZSc6J3JvdW5kcmVjdGFuZ2xlJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVba2luZD1cInpfY29uc3RyYWludFwiXSxub2RlW2tpbmQ9XCJjb25zdHJhaW50X3R5cGVcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyMwMGEwYWYnLCdzaGFwZSc6J3JvdW5kcmVjdGFuZ2xlJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVba2luZD1cInpfbGVnYWxcIl0sbm9kZVtraW5kPVwibGVnYWxfdHlwZVwiXScsc3R5bGU6eydiYWNrZ3JvdW5kLWNvbG9yJzonI2MzNDgyZicsJ3NoYXBlJzoncm91bmRyZWN0YW5nbGUnfX0sXG4gICAgICAgIHtzZWxlY3Rvcjonbm9kZVtraW5kPVwiel9wcm9vZlwiXSxub2RlW2tpbmQ9XCJwcm9vZl9yZXN1bHRcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyMwMDg1NmYnLCdzaGFwZSc6J3JvdW5kcmVjdGFuZ2xlJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVba2luZD1cInpfYXR0ZXN0YXRpb25cIl0sbm9kZVtraW5kPVwiYXR0ZXN0YXRpb25fdHlwZVwiXScsc3R5bGU6eydiYWNrZ3JvdW5kLWNvbG9yJzonI2YyOGMyOCcsJ3NoYXBlJzoncm91bmRyZWN0YW5nbGUnLCdjb2xvcic6JyMxNzJiM2EnfX0sXG4gICAgICAgIHtzZWxlY3Rvcjonbm9kZVtraW5kPVwic29sdmVyX3R5cGVcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyMwMDg1NmYnLCdzaGFwZSc6J2JhcnJlbCd9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW2tpbmQ9XCJyZXN1bHRfdHlwZVwiXScsc3R5bGU6eydiYWNrZ3JvdW5kLWNvbG9yJzonIzAwOTdhOScsJ3NoYXBlJzoncm91bmQtZGlhbW9uZCd9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW2tpbmQ9XCJpc3N1ZV90eXBlXCJdLG5vZGVba2luZD1cInJldmlld19zdGF0ZVwiXScsc3R5bGU6eydiYWNrZ3JvdW5kLWNvbG9yJzonI2MzNDgyZicsJ3NoYXBlJzonb2N0YWdvbid9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW2tpbmQ9XCJnYXRlX3R5cGVcIl0nLHN0eWxlOnsnYmFja2dyb3VuZC1jb2xvcic6JyNmMjhjMjgnLCdzaGFwZSc6J3ZlZScsJ2NvbG9yJzonIzE3MmIzYSd9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW2tpbmQ9XCJldmlkZW5jZV9ncmFwaFwiXSxub2RlW2tpbmQ9XCJldmlkZW5jZV9ub2RlXCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjNmY4Nzk0Jywnc2hhcGUnOidyb3VuZHJlY3RhbmdsZSd9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW2tpbmQ9XCJ3b3JrYm9va19wcm9qZWN0aW9uXCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjNWFhNjQ2Jywnc2hhcGUnOid0YWcnfX0sXG4gICAgICAgIHtzZWxlY3Rvcjonbm9kZVtraW5kPVwidGF4b25vbXlfdHlwZVwiXScsc3R5bGU6eydiYWNrZ3JvdW5kLWNvbG9yJzonIzdmYmYzZicsJ3NoYXBlJzonZGlhbW9uZCcsJ2NvbG9yJzonIzE3MmIzYSd9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW2tpbmQ9XCJ3b3JrZmxvd190eXBlXCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjMDA1ZDdmJywnc2hhcGUnOidyaG9tYm9pZCd9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW3pfbGF5ZXI9XCJQaXBlbGluZVwiXScsc3R5bGU6eydiYWNrZ3JvdW5kLWNvbG9yJzonIzAwNzNhOCd9fSxcbiAgICAgICAge3NlbGVjdG9yOidub2RlW3pfbGF5ZXI9XCJDb25zdHJhaW50XCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjMDBhMGFmJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVbel9sYXllcj1cIkxlZ2FsXCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjYzM0ODJmJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVbel9sYXllcj1cIkZvcm1hbFByb29mXCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjMDA4NTZmJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVbel9sYXllcj1cIkF0dGVzdGF0aW9uXCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjZjI4YzI4JywnY29sb3InOicjMTcyYjNhJ319LFxuICAgICAgICB7c2VsZWN0b3I6J25vZGVbel9sYXllcj1cIkRvY3VtZW50XCJdJyxzdHlsZTp7J2JhY2tncm91bmQtY29sb3InOicjNWY3NDgwJ319LFxuICAgICAgICB7c2VsZWN0b3I6J2VkZ2UnLHN0eWxlOnsnbGFiZWwnOidkYXRhKGxhYmVsKScsJ2ZvbnQtc2l6ZSc6JzlweCcsJ2NvbG9yJzonIzE3M2I0YScsJ3RleHQtYmFja2dyb3VuZC1jb2xvcic6JyNmZmZmZmYnLCd0ZXh0LWJhY2tncm91bmQtb3BhY2l0eSc6MC45MiwndGV4dC1iYWNrZ3JvdW5kLXBhZGRpbmcnOicycHgnfX0sXG4gICAgICBdXG4gICAgfSk7XG4gICAgX3ZpekluaXRpYWxpemVkPXRydWU7XG4gICAgc2V0dXBWaXpDb250cm9scygpO1xuICAgIHNldFZpekRldGFpbChudWxsKTtcbiAgICB3aW5kb3cuX2N5LnJlYWR5KGZ1bmN0aW9uKCl7XG4gICAgICBzZXRUaW1lb3V0KGZ1bmN0aW9uKCl7aWYod2luZG93Ll9jeSl3aW5kb3cuX2N5LmZpdCh3aW5kb3cuX2N5LmVsZW1lbnRzKCkubm90KCcuaGlkZGVuLWZpbHRlcicpLFZJWl9GSVRfUEFERElORyk7fSwzMDApO1xuICAgIH0pO1xuICAgIHZhciBidG49ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2J0bi12aXotcmVmcmVzaCcpO1xuICAgIGlmKGJ0bilidG4uYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7X3ZpekluaXRpYWxpemVkPWZhbHNlO3dpbmRvdy5fY3kmJndpbmRvdy5fY3kuZGVzdHJveSgpO2luaXRWaXpQYW5lbCgpO30pO1xuICB9KS5jYXRjaChmdW5jdGlvbihlKXtjb25zb2xlLmVycm9yKCdbdml6XSAnK2dyYXBoQ21kKycgZmFpbGVkOicsZSk7fSk7XG59XG5cbmZ1bmN0aW9uIHJ1blZpekxheW91dCgpe1xuICBpZighd2luZG93Ll9jeSlyZXR1cm47XG4gIHZhciBsYXlvdXQ9d2luZG93Ll9jeS5sYXlvdXQoe25hbWU6J2RhZ3JlJyxyYW5rRGlyOidUQicsbm9kZVNlcDo1MCxyYW5rU2VwOjcwLGFuaW1hdGU6ZmFsc2V9KTtcbiAgd2luZG93Ll9jeS5vbmUoJ2xheW91dHN0b3AnLGZ1bmN0aW9uKCl7d2luZG93Ll9jeS5maXQod2luZG93Ll9jeS5lbGVtZW50cygpLm5vdCgnLmhpZGRlbi1maWx0ZXInKSxWSVpfRklUX1BBRERJTkcpO30pO1xuICBsYXlvdXQucnVuKCk7XG59XG5cbmZ1bmN0aW9uIHpvb21WaXpCeShmYWN0b3Ipe1xuICB2YXIgY3k9d2luZG93Ll9jeTtpZighY3kpcmV0dXJuO1xuICBjeS56b29tKHtsZXZlbDpjeS56b29tKCkqZmFjdG9yLHJlbmRlcmVkUG9zaXRpb246e3g6Y3kud2lkdGgoKS8yLHk6Y3kuaGVpZ2h0KCkvMn19KTtcbn1cblxuZnVuY3Rpb24gcG9wdWxhdGVWaXpGaWx0ZXJzKCl7XG4gIHZhciBlZGdlU2VsPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCd2aXotZWRnZS1maWx0ZXInKTtcbiAgaWYoIWVkZ2VTZWx8fCF3aW5kb3cuX2N5KXJldHVybjtcbiAgZWRnZVNlbC5pbm5lckhUTUw9JzxvcHRpb24gdmFsdWU9XCJcIj5BbGwgcmVsYXRpb25zPC9vcHRpb24+JztcbiAgdmFyIGxhYmVscz17fTtcbiAgd2luZG93Ll9jeS5lZGdlcygpLmZvckVhY2goZnVuY3Rpb24oZSl7bGFiZWxzW2UuZGF0YSgnbGFiZWwnKV09dHJ1ZTt9KTtcbiAgT2JqZWN0LmtleXMobGFiZWxzKS5zb3J0KCkuZm9yRWFjaChmdW5jdGlvbihsKXtcbiAgICBpZighbClyZXR1cm47XG4gICAgdmFyIG89ZG9jdW1lbnQuY3JlYXRlRWxlbWVudCgnb3B0aW9uJyk7by52YWx1ZT1sO28udGV4dENvbnRlbnQ9bDtlZGdlU2VsLmFwcGVuZENoaWxkKG8pO1xuICB9KTtcbn1cblxuZnVuY3Rpb24gYXBwbHlWaXpGaWx0ZXJzKCl7XG4gIHZhciBjeT13aW5kb3cuX2N5O2lmKCFjeSlyZXR1cm47XG4gIHZhciBxdWVyeT0oZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3Zpei1zZWFyY2gnKT8udmFsdWV8fCcnKS50b0xvd2VyQ2FzZSgpLnRyaW0oKTtcbiAgdmFyIGVkZ2VMYWJlbD1kb2N1bWVudC5nZXRFbGVtZW50QnlJZCgndml6LWVkZ2UtZmlsdGVyJyk/LnZhbHVlfHwnJztcbiAgY3kuZWxlbWVudHMoKS5yZW1vdmVDbGFzcygnaGlkZGVuLWZpbHRlciBtYXRjaGVkIGZhZGVkJyk7XG4gIGN5Lm5vZGVzKCkuZm9yRWFjaChmdW5jdGlvbihuKXtcbiAgICB2YXIgbGFiZWw9U3RyaW5nKG4uZGF0YSgnbGFiZWwnKXx8JycpLnRvTG93ZXJDYXNlKCk7XG4gICAgdmFyIGlkPVN0cmluZyhuLmRhdGEoJ2lkJyl8fCcnKS50b0xvd2VyQ2FzZSgpO1xuICAgIHZhciBzZWFyY2hPaz0hcXVlcnl8fGxhYmVsLmluZGV4T2YocXVlcnkpIT09LTF8fGlkLmluZGV4T2YocXVlcnkpIT09LTE7XG4gICAgaWYoIXNlYXJjaE9rKW4uYWRkQ2xhc3MoJ2hpZGRlbi1maWx0ZXInKTtcbiAgICBlbHNlIGlmKHF1ZXJ5KW4uYWRkQ2xhc3MoJ21hdGNoZWQnKTtcbiAgfSk7XG4gIGN5LmVkZ2VzKCkuZm9yRWFjaChmdW5jdGlvbihlKXtcbiAgICBpZihlZGdlTGFiZWwmJmUuZGF0YSgnbGFiZWwnKSE9PWVkZ2VMYWJlbCllLmFkZENsYXNzKCdoaWRkZW4tZmlsdGVyJyk7XG4gICAgaWYoZS5zb3VyY2UoKS5oYXNDbGFzcygnaGlkZGVuLWZpbHRlcicpfHxlLnRhcmdldCgpLmhhc0NsYXNzKCdoaWRkZW4tZmlsdGVyJykpZS5hZGRDbGFzcygnaGlkZGVuLWZpbHRlcicpO1xuICB9KTtcbiAgdmFyIHZpc2libGU9Y3kuZWxlbWVudHMoKS5ub3QoJy5oaWRkZW4tZmlsdGVyJyk7XG4gIGlmKHF1ZXJ5fHxlZGdlTGFiZWwpe1xuICAgIGN5LmVsZW1lbnRzKCkubm90KHZpc2libGUpLmFkZENsYXNzKCdmYWRlZCcpO1xuICAgIGlmKHZpc2libGUubGVuZ3RoPjApY3kuZml0KHZpc2libGUsVklaX0ZJVF9QQURESU5HKTtcbiAgfVxufVxuXG5mdW5jdGlvbiBzZXRWaXpEZXRhaWwoZWxlKXtcbiAgdmFyIGJvZHk9ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3Zpei1kZXRhaWwtYm9keScpO1xuICBpZighYm9keSlyZXR1cm47XG4gIGlmKCFlbGUpe1xuICAgIGJvZHkudGV4dENvbnRlbnQ9J1NlbGVjdCBhIG5vZGUgb3IgcmVsYXRpb25zaGlwLic7XG4gICAgcmV0dXJuO1xuICB9XG4gIGlmKGVsZS5pc05vZGUmJmVsZS5pc05vZGUoKSl7XG4gICAgYm9keS5pbm5lckhUTUw9JzxkaXY+PGI+Jytlc2NhcGVIdG1sKGVsZS5kYXRhKCdsYWJlbCcpfHwnJykrJzwvYj48L2Rpdj48ZGl2PicrZXNjYXBlSHRtbChlbGUuZGF0YSgnaWQnKXx8JycpKyc8L2Rpdj48ZGl2IGNsYXNzPVwidml6LWRldGFpbC1jaGlwXCI+Jytlc2NhcGVIdG1sKGVsZS5kYXRhKCdraW5kJyl8fCcnKSsnPC9kaXY+JztcbiAgfWVsc2V7XG4gICAgYm9keS5pbm5lckhUTUw9JzxkaXY+PGI+Jytlc2NhcGVIdG1sKGVsZS5kYXRhKCdsYWJlbCcpfHwncmVsYXRpb25zaGlwJykrJzwvYj48L2Rpdj48ZGl2PicrZXNjYXBlSHRtbChlbGUuZGF0YSgnc291cmNlJyl8fCcnKSsnPC9kaXY+PGRpdj5cdTIxOTI8L2Rpdj48ZGl2PicrZXNjYXBlSHRtbChlbGUuZGF0YSgndGFyZ2V0Jyl8fCcnKSsnPC9kaXY+JztcbiAgfVxufVxuXG5mdW5jdGlvbiBlc2NhcGVIdG1sKHMpe1xuICByZXR1cm4gU3RyaW5nKHMpLnJlcGxhY2UoL1smPD5cIiddL2csZnVuY3Rpb24oYyl7cmV0dXJuIHsnJic6JyZhbXA7JywnPCc6JyZsdDsnLCc+JzonJmd0OycsJ1wiJzonJnF1b3Q7JyxcIidcIjonJiMzOTsnfVtjXTt9KTtcbn1cblxuZnVuY3Rpb24gc2V0dXBWaXpDb250cm9scygpe1xuICB2YXIgY3k9d2luZG93Ll9jeTtpZighY3kpcmV0dXJuO1xuICBwb3B1bGF0ZVZpekZpbHRlcnMoKTtcbiAgZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2J0bi12aXotem9vbS1pbicpPy5hZGRFdmVudExpc3RlbmVyKCdjbGljaycsZnVuY3Rpb24oKXt6b29tVml6QnkoMS4yKTt9KTtcbiAgZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2J0bi12aXotem9vbS1vdXQnKT8uYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7em9vbVZpekJ5KDAuODMpO30pO1xuICBkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgnYnRuLXZpei1maXQnKT8uYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7Y3kuZml0KGN5LmVsZW1lbnRzKCkubm90KCcuaGlkZGVuLWZpbHRlcicpLFZJWl9GSVRfUEFERElORyk7fSk7XG4gIGRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdidG4tdml6LXJlc2V0Jyk/LmFkZEV2ZW50TGlzdGVuZXIoJ2NsaWNrJyxmdW5jdGlvbigpe2N5Lnpvb20oMSk7Y3kuY2VudGVyKCk7fSk7XG4gIGRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdidG4tdml6LWxheW91dCcpPy5hZGRFdmVudExpc3RlbmVyKCdjbGljaycsZnVuY3Rpb24oKXtydW5WaXpMYXlvdXQoKTt9KTtcbiAgZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2J0bi12aXotbGFiZWxzJyk/LmFkZEV2ZW50TGlzdGVuZXIoJ2NsaWNrJyxmdW5jdGlvbigpe2N5Lm5vZGVzKCkudG9nZ2xlQ2xhc3MoJ2hpZGUtbGFiZWwnKTt9KTtcbiAgZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2J0bi12aXotZWRnZS1sYWJlbHMnKT8uYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7Y3kuZWRnZXMoKS50b2dnbGVDbGFzcygnaGlkZS1sYWJlbCcpO30pO1xuICBkb2N1bWVudC5nZXRFbGVtZW50QnlJZCgndml6LXNlYXJjaCcpPy5hZGRFdmVudExpc3RlbmVyKCdpbnB1dCcsYXBwbHlWaXpGaWx0ZXJzKTtcbiAgZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ3Zpei1lZGdlLWZpbHRlcicpPy5hZGRFdmVudExpc3RlbmVyKCdjaGFuZ2UnLGFwcGx5Vml6RmlsdGVycyk7XG4gIGRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdidG4tdml6LWNsZWFyJyk/LmFkZEV2ZW50TGlzdGVuZXIoJ2NsaWNrJyxmdW5jdGlvbigpe1xuICAgIHZhciBzPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCd2aXotc2VhcmNoJyk7aWYocylzLnZhbHVlPScnO1xuICAgIHZhciBlPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCd2aXotZWRnZS1maWx0ZXInKTtpZihlKWUudmFsdWU9Jyc7XG4gICAgYXBwbHlWaXpGaWx0ZXJzKCk7Y3kuZml0KHVuZGVmaW5lZCxWSVpfRklUX1BBRERJTkcpO1xuICB9KTtcbiAgdmFyIHRhYlR5cGU9ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2J0bi12aXotdGFiLXR5cGUnKTtcbiAgdmFyIHRhYlBpcGVsaW5lPWRvY3VtZW50LmdldEVsZW1lbnRCeUlkKCdidG4tdml6LXRhYi1waXBlbGluZScpO1xuICBpZih0YWJUeXBlKXRhYlR5cGUuYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7XG4gICAgaWYoX3ZpekFjdGl2ZUdyYXBoPT09J3R5cGUnKXJldHVybjtcbiAgICBfdml6QWN0aXZlR3JhcGg9J3R5cGUnO1xuICAgIHRhYlR5cGUuY2xhc3NMaXN0LmFkZCgnYWN0aXZlJyk7XG4gICAgaWYodGFiUGlwZWxpbmUpdGFiUGlwZWxpbmUuY2xhc3NMaXN0LnJlbW92ZSgnYWN0aXZlJyk7XG4gICAgX3ZpekluaXRpYWxpemVkPWZhbHNlO3dpbmRvdy5fY3kmJndpbmRvdy5fY3kuZGVzdHJveSgpO2luaXRWaXpQYW5lbCgpO1xuICB9KTtcbiAgaWYodGFiUGlwZWxpbmUpdGFiUGlwZWxpbmUuYWRkRXZlbnRMaXN0ZW5lcignY2xpY2snLGZ1bmN0aW9uKCl7XG4gICAgaWYoX3ZpekFjdGl2ZUdyYXBoPT09J3BpcGVsaW5lJylyZXR1cm47XG4gICAgX3ZpekFjdGl2ZUdyYXBoPSdwaXBlbGluZSc7XG4gICAgdGFiUGlwZWxpbmUuY2xhc3NMaXN0LmFkZCgnYWN0aXZlJyk7XG4gICAgaWYodGFiVHlwZSl0YWJUeXBlLmNsYXNzTGlzdC5yZW1vdmUoJ2FjdGl2ZScpO1xuICAgIF92aXpJbml0aWFsaXplZD1mYWxzZTt3aW5kb3cuX2N5JiZ3aW5kb3cuX2N5LmRlc3Ryb3koKTtpbml0Vml6UGFuZWwoKTtcbiAgfSk7XG4gIGN5Lm9uKCd0YXAnLCdub2RlLGVkZ2UnLGZ1bmN0aW9uKGV2dCl7c2V0Vml6RGV0YWlsKGV2dC50YXJnZXQpO30pO1xuICBjeS5vbigndGFwJyxmdW5jdGlvbihldnQpe2lmKGV2dC50YXJnZXQ9PT1jeSlzZXRWaXpEZXRhaWwobnVsbCk7fSk7XG59XG4iXSwKICAibWFwcGluZ3MiOiAiO0FBQ0EsU0FBUyxPQUFPLEtBQUksTUFBSztBQUFDLE1BQUksTUFBSSxPQUFPO0FBQVUsTUFBRyxDQUFDLElBQUksUUFBTyxRQUFRLE9BQU8sSUFBSSxNQUFNLGNBQWMsQ0FBQztBQUFFLE1BQUcsQ0FBQyxJQUFJLEtBQUssUUFBTyxRQUFRLE9BQU8sSUFBSSxNQUFNLFVBQVUsQ0FBQztBQUFFLFNBQU8sSUFBSSxLQUFLLE9BQU8sS0FBSSxJQUFJO0FBQUM7QUFDdE0sU0FBUyxPQUFPLEdBQUUsR0FBRTtBQUFDLE1BQUksTUFBSSxPQUFPO0FBQVUsTUFBRyxDQUFDLElBQUksUUFBTyxRQUFRLE9BQU8sSUFBSSxNQUFNLGNBQWMsQ0FBQztBQUFFLFNBQU8sSUFBSSxNQUFNLE9BQU8sR0FBRSxDQUFDO0FBQUM7QUFFbkksSUFBSSxTQUFPO0FBQUEsRUFDVCxFQUFDLElBQUcsUUFBTyxNQUFLLE1BQUssT0FBTSxPQUFNO0FBQUEsRUFDakMsRUFBQyxJQUFHLFFBQU8sTUFBSyxNQUFLLE9BQU0sT0FBTTtBQUFBLEVBQ2pDLEVBQUMsSUFBRyxRQUFPLE1BQUssTUFBSyxPQUFNLFlBQVc7QUFBQSxFQUN0QyxFQUFDLElBQUcsWUFBVyxNQUFLLE1BQUssT0FBTSxXQUFVO0FBQUEsRUFDekMsRUFBQyxJQUFHLFFBQU8sTUFBSyxNQUFLLE9BQU0sZ0JBQWU7QUFBQSxFQUMxQyxFQUFDLElBQUcsT0FBTSxNQUFLLE1BQUssT0FBTSxNQUFLO0FBQ2pDO0FBQ0EsSUFBSSxjQUFZO0FBQ2hCLElBQUksbUJBQWlCLE9BQU8sVUFBVSxT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
