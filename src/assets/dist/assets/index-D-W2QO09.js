(function(){const n=document.createElement("link").relList;if(n&&n.supports&&n.supports("modulepreload"))return;for(const a of document.querySelectorAll('link[rel="modulepreload"]'))t(a);new MutationObserver(a=>{for(const s of a)if(s.type==="childList")for(const i of s.addedNodes)i.tagName==="LINK"&&i.rel==="modulepreload"&&t(i)}).observe(document,{childList:!0,subtree:!0});function l(a){const s={};return a.integrity&&(s.integrity=a.integrity),a.referrerPolicy&&(s.referrerPolicy=a.referrerPolicy),a.crossOrigin==="use-credentials"?s.credentials="include":a.crossOrigin==="anonymous"?s.credentials="omit":s.credentials="same-origin",s}function t(a){if(a.ep)return;a.ep=!0;const s=l(a);fetch(a.href,s)}})();const p="";async function _e(){return(await fetch(`${p}/api/sessions`)).json()}async function ee(e,{starred:n=!1,search:l="",limit:t=100,offset:a=0}={}){const s=new URLSearchParams({limit:t,offset:a});return n?s.set("starred","1"):e&&s.set("session_id",e),l&&s.set("search",l),(await fetch(`${p}/api/requests?${s}`)).json()}async function V(e){return(await fetch(`${p}/api/requests/${e}`)).json()}async function we(e){return(await fetch(`${p}/api/sessions/${e}`,{method:"DELETE"})).json()}async function Ee(e){return(await fetch(`${p}/api/requests/${e}/star`,{method:"POST"})).json()}async function re(e,n){return(await fetch(`${p}/api/requests/${e}/memo`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({memo:n})})).json()}async function xe(){return(await fetch(`${p}/api/intercept/status`)).json()}async function Se(){return(await fetch(`${p}/api/intercept/toggle`,{method:"POST"})).json()}async function Be(e){return(await fetch(`${p}/api/intercept/${e}/forward`,{method:"POST"})).json()}async function ke(e,n){return(await fetch(`${p}/api/intercept/${e}/forward-modified`,{method:"POST",headers:{"content-type":"application/json"},body:typeof n=="string"?n:JSON.stringify(n)})).json()}async function Ie(e){return(await fetch(`${p}/api/intercept/${e}/reject`,{method:"POST"})).json()}async function Le(){return(await fetch(`${p}/api/routing/config`)).json()}async function Ce(e){return(await fetch(`${p}/api/routing/config`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(e)})).json()}async function Q(){return(await fetch(`${p}/api/routing/rules`)).json()}async function Te(e){return(await fetch(`${p}/api/routing/rules`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(e)})).json()}async function le(e,n){return(await fetch(`${p}/api/routing/rules/${e}`,{method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify(n)})).json()}async function Oe(e){return(await fetch(`${p}/api/routing/rules/${e}`,{method:"DELETE"})).json()}async function oe(e){return(await fetch(`${p}/api/routing/reorder`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({ids:e})})).json()}async function Re(e,n=""){return(await fetch(`${p}/api/routing/test`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({prompt:e,system:n})})).json()}async function je(e){return(await fetch(`${p}/api/supervisor/summary/${e}`)).json()}async function Ne(e){return(await fetch(`${p}/api/supervisor/coverage/${e}`)).json()}async function Pe(e){return(await fetch(`${p}/api/supervisor/patterns/${e}`)).json()}function De(e){let n;function l(){n=new EventSource(`${p}/events`),n.addEventListener("request_update",t=>e(t)),n.addEventListener("request_intercepted",t=>e(t)),n.addEventListener("session_update",t=>e(t)),n.onerror=()=>{n.close(),setTimeout(l,2e3)}}return l(),()=>n==null?void 0:n.close()}const ce=["c0","c1","c2","c3","c4","c5","c6","c7"],Z={};let Ae=0;function ve(e){return e?(Z[e]||(Z[e]=ce[Ae++%ce.length]),Z[e]):"c0"}function X(e){return new Date(e).toLocaleTimeString("en-US",{hour12:!1})}function Me(e,n){if(e==null&&n==null)return"";const l=t=>t>=1e3?(t/1e3).toFixed(1)+"k":String(t);return`in ${e!=null?l(e):"-"} / out ${n!=null?l(n):"-"} tok`}function ge(e){return e==null?"":e>=1e3?(e/1e3).toFixed(2)+"s":e+"ms"}function r(e){return String(e??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;")}function qe(e){return e==="complete"?'<span class="status-ok">✓</span>':e==="error"?'<span class="status-err">✗</span>':e==="intercepted"?'<span class="status-intercept">⏸</span>':e==="rejected"?'<span class="status-err">⊘</span>':'<span class="status-pending">⏳</span>'}let O=[],T=[],g=null,f=null,U=!1,ye="",de=null,A=!1,K=null,y=[],C=null,z=null,ue=null;const Je=e=>{clearTimeout(ue),ue=setTimeout(()=>se(e),2e3)};document.querySelector("#app").innerHTML=`
<header class="header">
  <div class="status-dot"></div>
  <span class="header-title">Claude Code Inspector</span>
  <button class="intercept-toggle" id="interceptToggle" title="Toggle request interception">
    <span class="intercept-dot" id="interceptDot"></span>
    <span id="interceptLabel">Intercept</span>
  </button>
  <button class="routing-btn" id="routingBtn" title="Configure multi-provider routing">
    <span id="routingLabel">⇄ Routes</span>
  </button>
  <button class="supervisor-btn" id="supervisorBtn" title="Supervisor analysis for selected session">
    <span id="supervisorLabel">Supervisor</span>
  </button>
  <span class="header-meta" id="hMeta">Loading…</span>
</header>
<div class="layout">
  <nav class="sidebar">
    <div class="sidebar-section">Sessions</div>
    <div id="sessionList"></div>
  </nav>
  <div class="req-panel">
    <div class="req-panel-header">
      Requests <span id="reqCount" style="font-weight:400;color:var(--text-muted)"></span>
    </div>
    <div class="search-bar">
      <input type="text" id="searchInput" class="search-input" placeholder="Search requests…" />
    </div>
    <div class="req-list" id="reqList"></div>
  </div>
  <div class="detail" id="detail">
    <div class="detail-empty">Select a request to inspect</div>
  </div>
</div>
`;const fe=document.getElementById("interceptToggle"),Fe=document.getElementById("interceptDot"),He=document.getElementById("interceptLabel"),be=document.getElementById("routingBtn"),Ue=document.getElementById("routingLabel"),Ke=document.getElementById("supervisorBtn");document.getElementById("supervisorLabel");const ze=document.getElementById("hMeta"),F=document.getElementById("sessionList"),We=document.getElementById("reqCount"),D=document.getElementById("reqList"),R=document.getElementById("detail"),pe=document.getElementById("searchInput");pe.addEventListener("input",()=>{clearTimeout(de),de=setTimeout(()=>{ye=pe.value.trim(),M()},300)});function w(e,n="cb-code"){return`<div class="code-wrap"><button class="copy-btn">Copy</button><pre class="code ${n}">${e}</pre></div>`}R.addEventListener("click",e=>{var a;const n=e.target.closest(".copy-btn");if(!n)return;const l=(a=n.closest(".code-wrap"))==null?void 0:a.querySelector("pre");if(!l)return;const t=l.textContent;navigator.clipboard.writeText(t).catch(()=>{const s=document.createElement("textarea");s.value=t,document.body.appendChild(s),s.select(),document.execCommand("copy"),document.body.removeChild(s)}),n.textContent="Copied!",n.classList.add("copied"),setTimeout(()=>{n.textContent="Copy",n.classList.remove("copied")},1500)});function he(){Fe.className=`intercept-dot ${U?"on":"off"}`,He.textContent=U?"Intercept ON":"Intercept",fe.classList.toggle("active",U)}fe.addEventListener("click",async()=>{U=(await Se()).enabled,he()});function H(){const e=y.filter(t=>t.enabled).length,n=`⇄ Routes${e>0?" · "+e:""}`;Ue.textContent=n;const l=K&&K.enabled&&y.length>0;be.classList.toggle("active",l)}be.addEventListener("click",()=>{A=!A,A?j():f?N(f):R.innerHTML='<div class="detail-empty">Select a request to inspect</div>'});Ke.addEventListener("click",()=>{const e=g&&g!=="__starred__"?g:O.length>0?O[0].id:null;if(!e){R.innerHTML='<div class="detail-empty">No session to analyze</div>';return}A=!1,se(e)});function j(){if(!A)return;const e=K||{},n=y,l=n.map((t,a)=>`
    <div class="rule-row ${t.enabled?"":"disabled"}" data-rule-id="${r(t.id)}">
      <button class="btn btn-sm" data-rule-up="${a}" title="Move up" ${a===0?"disabled":""}>↑</button>
      <button class="btn btn-sm" data-rule-down="${a}" title="Move down" ${a===n.length-1?"disabled":""}>↓</button>
      <input type="checkbox" class="rule-enabled-cb" data-rule-id="${r(t.id)}" ${t.enabled?"checked":""} title="Enable/Disable">
      <span class="routing-badge">${r(t.category)}</span>
      <span style="flex:1;font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${r(t.target_url)}">${r(t.target_url)}</span>
      ${t.model_override?`<span style="font-size:11px;color:var(--text-muted)">${r(t.model_override)}</span>`:""}
      ${t.label?`<span style="font-size:11px;color:var(--yellow)">${r(t.label)}</span>`:""}
      <button class="btn btn-sm" data-rule-edit="${r(t.id)}">Edit</button>
      <button class="btn btn-sm btn-danger" data-rule-del="${r(t.id)}">✕</button>
    </div>
  `).join("");R.innerHTML=`
    <div class="routing-panel">
      <div class="routing-section">
        <h3>Classifier Settings</h3>
        <div class="meta-row">
          <span class="meta-label">Enabled</span>
          <input type="checkbox" id="rEnabled" ${e.enabled?"checked":""}>
        </div>
        <div class="meta-row">
          <span class="meta-label">Provider URL</span>
          <input type="text" class="memo-input" id="rBaseUrl" value="${r(e.classifier_base_url||"https://api.anthropic.com")}" style="flex:1">
        </div>
        <div class="meta-row">
          <span class="meta-label">Model</span>
          <input type="text" class="memo-input" id="rModel" value="${r(e.classifier_model||"claude-haiku-4-5-20251001")}" style="flex:1">
        </div>
        <div class="meta-row">
          <span class="meta-label">API Key</span>
          <input type="password" class="memo-input" id="rApiKey" value="${r(e.classifier_api_key||"")}" placeholder="Leave empty to use proxy key" style="flex:1">
        </div>
        <div class="meta-row" style="align-items:flex-start">
          <span class="meta-label">System Prompt</span>
          <textarea class="intercept-textarea" id="rPrompt" rows="3" style="flex:1;min-height:60px">${r(e.classifier_prompt||"")}</textarea>
        </div>
      </div>

      <div class="routing-section">
        <h3>Routing Rules <button class="btn btn-sm" id="addRuleBtn" style="margin-left:8px">+ Add Rule</button></h3>
        <div id="rulesList">${l||'<div class="empty-msg" style="padding:8px 0;font-size:12px">No rules yet</div>'}</div>
        <div id="ruleForm" style="display:none;margin-top:8px;padding:8px;background:var(--bg3);border-radius:6px;border:1px solid var(--border)">
          <div class="meta-row">
            <span class="meta-label">Category</span>
            <input type="text" class="memo-input" id="rfCategory" placeholder="code_gen" style="flex:1">
          </div>
          <div class="meta-row">
            <span class="meta-label">API Endpoint</span>
            <input type="text" class="memo-input" id="rfTargetUrl" placeholder="https://api.openai.com" style="flex:1">
          </div>
          <div class="meta-row">
            <span class="meta-label">API Key</span>
            <input type="password" class="memo-input" id="rfApiKey" placeholder="Leave empty to use proxy key" style="flex:1">
          </div>
          <div class="meta-row">
            <span class="meta-label">Model Override</span>
            <input type="text" class="memo-input" id="rfModelOverride" placeholder="gpt-4 (optional)" style="flex:1">
          </div>
          <div class="meta-row">
            <span class="meta-label">Label</span>
            <input type="text" class="memo-input" id="rfLabel" placeholder="Display name (optional)" style="flex:1">
          </div>
          <div class="meta-row" style="align-items:flex-start">
            <span class="meta-label">Description</span>
            <textarea class="memo-input" id="rfDescription" rows="2" placeholder="Describe when to use this route (helps classifier)" style="flex:1;resize:vertical"></textarea>
          </div>
          <div class="meta-row" style="align-items:flex-start">
            <span class="meta-label">Prompt Override</span>
            <textarea class="memo-input" id="rfPromptOverride" rows="3" placeholder="Optional. Use {original_prompt} to inject the original message. Leave empty to keep original." style="flex:1;resize:vertical"></textarea>
          </div>
          <div class="meta-row">
            <span class="meta-label">Enabled</span>
            <input type="checkbox" id="rfEnabled" checked>
          </div>
          <div style="display:flex;gap:8px;margin-top:8px">
            <button class="btn btn-primary" id="rfSaveBtn">Save Rule</button>
            <button class="btn" id="rfCancelBtn">Cancel</button>
          </div>
        </div>
      </div>

      <div class="routing-section">
        <h3>Test Classifier</h3>
        <div class="meta-row" style="align-items:flex-start">
          <span class="meta-label">Prompt</span>
          <textarea class="intercept-textarea" id="rTestPrompt" rows="2" style="flex:1;min-height:50px" placeholder="Enter a test prompt…"></textarea>
        </div>
        <div style="display:flex;gap:8px;margin-top:8px;align-items:center">
          <button class="btn btn-primary" id="rTestBtn">Test</button>
          <span id="rTestResult" style="font-size:13px;color:var(--text-muted)"></span>
        </div>
      </div>

      <div style="margin-top:16px;display:flex;gap:8px">
        <button class="btn btn-primary" id="rSaveAllBtn">Save All Settings</button>
        <button class="btn" id="rCloseBtn">Close</button>
      </div>
    </div>
  `,document.getElementById("rSaveAllBtn").addEventListener("click",async()=>{const t={enabled:document.getElementById("rEnabled").checked,classifier_base_url:document.getElementById("rBaseUrl").value.trim(),classifier_api_key:document.getElementById("rApiKey").value.trim(),classifier_model:document.getElementById("rModel").value.trim(),classifier_prompt:document.getElementById("rPrompt").value};K=await Ce(t),H(),j()}),document.getElementById("rCloseBtn").addEventListener("click",()=>{A=!1,f?N(f):R.innerHTML='<div class="detail-empty">Select a request to inspect</div>'}),document.getElementById("addRuleBtn").addEventListener("click",()=>{C="new",document.getElementById("ruleForm").style.display="",document.getElementById("rfCategory").value="",document.getElementById("rfTargetUrl").value="",document.getElementById("rfApiKey").value="",document.getElementById("rfModelOverride").value="",document.getElementById("rfLabel").value="",document.getElementById("rfDescription").value="",document.getElementById("rfPromptOverride").value="",document.getElementById("rfEnabled").checked=!0}),document.querySelectorAll("[data-rule-up]").forEach(t=>{t.addEventListener("click",async()=>{const a=parseInt(t.dataset.ruleUp);if(a<=0)return;const s=y.map(i=>i.id);[s[a-1],s[a]]=[s[a],s[a-1]],y=await oe(s),j()})}),document.querySelectorAll("[data-rule-down]").forEach(t=>{t.addEventListener("click",async()=>{const a=parseInt(t.dataset.ruleDown);if(a>=y.length-1)return;const s=y.map(i=>i.id);[s[a],s[a+1]]=[s[a+1],s[a]],y=await oe(s),j()})}),document.querySelectorAll(".rule-enabled-cb").forEach(t=>{t.addEventListener("change",async()=>{const a=t.dataset.ruleId,s=y.find(u=>u.id===a);if(!s)return;const i={...s,enabled:t.checked};await le(a,i),y=await Q(),H(),j()})}),document.querySelectorAll("[data-rule-edit]").forEach(t=>{t.addEventListener("click",()=>{const a=t.dataset.ruleEdit,s=y.find(i=>i.id===a);s&&(C=a,document.getElementById("ruleForm").style.display="",document.getElementById("rfCategory").value=s.category,document.getElementById("rfTargetUrl").value=s.target_url,document.getElementById("rfApiKey").value=s.api_key||"",document.getElementById("rfModelOverride").value=s.model_override||"",document.getElementById("rfLabel").value=s.label||"",document.getElementById("rfDescription").value=s.description||"",document.getElementById("rfPromptOverride").value=s.prompt_override||"",document.getElementById("rfEnabled").checked=s.enabled)})}),document.querySelectorAll("[data-rule-del]").forEach(t=>{t.addEventListener("click",async()=>{const a=t.dataset.ruleDel;confirm("Delete this routing rule?")&&(await Oe(a),y=await Q(),H(),j())})}),document.getElementById("rfSaveBtn").addEventListener("click",async()=>{const t=document.getElementById("rfCategory").value.trim(),a=document.getElementById("rfTargetUrl").value.trim();if(!t||!a){alert("Category and Target URL are required");return}const s={id:C||"",priority:0,enabled:document.getElementById("rfEnabled").checked,category:t,target_url:a,api_key:document.getElementById("rfApiKey").value.trim(),prompt_override:document.getElementById("rfPromptOverride").value,model_override:document.getElementById("rfModelOverride").value.trim(),label:document.getElementById("rfLabel").value.trim(),description:document.getElementById("rfDescription").value.trim()};C==="new"?await Te(s):C&&await le(C,{...s,id:C}),C=null,y=await Q(),H(),j()}),document.getElementById("rfCancelBtn").addEventListener("click",()=>{C=null,document.getElementById("ruleForm").style.display="none"}),document.getElementById("rTestBtn").addEventListener("click",async()=>{const t=document.getElementById("rTestPrompt").value.trim(),a=document.getElementById("rTestResult");a.textContent="Testing…";const s=await Re(t);s.error?(a.textContent=`Error: ${s.error}`,a.style.color="var(--red)"):(a.textContent=`Category: ${s.category}`,a.style.color="var(--green)")})}function $e(){const e=O.filter(s=>s.pending_count>0).length;ze.textContent=`${O.length} session${O.length!==1?"s":""}${e?` · ${e} active`:""}`;let t=`
  <div class="${g==="__starred__"?"session-item selected":"session-item"}" data-sid="__starred__">
    <div class="session-name"><span style="color:var(--yellow)">★</span> Starred</div>
  </div>
  <div class="${g===null?"session-item selected":"session-item"}" data-sid="">
    <div class="session-name"><span class="sdot live"></span>All sessions</div>
  </div>`;for(const s of O){const i=s.pending_count>0,u=g===s.id,m=s.total_input_tokens+s.total_output_tokens,E=m>0?` · ${m>=1e3?(m/1e3).toFixed(1)+"k":m} tok`:"";t+=`<div class="${u?"session-item selected":"session-item"}" data-sid="${s.id}">
      <div class="session-name">
        <span class="sdot ${i?"live":"idle"}"></span>
        ${r(s.project_name||"unknown")}
        <button class="session-del-btn" data-del-sid="${s.id}" title="Delete session">✕</button>
      </div>
      <div class="session-id" title="${s.id}">${s.id.slice(0,8)}</div>
      <div class="session-cwd">${r(s.cwd||"")}</div>
      <div class="session-stats">${s.request_count} req${E}</div>
    </div>`}const a=F.scrollTop;F.innerHTML=t,F.scrollTop=a,F.querySelectorAll("[data-sid]").forEach(s=>{s.addEventListener("click",i=>{i.target.closest("[data-del-sid]")||(g=s.dataset.sid||null,g===""&&(g=null),$e(),M(),z&&g&&g!=="__starred__"&&se(g))})}),F.querySelectorAll("[data-del-sid]").forEach(s=>{s.addEventListener("click",async i=>{i.stopPropagation();const u=s.dataset.delSid;confirm("Delete this session and all its requests?")&&(await we(u),g===u&&(g=null),await te(),await M())})})}function W(){var l;if(We.textContent=T.length?`(${T.length})`:"",!T.length){D.innerHTML='<div class="empty-msg">No requests yet</div>';return}let e="";for(const t of T){const a=(l=O.find(v=>v.id===t.session_id))==null?void 0:l.project_name,s=ve(a||"unknown"),i=Me(t.input_tokens,t.output_tokens),u=ge(t.duration_ms),m=t.id===f,E=t.id.slice(0,8);e+=`<div class="${m?"req-item selected":"req-item"}" data-rid="${t.id}">
      <div class="req-top">
        <span class="req-id" title="${t.id}">#${E}</span>
        ${t.agent_type&&t.agent_type!=="main"?`<span class="agent-badge agent-${t.agent_type}" title="${r(t.agent_task||"")}">${t.agent_type}${t.agent_task?": "+r(t.agent_task.slice(0,40))+(t.agent_task.length>40?"…":""):""}</span>`:""}
        <span class="badge ${s}">${r(a||"unknown")}</span>
        <span class="req-time">${X(t.timestamp)}</span>
        <button class="star-btn ${t.starred?"starred":""}" data-star-rid="${t.id}" title="${t.starred?"Unstar":"Star"}">${t.starred?"★":"☆"}</button>
      </div>
      <div class="req-bottom">
        ${qe(t.status)}
        ${i?`<span>${i}</span>`:""}
        ${u?`<span title="Response time">${u}</span>`:""}
        ${t.is_streaming?'<span class="req-tag streaming" title="Server-Sent Events streaming response">stream</span>':'<span class="req-tag" title="Single JSON response">json</span>'}
        ${t.memo?`<span class="req-memo" title="${r(t.memo)}">${r(t.memo)}</span>`:""}
        ${t.routing_category?`<span class="routing-badge" title="Routed: ${r(t.routing_category)}">${r(t.routing_category)}</span>`:""}
      </div>
    </div>`}const n=D.scrollTop;D.innerHTML=e,D.scrollTop=n,D.querySelectorAll("[data-rid]").forEach(t=>{t.addEventListener("click",a=>{a.target.closest("[data-star-rid]")||(f=t.dataset.rid,W(),N(f))})}),D.querySelectorAll("[data-star-rid]").forEach(t=>{t.addEventListener("click",async a=>{a.stopPropagation();const s=t.dataset.starRid,i=await Ee(s),u=T.find(m=>m.id===s);u&&(u.starred=i.starred),W()})})}function Qe(e){var a;const n={};let l={};for(const s of e.split(`
`))if(s.startsWith("data: "))try{const i=JSON.parse(s.slice(6));if(i.type==="message_start"&&(l={...((a=i.message)==null?void 0:a.usage)||{}}),i.type==="message_delta"&&i.usage&&Object.assign(l,i.usage),i.type==="content_block_start"&&(n[i.index]={type:i.content_block.type,name:i.content_block.name,_buf:""}),i.type==="content_block_delta"){const u=n[i.index];if(!u)continue;i.delta.type==="text_delta"&&(u._buf+=i.delta.text||""),i.delta.type==="input_json_delta"&&(u._buf+=i.delta.partial_json||"")}}catch{}return{blocks:Object.entries(n).sort(([s],[i])=>Number(s)-Number(i)).map(([,s])=>{if(s.type==="text")return{type:"text",text:s._buf};if(s.type==="tool_use"){let i={};try{i=JSON.parse(s._buf)}catch{i=s._buf}return{type:"tool_use",name:s.name,input:i}}return{type:s.type,raw:s._buf}}),usage:l}}function Xe(e){if(typeof e=="string")return w(r(e));switch(e.type){case"text":return w(r(e.text||""));case"tool_use":return`<div class="cb-tool-use">
        <div class="cb-label tool-use-label">call · ${r(e.name)}</div>
        ${w(r(JSON.stringify(e.input||{},null,2)))}
      </div>`;case"tool_result":{const n=Array.isArray(e.content)?e.content.map(l=>l.text||JSON.stringify(l)).join(`
`):e.content||"";return`<div class="cb-tool-result">
        <div class="cb-label tool-result-label">result · ${r(e.tool_use_id||"")}</div>
        ${w(r(n))}
      </div>`}default:return w(r(JSON.stringify(e,null,2)))}}function me(e,n,l){const t=typeof e.content=="string"?w(r(e.content)):Array.isArray(e.content)?e.content.map(Xe).join(""):w(r(JSON.stringify(e.content,null,2))),a=n!=null?`<span class="msg-num">#${n+1}${l?" · "+l:""}</span>`:"";return`<div class="msg-block"><div class="msg-role ${e.role}">${a}${e.role}</div>${t}</div>`}function Ye(e,n=0,l=[]){var ie;const t=(ie=O.find(o=>o.id===e.session_id))==null?void 0:ie.project_name,a=ve(t||"unknown");let s={},i=[],u="",m=null;try{s=JSON.parse(e.request_body),i=s.messages||[],u=s.model||"",m=s.system||null}catch{}let E=[],v={},b=null;if(e.response_body)try{const o=JSON.parse(e.response_body);if(o.raw_sse){const d=Qe(o.raw_sse);E=d.blocks,v=d.usage}else b=o}catch{}let h=E.map(o=>o.type==="text"?`<div class="msg-block">
        <div class="msg-role assistant">text</div>
        ${w(r(o.text))}
      </div>`:o.type==="tool_use"?`<div class="msg-block">
        <div class="cb-label tool-use-label">call · ${r(o.name)}</div>
        ${w(r(JSON.stringify(o.input,null,2)))}
      </div>`:`<div class="msg-block">${w(r(JSON.stringify(o,null,2)))}</div>`).join("");!h&&b&&(h=w(r(JSON.stringify(b,null,2)))),h||(h=`<div class="empty-msg" style="padding:12px 0">${e.status==="pending"?"Waiting…":"No content"}</div>`);const k=v.cache_read_input_tokens,I=v.cache_creation_input_tokens,Y=k||I?`
    <div class="meta-row">
      <span class="meta-label">Cache read</span>
      <span class="meta-val">${k??0}</span>
    </div>
    <div class="meta-row">
      <span class="meta-label">Cache write</span>
      <span class="meta-val">${I??0}</span>
    </div>`:"";let c={};try{c=JSON.parse(e.request_headers)}catch{}const _=Object.entries(c).map(([o,d])=>`  -H "${o}: ${d}"`).join(` \\
`),L=`curl -X ${e.method} http://localhost:7878${e.path} \\
${_} \\
  -H "x-api-key: $ANTHROPIC_API_KEY" \\
  -d '${e.request_body.replace(/'/g,"'\\''")}'`,x=e.status==="complete"?"status-ok":e.status==="error"?"status-err":e.status==="intercepted"?"status-intercept":e.status==="rejected"?"status-err":"status-pending",P=m?`<div class="msg-block">
    <div class="msg-role system">system</div>
    ${w(r(typeof m=="string"?m:JSON.stringify(m,null,2)))}
  </div>`:"";let S;if(e.status==="intercepted"){const o=(()=>{try{return JSON.stringify(JSON.parse(e.request_body),null,2)}catch{return e.request_body}})();S=`
      <div class="split-header">Intercepted — Edit & Forward</div>
      <div class="split-body">
        <div class="meta-row"><span class="meta-label">Status</span><span class="status-intercept" style="font-weight:500">⏸ intercepted</span></div>
        <div class="intercept-editor">
          <textarea id="interceptBody" class="intercept-textarea" spellcheck="false">${r(o)}</textarea>
          <div class="intercept-actions">
            <button class="btn" id="btnForwardOriginal">Forward Original</button>
            <button class="btn btn-primary" id="btnForwardModified">Forward Modified</button>
            <button class="btn btn-danger" id="btnReject">Reject</button>
          </div>
        </div>
      </div>`}else S=`
      <div class="split-header">Response</div>
      <div class="split-body">
        <div class="meta-row"><span class="meta-label">Status</span><span class="${x}" style="font-weight:500">${e.response_status??"-"} ${e.status}</span></div>
        <div class="meta-row"><span class="meta-label">Output tokens</span><span class="meta-val">${e.output_tokens??"-"}</span></div>
        ${Y}
        <div class="meta-row"><span class="meta-label">Duration</span><span class="meta-val">${ge(e.duration_ms)||"-"}</span></div>
        ${h}
      </div>`;R.innerHTML=`
    <div class="detail-topbar">
      <span class="req-id" title="${e.id}">#${e.id.slice(0,8)}</span>
      ${e.agent_type&&e.agent_type!=="main"?`<span class="agent-badge agent-${e.agent_type}">${e.agent_type}${e.agent_task?": "+r(e.agent_task.slice(0,60))+(e.agent_task.length>60?"…":""):""}</span>`:""}
      <span class="badge ${a}">${r(t||"unknown")}</span>
      <span class="detail-method">${e.method} ${e.path}</span>
      <span class="detail-time">${X(e.timestamp)}</span>
      <button class="btn btn-sm" id="copyCurl">Copy curl</button>
    </div>
    ${e.routing_category?`<div class="routing-meta">Routing: <span class="routing-badge">${r(e.routing_category)}</span> → ${r(e.routed_to_url||"default upstream")}</div>`:""}

    <div class="split-pane">
      <div class="split-col">
        <div class="split-header">Request</div>
        <div class="split-body">
          <div class="meta-row"><span class="meta-label">Model</span><span class="meta-val">${r(u||"-")}</span></div>
          <div class="meta-row"><span class="meta-label">Input tokens</span><span class="meta-val">${e.input_tokens??"-"}</span></div>
          ${P}
          ${n>0?`<div class="prev-messages-toggle" id="prevMsgToggle">▶ ${n} previous messages hidden</div><div id="prevMsgContainer" class="prev-messages hidden">${i.slice(0,n).map((o,d)=>me(o,d,X(l[d]))).join("")}</div><div class="new-messages-divider">── New in this request ──</div>`:""}
          ${(n>0?i.slice(n):i).map((o,d)=>{const $=n>0?n+d:d;return me(o,$,X(l[$]))}).join("")||'<div class="empty-msg" style="padding:12px 0">No messages</div>'}
        </div>
      </div>

      <div class="split-divider"></div>

      <div class="split-col">
        ${S}
      </div>
    </div>

    <div class="memo-section">
      <div class="memo-header">Memo</div>
      ${e.memo?`<div class="memo-display" id="memoDisplay">${r(e.memo)}<button class="memo-delete-btn" id="memoDeleteBtn" title="Delete memo">&times;</button></div>`:""}
      <div class="memo-form">
        <input type="text" id="memoInput" class="memo-input" placeholder="Write a memo…" value="${r(e.memo||"")}" />
        <button class="btn btn-sm" id="memoSaveBtn">Save</button>
      </div>
    </div>
  `,document.getElementById("copyCurl").addEventListener("click",()=>{navigator.clipboard.writeText(L).catch(()=>{const o=document.createElement("textarea");o.value=L,document.body.appendChild(o),o.select(),document.execCommand("copy"),document.body.removeChild(o)})});const B=document.getElementById("prevMsgToggle");B&&B.addEventListener("click",()=>{const d=document.getElementById("prevMsgContainer").classList.toggle("hidden");B.textContent=d?`▶ ${n} previous messages hidden`:`▼ ${n} previous messages shown`});const q=document.getElementById("memoInput"),J=document.getElementById("memoSaveBtn"),G=document.getElementById("memoDisplay"),ne=async()=>{const o=q.value.trim();if(J.disabled=!0,J.textContent="Saving…",await re(e.id,o),J.disabled=!1,J.textContent="Save",q.value="",G)G.textContent=o,G.style.display=o?"":"none";else if(o){const $=document.createElement("div");$.className="memo-display",$.id="memoDisplay",$.textContent=o,document.querySelector(".memo-form").before($)}const d=T.find($=>$.id===e.id);d&&(d.memo=o),W()};J.addEventListener("click",ne),q.addEventListener("keydown",o=>{o.key==="Enter"&&(o.preventDefault(),ne())});const ae=document.getElementById("memoDeleteBtn");if(ae&&ae.addEventListener("click",async()=>{await re(e.id,"");const o=T.find($=>$.id===e.id);o&&(o.memo=""),W();const d=document.getElementById("memoDisplay");d&&d.remove()}),e.status==="intercepted"){const o=async()=>{await new Promise(d=>setTimeout(d,500)),await M(),await N(e.id)};document.getElementById("btnForwardOriginal").addEventListener("click",async d=>{d.target.disabled=!0,d.target.textContent="Forwarding…",await Be(e.id),await o()}),document.getElementById("btnForwardModified").addEventListener("click",async d=>{d.target.disabled=!0,d.target.textContent="Forwarding…";const $=document.getElementById("interceptBody").value;await ke(e.id,$),await o()}),document.getElementById("btnReject").addEventListener("click",async d=>{d.target.disabled=!0,d.target.textContent="Rejecting…",await Ie(e.id),await o()})}}async function se(e){z=e,R.innerHTML='<div class="detail-empty">Loading supervisor analysis…</div>';const[n,l,t]=await Promise.all([je(e),Ne(e),Pe(e)]),a=(t.patterns||[]).length===0?'<div class="empty-msg" style="padding:8px 0">No problematic patterns detected</div>':(t.patterns||[]).map(c=>`
        <div class="sv-pattern sv-${c.severity}">
          <span class="sv-severity">${c.severity}</span>
          <span class="sv-type">${r(c.type)}</span>
          ${r(c.description)}
        </div>`).join(""),s=(l.files||[]).sort((c,_)=>(_.access_count||0)-(c.access_count||0)),i=s.filter(c=>(c.access_types||[]).includes("read")),u=s.filter(c=>c.has_full_read),m=i.filter(c=>!c.has_full_read),E=s.filter(c=>(c.access_types||[]).includes("write")||(c.access_types||[]).includes("edit")),v=s.filter(c=>(c.access_types||[]).includes("search")),b=s.filter(c=>!(c.access_types||[]).includes("read")),h=c=>!c||c.length===0?"":c.map(_=>{if(_==="full")return"full";const L={};_.split(",").forEach(S=>{const[B,q]=S.split(":");L[B]=parseInt(q)});const x=(L.offset||0)+1,P=L.limit?x+L.limit-1:"?";return`L${x}–${P}`}).join(", "),k=s.length===0?"":`
    <div class="sv-stats-bar">
      <span class="sv-stat sv-stat-good">${u.length} full read</span>
      <span class="sv-stat sv-stat-warn">${m.length} partial</span>
      <span class="sv-stat">${E.length} written</span>
      <span class="sv-stat">${v.length} searched</span>
      ${b.length>0?`<span class="sv-stat sv-stat-bad">${b.length} not read</span>`:""}
    </div>`,I=s.length===0?'<div class="empty-msg" style="padding:8px 0">No file access recorded</div>':`${k}
      <table class="sv-table">
        <tr><th>File</th><th>Type</th><th>Read Coverage</th><th>Count</th></tr>
        ${s.map(c=>{const _=c.read_ranges||[],L=(c.access_types||[]).includes("read"),x=c.total_lines>0?c.total_lines:null,P=c.lines_read||0;let S="-";if(c.has_full_read)S=`<span class="sv-full-read">full${x?` (${x}/${x})`:""}</span>`;else if(_.length>0){const B=x?` ${Math.round(P/x*100)}%`:"";S=`<span class="sv-partial-read">${h(_)} (${P}/${x||"?"})${B}</span>`}else L&&(S='<span class="sv-partial-read">partial</span>');return`<tr>
          <td class="sv-filepath">${r(c.file_path)}</td>
          <td>${(c.access_types||[]).map(B=>`<span class="sv-access-${B}">${B}</span>`).join(" ")}</td>
          <td>${S}</td>
          <td>${c.access_count}</td>
        </tr>`}).join("")}
      </table>`,Y=(n.requests||[]).map(c=>{var _;return`
    <tr>
      <td>${r(((_=c.request_id)==null?void 0:_.slice(0,8))||"")}</td>
      <td><span class="sv-agent">${r(c.agent_type||"")}</span></td>
      <td>${r(c.status||"")}</td>
      <td>${c.input_tokens??"-"}/${c.output_tokens??"-"}</td>
      <td>${c.duration_ms?c.duration_ms+"ms":"-"}</td>
    </tr>`}).join("");R.innerHTML=`
    <div class="sv-panel">
      <div class="sv-header">
        <span>Supervisor Analysis</span>
        <span class="sv-session-id" title="${e}">${e.slice(0,12)}…</span>
        <button class="btn btn-sm" id="svClose">Close</button>
      </div>

      <div class="sv-section">
        <h3>Patterns (${t.pattern_count||0})</h3>
        ${a}
      </div>

      <div class="sv-section">
        <h3>File Coverage (${l.file_count||0} files, ${l.total_accesses||0} accesses)</h3>
        ${I}
      </div>

      <div class="sv-section">
        <h3>Request Summary (${n.request_count||0} requests, ${n.total_tokens||0} tokens)</h3>
        ${n.error_count>0?`<div class="sv-pattern sv-error">${n.error_count} errors detected</div>`:""}
        <table class="sv-table">
          <tr><th>ID</th><th>Agent</th><th>Status</th><th>In/Out</th><th>Duration</th></tr>
          ${Y||'<tr><td colspan="5">No requests</td></tr>'}
        </table>
      </div>
    </div>
  `,document.getElementById("svClose").addEventListener("click",()=>{z=null,f?N(f):R.innerHTML='<div class="detail-empty">Select a request to inspect</div>'})}async function te(){O=await _e(),$e()}async function M(){g==="__starred__"?T=await ee(null,{starred:!0}):T=await ee(g,{search:ye}),W()}async function N(e){const n=await V(e);let l=0;const t=[];let a=[];try{a=JSON.parse(n.request_body).messages||[]}catch{}const s=a.length;if(n.session_id){const u=[...await ee(n.session_id,{limit:500})].sort((v,b)=>v.timestamp.localeCompare(b.timestamp)),m=u.findIndex(v=>v.id===e);let E=0;for(let v=0;v<=m&&v<u.length;v++){const b=u[v];let h,k;if(b.id===e)k=s,h=n;else try{h=await V(b.id),k=(JSON.parse(h.request_body).messages||[]).length}catch{continue}for(let I=E;I<k;I++)t[I]=h.timestamp;E=k}if(m>0){const v=u[m-1];try{const b=await V(v.id);l=(JSON.parse(b.request_body).messages||[]).length}catch{}}}for(let i=0;i<s;i++)t[i]||(t[i]=n.timestamp);Ye(n,l,t)}async function Ge(){try{U=(await xe()).enabled,he()}catch{}try{K=await Le(),y=await Q(),H()}catch{}await te(),await M(),De(e=>{var l,t;te(),M();let n=null;try{n=(t=(l=JSON.parse(e.data))==null?void 0:l.data)==null?void 0:t.id}catch{}if(e.type==="request_intercepted"&&n){f=n,N(f);return}f&&n===f&&N(f),z&&Je(z)})}Ge();
