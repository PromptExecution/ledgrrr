function tauriApi(){return window.__TAURI__}
function invoke(cmd,args){var api=window.__TAURI__;if(!api)return Promise.reject(new Error('no __TAURI__'));if(!api.core)return Promise.reject(new Error('no .core'));return api.core.invoke(cmd,args)}
function listen(e,h){var api=window.__TAURI__;if(!api)return Promise.reject(new Error('no __TAURI__'));return api.event.listen(e,h)}

var PANELS=[
  {id:'chat',icon:'AI',label:'Chat'},
  {id:'logs',icon:'LG',label:'Logs'},
  {id:'dash',icon:'DB',label:'Dashboard'},
  {id:'settings',icon:'ST',label:'Settings'},
  {id:'docs',icon:'DK',label:'Docs Playbook'},
  {id:'viz',icon:'VZ',label:'Viz'},
];
var activePanel=0;
var DASH_PANEL_INDEX=PANELS.findIndex(function(p){return p.id==='dash'});
var VIZ_PANEL_INDEX=PANELS.findIndex(function(p){return p.id==='viz'});
var _vizInitialized=false;
var _vizAllElements=[];
var _vizActiveGraph='type'; // 'type' | 'pipeline'
var VIZ_FIT_PADDING=72;

function showPanel(i){
  activePanel=i;
  PANELS.forEach(function(p,j){
    var el=document.getElementById('panel-'+p.id);
    if(el)el.classList.toggle('hidden',j!==i);
  });
  document.querySelectorAll('.nav-item[data-panel-index]').forEach(function(b,j){
    b.classList.toggle('active',j===i);
  });
  if(DASH_PANEL_INDEX!==-1&&i===DASH_PANEL_INDEX)refreshDashboard();
  if(VIZ_PANEL_INDEX!==-1&&i===VIZ_PANEL_INDEX)initVizPanel();
}

function panelTemplate(id){
  var t={}
  t.chat='<div class="panel-header"><span class="panel-title">Chat</span><div id="model-badge" class="model-badge phi"><span id="model-badge-icon">&#9889;</span><span id="model-badge-text">No model</span></div></div><div class="model-bar"><span class="model-bar-label">Model:</span><button id="pill-phi" class="model-pill">&#9889; Phi-4</button><button id="pill-foundry" class="model-pill">Windows AI</button><button id="pill-cloud" class="model-pill">&#9729; Cloud</button><span id="cloud-hint" class="cloud-hint hidden">edit in Settings</span></div><div id="transcript-wrap" class="transcript-wrap"><div class="log-label">Transcript</div><div id="transcript" class="transcript-content"></div></div><div class="input-area"><textarea id="draft-input" rows="5"></textarea><div class="input-actions"><button id="send-btn">Send</button><button id="rhai-btn">Rhai Rule</button></div></div>';
  t.logs='<div class="panel-title-row"><span class="panel-title">Logs</span></div><div class="log-tabs"><button class="log-tab active" data-log="0">Transport</button><button class="log-tab" data-log="1">Review</button></div><div id="log-panel-0" class="log-subpanel transport-bg"><div class="log-label">Transport</div><div id="rig-log" class="log-content"></div></div><div id="log-panel-1" class="log-subpanel review-bg hidden"><div class="log-label review-label">Diffsets</div><div id="review-log" class="log-content"></div></div></div>';
  t.dash='<span class="panel-title">Dashboard</span><div id="evidence-summary" class="evidence-summary"><div class="ev-card ev-card-blocked"><div class="ev-card-value" id="blocked-value">-</div><div class="ev-card-label">Blocked</div></div><div class="ev-card ev-card-ready"><div class="ev-card-value" id="ready-value">-</div><div class="ev-card-label">Ready</div></div><div class="ev-card ev-card-exported"><div class="ev-card-value" id="exported-value">-</div><div class="ev-card-label">Exported</div></div><div class="ev-card ev-card-issues"><div class="ev-card-value" id="issues-value">-</div><div class="ev-card-label">Issues</div></div></div><div class="ev-section"><div class="ev-section-title">Last Action</div><div id="ev-last-action" class="ev-last-action">Loading...</div></div><div class="ev-section"><div class="ev-section-title">Next Actions</div><ul id="ev-next-actions" class="ev-next-actions"></ul></div><div class="ev-section"><div class="ev-section-title">Providers</div><div id="ev-provider-status" class="ev-provider-status">Loading...</div></div><div class="ev-refresh-row"><button id="btn-refresh-dashboard">Refresh</button></div>';
  t.settings='<span class="panel-title">Settings</span><label class="field-label" for="input-endpoint">Endpoint</label><input id="input-endpoint" type="text" class="field-input"/><label class="field-label" for="input-model">Model</label><input id="input-model" type="text" class="field-input"/><label class="field-label" for="input-api-key">Key</label><input id="input-api-key" type="text" class="field-input"/><label class="field-label" for="input-system-prompt">System Prompt</label><textarea id="input-system-prompt" class="field-input system-prompt-area" rows="6"></textarea><div class="settings-actions"><button id="btn-use-phi">Use Phi-4</button><button id="btn-use-foundry">Use Win AI</button><button id="btn-use-cloud">Use Cloud</button><button id="btn-save-settings">Save</button></div>';
  t.viz='<div class="panel-title-row viz-title-row"><div class="viz-tabs"><button id="btn-viz-tab-type" class="viz-tab active">Type Graph</button><button id="btn-viz-tab-pipeline" class="viz-tab">Pipeline</button></div><span class="panel-title">Ontology Viz</span><div class="viz-toolbar"><button id="btn-viz-zoom-out" title="Zoom out">-</button><button id="btn-viz-zoom-in" title="Zoom in">+</button><button id="btn-viz-fit" title="Fit graph">Fit</button><button id="btn-viz-reset" title="Reset zoom">1:1</button><button id="btn-viz-layout" title="Run layout">Layout</button><button id="btn-viz-labels" title="Toggle node labels">Labels</button><button id="btn-viz-edge-labels" title="Toggle relationship labels">Edges</button><input id="viz-search" class="viz-search" type="search" placeholder="Find type"/><select id="viz-edge-filter" class="viz-select"><option value="">All relations</option></select><button id="btn-viz-clear" title="Clear filters">Clear</button><button id="btn-viz-refresh" title="Reload graph">Refresh</button></div></div><div class="viz-body"><div id="cy" class="viz-canvas"></div><aside id="viz-detail" class="viz-detail"><div class="viz-detail-title">Selection</div><div id="viz-detail-body" class="viz-detail-body">Select a node or relationship.</div></aside></div>';
  t.docs='<span class="panel-title">Docs Playbook</span><p id="docs-status-text" class="docs-status"></p><div class="docs-actions"><button id="btn-open-docs">Open Docs</button><button id="btn-load-rhai-mutation">Load Rhai</button></div><div class="docs-preview-wrap"><div id="docs-rig-log" class="log-content"></div></div>';
  return t[id]||'';
}

function buildUI(){
  try{
    var nav=document.getElementById('nav-items');
    var pc=document.getElementById('panel-container');
    if(!nav||!pc)return;
    PANELS.forEach(function(p,i){
      var btn=document.createElement('button');btn.className='nav-item';btn.dataset.panelIndex=i;
      btn.innerHTML='<span class="mark">'+p.icon+'</span><span class="label">'+p.label+'</span>';
      (function(idx){btn.addEventListener('click',function(){showPanel(idx);});})(i);
      nav.appendChild(btn);
      var div=document.createElement('div');div.id='panel-'+p.id;
      div.className='panel card'+(i===0?'':' hidden');
      if(p.id==='settings')div.classList.add('settings-bg');
      div.innerHTML=panelTemplate(p.id);
      pc.appendChild(div);
    });
    showPanel(0);
  }catch(e){console.error('[ui] buildUI err:',e)}
}

function readinessLabel(r){
  if(!r)return'Unknown';
  if(r==='ready')return'Ready';
  if(r.setup_needed)return'Setup needed';
  if(r.unavailable)return'Unavailable';
  if(r.diagnostic)return'Diagnostic';
  return String(r);
}

function setTextSafe(el,text){
  if(el)el.textContent=text!=null?String(text):'';
}

function refreshDashboard(){
  var api=window.__TAURI__;
  if(!api)return;
  api.core.invoke('get_evidence_dashboard').then(function(p){
    var q=p.today_queue||{};
    setTextSafe(document.getElementById('blocked-value'),q.blocked??'-');
    setTextSafe(document.getElementById('ready-value'),q.ready_to_review??'-');
    setTextSafe(document.getElementById('exported-value'),q.exported??'-');
    setTextSafe(document.getElementById('issues-value'),q.with_validation_issues??'-');
    setTextSafe(document.getElementById('ev-last-action'),q.last_action_summary??'');
    var na=document.getElementById('ev-next-actions');
    if(na){
      na.innerHTML='';
      (q.next_actions||[]).forEach(function(a){
        var li=document.createElement('li');
        li.textContent=a;
        na.appendChild(li);
      });
    }
    var ps=document.getElementById('ev-provider-status');
    if(ps){
      ps.innerHTML='';
      (q.providers||[]).forEach(function(prov){
        var d=document.createElement('div');
        d.className='ev-provider-line';
        d.textContent=`${prov.display_name||prov.label}: ${readinessLabel(prov.readiness)}`;
        ps.appendChild(d);
      });
    }
  }).catch(function(err){
    var sb=document.getElementById('status-bar');
    if(sb)sb.textContent='Dashboard refresh failed: '+(err&&err.message||err||'unknown error');
  });
}

function setVal(id,v){var el=document.getElementById(id);if(el)el.value=v!=null?String(v):'';}

function updateModelBadge(model,apiKey){
  var isPhi=apiKey==='local-tool-tray';
  var isFoundry=apiKey==='local-foundry';
  var badge=document.getElementById('model-badge');
  var icon=document.getElementById('model-badge-icon');
  var text=document.getElementById('model-badge-text');
  if(!badge)return;
  badge.className='model-badge '+(isPhi?'phi':isFoundry?'foundry':'cloud');
  if(icon)icon.textContent=isPhi?'⚡':isFoundry?'WA':'☁';
  if(text)text.textContent=model||'No model — go to Settings';
  // Update pill active states
  var pillPhi=document.getElementById('pill-phi');
  var pillFoundry=document.getElementById('pill-foundry');
  var pillCloud=document.getElementById('pill-cloud');
  if(pillPhi)pillPhi.classList.toggle('active',isPhi);
  if(pillFoundry)pillFoundry.classList.toggle('active',isFoundry);
  if(pillCloud)pillCloud.classList.toggle('active',!isPhi&&!isFoundry&&model!=='');
  // Cloud hint: show when cloud is active
  var ch=document.getElementById('cloud-hint');
  if(ch)ch.classList.toggle('hidden',isPhi||isFoundry||model==='');
}

function setBusy(busy){
  var sb=document.getElementById('send-btn');if(sb)sb.disabled=busy;
  if(sb)sb.textContent=busy?'Sending…':'Send';
  ['draft-input','rhai-btn','pill-phi','pill-foundry','pill-cloud',
   'btn-use-phi','btn-use-foundry','btn-use-cloud',
   'btn-open-docs','btn-load-rhai-mutation'].forEach(function(id){
    var el=document.getElementById(id);if(el)el.disabled=busy;
  });
  ['input-endpoint','input-model','input-api-key','input-system-prompt'].forEach(function(id){
    var el=document.getElementById(id);if(el)el.disabled=busy;
  });
  var saveBtn=document.getElementById('btn-save-settings');
  if(saveBtn)saveBtn.textContent=busy?'Working…':'Save';
}

function applySettings(p){
  setVal('input-endpoint',p.endpoint_text);
  setVal('input-model',p.model_text);
  setVal('input-api-key',p.api_key_text);
  setVal('input-system-prompt',p.system_prompt_text);
  setTextSafe(document.getElementById('status-bar'),p.status_text);
  updateModelBadge(p.model_text,p.api_key_text);
}

document.addEventListener('DOMContentLoaded',function(){
  buildUI();

  // Populate initial state from backend
  invoke('get_initial_state').then(function(s){
    setTextSafe(document.getElementById('version-text'),s.version_text);
    setTextSafe(document.getElementById('status-bar'),s.status_text);
    setVal('input-endpoint',s.endpoint_text);
    setVal('input-model',s.model_text);
    setVal('input-api-key',s.api_key_text);
    setVal('input-system-prompt',s.system_prompt_text);
    setTextSafe(document.getElementById('transcript'),s.transcript_text);
    setTextSafe(document.getElementById('rig-log'),s.rig_log_text);
    setTextSafe(document.getElementById('review-log'),s.review_log_text);
    setVal('draft-input',s.draft_message_text);
    setTextSafe(document.getElementById('docs-status-text'),s.docs_status_text);
    updateModelBadge(s.model_text,s.api_key_text);
  }).catch(function(){});

  // Listen for chat-update events from send_message
  listen('chat-update',function(ev){
    var d=ev.payload;
    setTextSafe(document.getElementById('transcript'),d.transcript_text);
    setTextSafe(document.getElementById('rig-log'),d.rig_log_text);
    if(d.review_log_text!=null)setTextSafe(document.getElementById('review-log'),d.review_log_text);
    setVal('draft-input',d.draft_message_text);
    setTextSafe(document.getElementById('status-bar'),d.status_text);
    setBusy(!!d.busy);
  }).catch(function(){});

  // Sidebar collapse
  var colBtn=document.getElementById('collapse-btn');
  if(colBtn)colBtn.addEventListener('click',function(){
    var sb=document.getElementById('sidebar');
    if(!sb)return;
    var collapsed=sb.classList.toggle('collapsed');
    var mark=colBtn.querySelector('.mark');
    if(mark)mark.textContent=collapsed?'>':'<';
  });

  // Dashboard refresh
  refreshDashboard();
  var dr=document.getElementById('btn-refresh-dashboard');
  if(dr)dr.addEventListener('click',refreshDashboard);

  // Chat: send message
  var sendBtn=document.getElementById('send-btn');
  if(sendBtn)sendBtn.addEventListener('click',function(){
    invoke('send_message',{
      draft:document.getElementById('draft-input')?.value||'',
      endpoint:document.getElementById('input-endpoint')?.value||'',
      model:document.getElementById('input-model')?.value||'',
      apiKey:document.getElementById('input-api-key')?.value||'',
      systemPrompt:document.getElementById('input-system-prompt')?.value||''
    }).then(function(s){setTextSafe(document.getElementById('status-bar'),s);}).catch(function(e){
      setTextSafe(document.getElementById('status-bar'),'Send failed: '+(e&&e.message||e||'unknown'));
    });
  });

  // Chat: load Rhai prompt seed
  var rhaiBtn=document.getElementById('rhai-btn');
  if(rhaiBtn)rhaiBtn.addEventListener('click',function(){
    invoke('load_rhai_rule_prompt',{
      currentModel:document.getElementById('input-model')?.value||'',
      currentSystemPrompt:document.getElementById('input-system-prompt')?.value||''
    }).then(function(p){
      setVal('input-system-prompt',p.system_prompt);
      if(p.suggested_model)setVal('input-model',p.suggested_model);
      setVal('draft-input',p.draft_message);
      setTextSafe(document.getElementById('review-log'),p.review_log_text);
      setTextSafe(document.getElementById('status-bar'),p.status);
    }).catch(function(){});
  });

  // Chat model pills
  var pillPhi=document.getElementById('pill-phi');
  if(pillPhi)pillPhi.addEventListener('click',function(){
    invoke('use_internal_phi',{systemPrompt:document.getElementById('input-system-prompt')?.value||''}).then(applySettings).catch(function(){});
  });
  var pillFoundry=document.getElementById('pill-foundry');
  if(pillFoundry)pillFoundry.addEventListener('click',function(){
    invoke('use_foundry_local',{systemPrompt:document.getElementById('input-system-prompt')?.value||''}).then(applySettings).catch(function(){});
  });
  var pillCloud=document.getElementById('pill-cloud');
  if(pillCloud)pillCloud.addEventListener('click',function(){
    invoke('use_cloud_model',{systemPrompt:document.getElementById('input-system-prompt')?.value||''}).then(applySettings).catch(function(){
      setTextSafe(document.getElementById('cloud-hint'),'edit endpoint/key in Settings');
      var ch=document.getElementById('cloud-hint');if(ch)ch.classList.remove('hidden');
    });
  });

  // Settings: model preset buttons
  var usePhi=document.getElementById('btn-use-phi');
  if(usePhi)usePhi.addEventListener('click',function(){
    invoke('use_internal_phi',{systemPrompt:document.getElementById('input-system-prompt')?.value||''}).then(applySettings).catch(function(){});
  });
  var useFoundry=document.getElementById('btn-use-foundry');
  if(useFoundry)useFoundry.addEventListener('click',function(){
    invoke('use_foundry_local',{systemPrompt:document.getElementById('input-system-prompt')?.value||''}).then(applySettings).catch(function(){});
  });
  var useCloud=document.getElementById('btn-use-cloud');
  if(useCloud)useCloud.addEventListener('click',function(){
    invoke('use_cloud_model',{systemPrompt:document.getElementById('input-system-prompt')?.value||''}).then(applySettings).catch(function(){});
  });

  // Settings: save
  var sf=document.getElementById('btn-save-settings');
  if(sf)sf.addEventListener('click',function(){
    invoke('save_settings',{
      endpoint:document.getElementById('input-endpoint')?.value||'',
      model:document.getElementById('input-model')?.value||'',
      apiKey:document.getElementById('input-api-key')?.value||'',
      systemPrompt:document.getElementById('input-system-prompt')?.value||''
    }).then(function(s){setTextSafe(document.getElementById('status-bar'),s);}).catch(function(){});
  });

  // Docs: open and load rhai
  var od=document.getElementById('btn-open-docs');
  if(od)od.addEventListener('click',function(){
    invoke('open_docs_playbook').then(function(s){
      setTextSafe(document.getElementById('docs-status-text'),s);
      setTextSafe(document.getElementById('docs-rig-log'),s);
    }).catch(function(){});
  });
  var lr=document.getElementById('btn-load-rhai-mutation');
  if(lr)lr.addEventListener('click',function(){
    var chatIdx=PANELS.findIndex(function(p){return p.id==='chat'});
    if(chatIdx!==-1)showPanel(chatIdx);
    invoke('load_rhai_rule_prompt',{
      currentModel:document.getElementById('input-model')?.value||'',
      currentSystemPrompt:document.getElementById('input-system-prompt')?.value||''
    }).then(function(p){
      setTextSafe(document.getElementById('docs-rig-log'),p.review_log_text);
      setTextSafe(document.getElementById('docs-status-text'),p.status);
      setVal('draft-input',p.draft_message);
      setVal('input-system-prompt',p.system_prompt);
      if(p.suggested_model)setVal('input-model',p.suggested_model);
    }).catch(function(){});
  });

  // Log tabs
  document.querySelectorAll('.log-tab').forEach(function(tab){
    tab.addEventListener('click',function(){
      var idx=tab.dataset.log;
      document.querySelectorAll('.log-tab').forEach(function(t){t.classList.remove('active');});
      tab.classList.add('active');
      document.getElementById('log-panel-0').classList.toggle('hidden',idx!=='0');
      document.getElementById('log-panel-1').classList.toggle('hidden',idx!=='1');
    });
  });
});

function initVizPanel(){
  if(_vizInitialized)return;
  var cy_div=document.getElementById('cy');
  if(!cy_div||typeof cytoscape==='undefined')return;
  var graphCmd=_vizActiveGraph==='type'?'get_type_graph':'get_holon_viz_graph';
  invoke(graphCmd).then(function(data){
    var elements=[];
    (data.nodes||[]).forEach(function(n){elements.push({data:n.data});});
    (data.edges||[]).forEach(function(e){elements.push({data:e.data});});
    _vizAllElements=elements;
    window._cy=cytoscape({
      container:cy_div,
      elements:elements,
      minZoom:0.18,
      maxZoom:3.0,
      layout:{name:'dagre',rankDir:'TB',nodeSep:50,rankSep:70,animate:false},
      style:[
        {selector:'node',style:{'label':'data(label)','background-color':'#1a6fa8','color':'#fff',
          'text-valign':'center','text-halign':'center','font-size':'11px',
          'width':'label','height':'label','padding':'8px','shape':'roundrectangle',
          'border-width':1,'border-color':'#0b4f71'}},
        {selector:'edge',style:{'curve-style':'bezier','target-arrow-shape':'triangle',
          'line-color':'#6f8794','target-arrow-color':'#6f8794','width':1.5}},
        {selector:'.faded',style:{'opacity':0.18,'text-opacity':0.12}},
        {selector:'.hidden-filter',style:{'display':'none'}},
        {selector:'.matched',style:{'border-width':3,'border-color':'#f28c28','z-index':999}},
        {selector:'.hide-label',style:{'label':''}},
        {selector:':selected',style:{'border-width':3,'border-color':'#f28c28','line-color':'#f28c28','target-arrow-color':'#f28c28'}},
        {selector:'node[kind="CapsuleGroup"]',style:{'background-color':'#5a3e8a'}},
        {selector:'node[kind="AuditEvent"]',style:{'background-color':'#7a3030'}},
        {selector:'node[kind="OwlClass"]',style:{'background-color':'#2e6e45'}},
        {selector:'node[kind="trait"]',style:{'background-color':'#5a3e8a','shape':'hexagon'}},
        {selector:'node[kind="enum"]',style:{'background-color':'#2e6e45','shape':'diamond'}},
        {selector:'node[kind="mcp_tool"]',style:{'background-color':'#8a6b1f','shape':'tag'}},
        {selector:'node[kind="tauri_command"]',style:{'background-color':'#7a3030','shape':'roundrectangle'}},
        {selector:'node[kind="abstract_trait"]',style:{'background-color':'#003b5c','shape':'hexagon'}},
        {selector:'node[kind="contract_type"],node[kind="dsl_contract"]',style:{'background-color':'#005d7f','shape':'roundrectangle'}},
        {selector:'node[kind="metamodel_enum"],node[kind="ontology_enum"]',style:{'background-color':'#007c89','shape':'diamond'}},
        {selector:'node[kind="z_document"]',style:{'background-color':'#5f7480','shape':'roundrectangle'}},
        {selector:'node[kind="z_pipeline"],node[kind="pipeline_state"]',style:{'background-color':'#0073a8','shape':'roundrectangle'}},
        {selector:'node[kind="z_constraint"],node[kind="constraint_type"]',style:{'background-color':'#00a0af','shape':'roundrectangle'}},
        {selector:'node[kind="z_legal"],node[kind="legal_type"]',style:{'background-color':'#c3482f','shape':'roundrectangle'}},
        {selector:'node[kind="z_proof"],node[kind="proof_result"]',style:{'background-color':'#00856f','shape':'roundrectangle'}},
        {selector:'node[kind="z_attestation"],node[kind="attestation_type"]',style:{'background-color':'#f28c28','shape':'roundrectangle','color':'#172b3a'}},
        {selector:'node[kind="solver_type"]',style:{'background-color':'#00856f','shape':'barrel'}},
        {selector:'node[kind="result_type"]',style:{'background-color':'#0097a9','shape':'round-diamond'}},
        {selector:'node[kind="issue_type"],node[kind="review_state"]',style:{'background-color':'#c3482f','shape':'octagon'}},
        {selector:'node[kind="gate_type"]',style:{'background-color':'#f28c28','shape':'vee','color':'#172b3a'}},
        {selector:'node[kind="evidence_graph"],node[kind="evidence_node"]',style:{'background-color':'#6f8794','shape':'roundrectangle'}},
        {selector:'node[kind="workbook_projection"]',style:{'background-color':'#5aa646','shape':'tag'}},
        {selector:'node[kind="taxonomy_type"]',style:{'background-color':'#7fbf3f','shape':'diamond','color':'#172b3a'}},
        {selector:'node[kind="workflow_type"]',style:{'background-color':'#005d7f','shape':'rhomboid'}},
        {selector:'node[z_layer="Pipeline"]',style:{'background-color':'#0073a8'}},
        {selector:'node[z_layer="Constraint"]',style:{'background-color':'#00a0af'}},
        {selector:'node[z_layer="Legal"]',style:{'background-color':'#c3482f'}},
        {selector:'node[z_layer="FormalProof"]',style:{'background-color':'#00856f'}},
        {selector:'node[z_layer="Attestation"]',style:{'background-color':'#f28c28','color':'#172b3a'}},
        {selector:'node[z_layer="Document"]',style:{'background-color':'#5f7480'}},
        {selector:'edge',style:{'label':'data(label)','font-size':'9px','color':'#173b4a','text-background-color':'#ffffff','text-background-opacity':0.92,'text-background-padding':'2px'}},
      ]
    });
    _vizInitialized=true;
    setupVizControls();
    setVizDetail(null);
    window._cy.ready(function(){
      setTimeout(function(){if(window._cy)window._cy.fit(window._cy.elements().not('.hidden-filter'),VIZ_FIT_PADDING);},300);
    });
    var btn=document.getElementById('btn-viz-refresh');
    if(btn)btn.addEventListener('click',function(){_vizInitialized=false;window._cy&&window._cy.destroy();initVizPanel();});
  }).catch(function(e){console.error('[viz] '+graphCmd+' failed:',e);});
}

function runVizLayout(){
  if(!window._cy)return;
  var layout=window._cy.layout({name:'dagre',rankDir:'TB',nodeSep:50,rankSep:70,animate:false});
  window._cy.one('layoutstop',function(){window._cy.fit(window._cy.elements().not('.hidden-filter'),VIZ_FIT_PADDING);});
  layout.run();
}

function zoomVizBy(factor){
  var cy=window._cy;if(!cy)return;
  cy.zoom({level:cy.zoom()*factor,renderedPosition:{x:cy.width()/2,y:cy.height()/2}});
}

function populateVizFilters(){
  var edgeSel=document.getElementById('viz-edge-filter');
  if(!edgeSel||!window._cy)return;
  edgeSel.innerHTML='<option value="">All relations</option>';
  var labels={};
  window._cy.edges().forEach(function(e){labels[e.data('label')]=true;});
  Object.keys(labels).sort().forEach(function(l){
    if(!l)return;
    var o=document.createElement('option');o.value=l;o.textContent=l;edgeSel.appendChild(o);
  });
}

function applyVizFilters(){
  var cy=window._cy;if(!cy)return;
  var query=(document.getElementById('viz-search')?.value||'').toLowerCase().trim();
  var edgeLabel=document.getElementById('viz-edge-filter')?.value||'';
  cy.elements().removeClass('hidden-filter matched faded');
  cy.nodes().forEach(function(n){
    var label=String(n.data('label')||'').toLowerCase();
    var id=String(n.data('id')||'').toLowerCase();
    var searchOk=!query||label.indexOf(query)!==-1||id.indexOf(query)!==-1;
    if(!searchOk)n.addClass('hidden-filter');
    else if(query)n.addClass('matched');
  });
  cy.edges().forEach(function(e){
    if(edgeLabel&&e.data('label')!==edgeLabel)e.addClass('hidden-filter');
    if(e.source().hasClass('hidden-filter')||e.target().hasClass('hidden-filter'))e.addClass('hidden-filter');
  });
  var visible=cy.elements().not('.hidden-filter');
  if(query||edgeLabel){
    cy.elements().not(visible).addClass('faded');
    if(visible.length>0)cy.fit(visible,VIZ_FIT_PADDING);
  }
}

function setVizDetail(ele){
  var body=document.getElementById('viz-detail-body');
  if(!body)return;
  if(!ele){
    body.textContent='Select a node or relationship.';
    return;
  }
  if(ele.isNode&&ele.isNode()){
    body.innerHTML='<div><b>'+escapeHtml(ele.data('label')||'')+'</b></div><div>'+escapeHtml(ele.data('id')||'')+'</div><div class="viz-detail-chip">'+escapeHtml(ele.data('kind')||'')+'</div>';
  }else{
    body.innerHTML='<div><b>'+escapeHtml(ele.data('label')||'relationship')+'</b></div><div>'+escapeHtml(ele.data('source')||'')+'</div><div>→</div><div>'+escapeHtml(ele.data('target')||'')+'</div>';
  }
}

function escapeHtml(s){
  return String(s).replace(/[&<>"']/g,function(c){return {'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c];});
}

function setupVizControls(){
  var cy=window._cy;if(!cy)return;
  populateVizFilters();
  document.getElementById('btn-viz-zoom-in')?.addEventListener('click',function(){zoomVizBy(1.2);});
  document.getElementById('btn-viz-zoom-out')?.addEventListener('click',function(){zoomVizBy(0.83);});
  document.getElementById('btn-viz-fit')?.addEventListener('click',function(){cy.fit(cy.elements().not('.hidden-filter'),VIZ_FIT_PADDING);});
  document.getElementById('btn-viz-reset')?.addEventListener('click',function(){cy.zoom(1);cy.center();});
  document.getElementById('btn-viz-layout')?.addEventListener('click',function(){runVizLayout();});
  document.getElementById('btn-viz-labels')?.addEventListener('click',function(){cy.nodes().toggleClass('hide-label');});
  document.getElementById('btn-viz-edge-labels')?.addEventListener('click',function(){cy.edges().toggleClass('hide-label');});
  document.getElementById('viz-search')?.addEventListener('input',applyVizFilters);
  document.getElementById('viz-edge-filter')?.addEventListener('change',applyVizFilters);
  document.getElementById('btn-viz-clear')?.addEventListener('click',function(){
    var s=document.getElementById('viz-search');if(s)s.value='';
    var e=document.getElementById('viz-edge-filter');if(e)e.value='';
    applyVizFilters();cy.fit(undefined,VIZ_FIT_PADDING);
  });
  var tabType=document.getElementById('btn-viz-tab-type');
  var tabPipeline=document.getElementById('btn-viz-tab-pipeline');
  if(tabType)tabType.addEventListener('click',function(){
    if(_vizActiveGraph==='type')return;
    _vizActiveGraph='type';
    tabType.classList.add('active');
    if(tabPipeline)tabPipeline.classList.remove('active');
    _vizInitialized=false;window._cy&&window._cy.destroy();initVizPanel();
  });
  if(tabPipeline)tabPipeline.addEventListener('click',function(){
    if(_vizActiveGraph==='pipeline')return;
    _vizActiveGraph='pipeline';
    tabPipeline.classList.add('active');
    if(tabType)tabType.classList.remove('active');
    _vizInitialized=false;window._cy&&window._cy.destroy();initVizPanel();
  });
  cy.on('tap','node,edge',function(evt){setVizDetail(evt.target);});
  cy.on('tap',function(evt){if(evt.target===cy)setVizDetail(null);});
}
