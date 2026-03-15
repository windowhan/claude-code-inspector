(function(){const t=document.createElement("link").relList;if(t&&t.supports&&t.supports("modulepreload"))return;for(const a of document.querySelectorAll('link[rel="modulepreload"]'))s(a);new MutationObserver(a=>{for(const n of a)if(n.type==="childList")for(const i of n.addedNodes)i.tagName==="LINK"&&i.rel==="modulepreload"&&s(i)}).observe(document,{childList:!0,subtree:!0});function r(a){const n={};return a.integrity&&(n.integrity=a.integrity),a.referrerPolicy&&(n.referrerPolicy=a.referrerPolicy),a.crossOrigin==="use-credentials"?n.credentials="include":a.crossOrigin==="anonymous"?n.credentials="omit":n.credentials="same-origin",n}function s(a){if(a.ep)return;a.ep=!0;const n=r(a);fetch(a.href,n)}})();const h="";async function Ie(){return(await fetch(`${h}/api/sessions`)).json()}async function te(e,{starred:t=!1,search:r="",limit:s=100,offset:a=0}={}){const n=new URLSearchParams({limit:s,offset:a});return t?n.set("starred","1"):e&&n.set("session_id",e),r&&n.set("search",r),(await fetch(`${h}/api/requests?${n}`)).json()}async function Z(e){return(await fetch(`${h}/api/requests/${e}`)).json()}async function Ce(e){return(await fetch(`${h}/api/sessions/${e}`,{method:"DELETE"})).json()}async function Re(e){return(await fetch(`${h}/api/requests/${e}/star`,{method:"POST"})).json()}async function ue(e,t){return(await fetch(`${h}/api/requests/${e}/memo`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({memo:t})})).json()}async function qe(){return(await fetch(`${h}/api/intercept/status`)).json()}async function Te(){return(await fetch(`${h}/api/intercept/toggle`,{method:"POST"})).json()}async function Oe(e){return(await fetch(`${h}/api/intercept/${e}/forward`,{method:"POST"})).json()}async function je(e,t){return(await fetch(`${h}/api/intercept/${e}/forward-modified`,{method:"POST",headers:{"content-type":"application/json"},body:typeof t=="string"?t:JSON.stringify(t)})).json()}async function Ne(e){return(await fetch(`${h}/api/intercept/${e}/reject`,{method:"POST"})).json()}async function Ae(){return(await fetch(`${h}/api/routing/config`)).json()}async function Me(e){return(await fetch(`${h}/api/routing/config`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(e)})).json()}async function Q(){return(await fetch(`${h}/api/routing/rules`)).json()}async function Pe(e){return(await fetch(`${h}/api/routing/rules`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(e)})).json()}async function pe(e,t){return(await fetch(`${h}/api/routing/rules/${e}`,{method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify(t)})).json()}async function De(e){return(await fetch(`${h}/api/routing/rules/${e}`,{method:"DELETE"})).json()}async function me(e){return(await fetch(`${h}/api/routing/reorder`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({ids:e})})).json()}async function Je(e,t=""){return(await fetch(`${h}/api/routing/test`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({prompt:e,system:t})})).json()}async function Fe(e){return(await fetch(`${h}/api/supervisor/summary/${e}`)).json()}async function He(e){return(await fetch(`${h}/api/supervisor/coverage/${e}`)).json()}async function Ue(e){return(await fetch(`${h}/api/supervisor/patterns/${e}`)).json()}async function Ve(e){return(await fetch(`${h}/api/files/tree/${e}`)).json()}async function Ke(e,t){return(await fetch(`${h}/api/files/content/${e}?path=${encodeURIComponent(t)}`)).json()}async function ze(e,t){return(await fetch(`${h}/api/files/requests/${e}?path=${encodeURIComponent(t)}`)).json()}function We(e){let t;function r(){t=new EventSource(`${h}/events`),t.addEventListener("request_update",s=>e(s)),t.addEventListener("request_intercepted",s=>e(s)),t.addEventListener("session_update",s=>e(s)),t.onerror=()=>{t.close(),setTimeout(r,2e3)}}return r(),()=>t==null?void 0:t.close()}const ve=["c0","c1","c2","c3","c4","c5","c6","c7"],ee={};let Qe=0;function be(e){return e?(ee[e]||(ee[e]=ve[Qe++%ve.length]),ee[e]):"c0"}function X(e){return new Date(e).toLocaleTimeString("en-US",{hour12:!1})}function Xe(e,t){if(e==null&&t==null)return"";const r=s=>s>=1e3?(s/1e3).toFixed(1)+"k":String(s);return`in ${e!=null?r(e):"-"} / out ${t!=null?r(t):"-"} tok`}function _e(e){return e==null?"":e>=1e3?(e/1e3).toFixed(2)+"s":e+"ms"}function l(e){return String(e??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;")}function Ye(e){return e==="complete"?'<span class="status-ok">✓</span>':e==="error"?'<span class="status-err">✗</span>':e==="intercepted"?'<span class="status-intercept">⏸</span>':e==="rejected"?'<span class="status-err">⊘</span>':'<span class="status-pending">⏳</span>'}let k=[],O=[],$=null,B=null,V=!1,we="",ye=null,D=!1,K=null,x=[],T=null,z=null,fe=null;const Ge=e=>{clearTimeout(fe),fe=setTimeout(()=>le(e),2e3)};document.querySelector("#app").innerHTML=`
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
    <span id="supervisorLabel">&#x1F50D; Supervisor</span>
  </button>
  <button class="code-btn" id="codeBtn" title="Code Viewer - browse files with request annotations">
    <span id="codeLabel">&#x1F4C4; Code</span>
  </button>
  <button class="prompt-view-btn" id="promptViewBtn" title="Switch to Request/Response view" style="display:none">
    <span>&#x1F4AC; Prompt View</span>
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
`;const Ee=document.getElementById("interceptToggle"),Ze=document.getElementById("interceptDot"),et=document.getElementById("interceptLabel"),Se=document.getElementById("routingBtn"),tt=document.getElementById("routingLabel"),st=document.getElementById("supervisorBtn");document.getElementById("supervisorLabel");const ne=document.getElementById("codeBtn"),ae=document.getElementById("promptViewBtn"),nt=document.getElementById("hMeta"),H=document.getElementById("sessionList"),at=document.getElementById("reqCount"),P=document.getElementById("reqList"),j=document.getElementById("detail"),ge=document.getElementById("searchInput");ge.addEventListener("input",()=>{clearTimeout(ye),ye=setTimeout(()=>{we=ge.value.trim(),A()},300)});function I(e,t="cb-code"){return`<div class="code-wrap"><button class="copy-btn">Copy</button><pre class="code ${t}">${e}</pre></div>`}j.addEventListener("click",e=>{var a;const t=e.target.closest(".copy-btn");if(!t)return;const r=(a=t.closest(".code-wrap"))==null?void 0:a.querySelector("pre");if(!r)return;const s=r.textContent;navigator.clipboard.writeText(s).catch(()=>{const n=document.createElement("textarea");n.value=s,document.body.appendChild(n),n.select(),document.execCommand("copy"),document.body.removeChild(n)}),t.textContent="Copied!",t.classList.add("copied"),setTimeout(()=>{t.textContent="Copy",t.classList.remove("copied")},1500)});function xe(){Ze.className=`intercept-dot ${V?"on":"off"}`,et.textContent=V?"Intercept ON":"Intercept",Ee.classList.toggle("active",V)}Ee.addEventListener("click",async()=>{V=(await Te()).enabled,xe()});function U(){const e=x.filter(s=>s.enabled).length,t=`⇄ Routes${e>0?" · "+e:""}`;tt.textContent=t;const r=K&&K.enabled&&x.length>0;Se.classList.toggle("active",r)}Se.addEventListener("click",()=>{D=!D,D?N():B?M(B):j.innerHTML='<div class="detail-empty">Select a request to inspect</div>'});st.addEventListener("click",()=>{const e=$&&$!=="__starred__"?$:k.length>0?k[0].id:null;if(!e){j.innerHTML='<div class="detail-empty">No session to analyze</div>';return}D=!1,le(e)});let ie=null;ne.addEventListener("click",()=>{const e=$&&$!=="__starred__"?$:k.length>0?k[0].id:null;e&&Y(e)});async function Y(e,t,r){ie=e,ae.style.display="",ne.style.display="none";const s=k.find(o=>o.id===e);s!=null&&s.project_name,document.querySelector(".layout").style.display="none";let a=document.getElementById("codeViewerRoot");a||(a=document.createElement("div"),a.id="codeViewerRoot",a.className="cv-layout",document.querySelector(".layout").after(a)),a.style.display="flex",a.innerHTML=`
    <div class="cv-body">
      <nav class="sidebar" id="cvSidebar">
        <div class="sidebar-section">Sessions</div>
        <div id="cvSessionList"></div>
        <div class="cv-sidebar-actions">
          <button class="btn btn-sm cv-sidebar-btn" id="cvBack">&larr; Back</button>
        </div>
      </nav>
      <div class="cv-tree" id="cvTree"><div class="cv-loading">Loading tree…</div></div>
      <div class="cv-code" id="cvCode"><div class="cv-empty">Select a file from the tree</div></div>
      <div class="cv-timeline" id="cvTimeline"><div class="cv-empty">Click an annotation to see request details</div></div>
    </div>
  `;const n=document.getElementById("cvSessionList");let i="";for(const o of k){const y=o.id===e,c=o.pending_count>0,m=o.total_input_tokens+o.total_output_tokens,v=m>0?` · ${m>=1e3?(m/1e3).toFixed(1)+"k":m} tok`:"";i+=`<div class="${y?"session-item selected":"session-item"}" data-cv-sid="${o.id}">
      <div class="session-name"><span class="sdot ${c?"live":"idle"}"></span>${l(o.project_name||"unknown")}</div>
      <div class="session-id" title="${o.id}">${o.id.slice(0,8)}</div>
      <div class="session-cwd">${l(o.cwd||"")}</div>
      <div class="session-stats">${o.request_count} req${v}</div>
    </div>`}n.innerHTML=i,n.querySelectorAll("[data-cv-sid]").forEach(o=>{o.addEventListener("click",()=>{Y(o.dataset.cvSid)})}),document.getElementById("cvBack").addEventListener("click",Be);const u=await Ve(e);document.getElementById("cvTree").innerHTML=Le(u,e),it(e),t&&await ke(e,t,r)}function Be(){ie=null,ae.style.display="none",ne.style.display="";const e=document.getElementById("codeViewerRoot");e&&(e.style.display="none",e.innerHTML=""),document.querySelector(".layout").style.display="flex"}ae.addEventListener("click",()=>{const e=ie;Be(),e&&($=e,re(),A())});function Le(e,t,r=0){if(!Array.isArray(e)||e.length===0)return'<div class="cv-empty">No files</div>';let s='<ul class="cv-tree-list">';for(const a of e)a.type==="dir"?s+=`<li class="cv-tree-dir" style="padding-left:${r*12}px">
        <span class="cv-tree-toggle" data-expanded="false">▶ ${l(a.name)}</span>
        <div class="cv-tree-children" style="display:none">${Le(a.children||[],t,r+1)}</div>
      </li>`:s+=`<li class="cv-tree-file" style="padding-left:${r*12+16}px" data-path="${l(a.path)}" data-sid="${t}">
        ${l(a.name)}
      </li>`;return s+="</ul>",s}function it(e){document.querySelectorAll(".cv-tree-toggle").forEach(t=>{t.addEventListener("click",r=>{r.stopPropagation();const a=t.closest(".cv-tree-dir").querySelector(".cv-tree-children"),n=t.dataset.expanded==="true";a.style.display=n?"none":"block";const i=t.textContent.replace(/^[▶▼]\s*/,"");t.textContent=n?`▶ ${i}`:`▼ ${i}`,t.dataset.expanded=n?"false":"true"})}),document.querySelectorAll(".cv-tree-file").forEach(t=>{t.addEventListener("click",()=>{document.querySelectorAll(".cv-tree-file.selected").forEach(r=>r.classList.remove("selected")),t.classList.add("selected"),ke(e,t.dataset.path)})})}async function ke(e,t,r){const s=document.getElementById("cvCode");s.innerHTML='<div class="cv-loading">Loading…</div>',document.getElementById("cvTimeline").innerHTML='<div class="cv-empty">Click an annotation to see request details</div>';const[a,n]=await Promise.all([Ke(e,t),ze(e,t)]);if(a.error){s.innerHTML=`<div class="cv-empty">${l(a.error)}</div>`;return}const i=a.lines||[],u=n.requests||[],o=rt(i.length,u);let y='<div class="cv-code-inner">';for(let c=0;c<i.length;c++){const m=c+1,b=(o[m]||[]).map(g=>`<span class="cv-layer ${g.access_type==="edit"||g.access_type==="write"?"cv-edit":g.access_type==="read"?"cv-read":"cv-search"}" data-req-id="${g.request_id}" data-line="${m}" title="#${g.request_id.slice(0,6)} ${g.agent_type} ${g.access_type} ${g.timestamp.slice(11,19)}"></span>`).join("");y+=`<div class="cv-line${r===m?" cv-highlight":""}" data-line="${m}">
      <span class="cv-linenum">${m}</span>
      <span class="cv-gutter">${b}</span>
      <span class="cv-text">${l(i[c])}</span>
    </div>`}if(y+="</div>",s.innerHTML=y,r){const c=s.querySelector(`[data-line="${r}"]`);c&&c.scrollIntoView({block:"center"})}s.querySelectorAll(".cv-layer").forEach(c=>{c.addEventListener("click",m=>{m.stopPropagation();const v=parseInt(c.dataset.line),b=o[v]||[];he(v,b)})}),s.querySelectorAll(".cv-linenum").forEach(c=>{c.addEventListener("click",()=>{const m=parseInt(c.closest(".cv-line").dataset.line),v=o[m]||[];v.length>0&&he(m,v)})})}function rt(e,t){const r={};for(const s of t){let a=1,n=e;if(s.access_type!=="search"){if(s.read_range&&s.read_range!=="full"&&s.read_range!==""){const i={};s.read_range.split(",").forEach(u=>{const[o,y]=u.split(":");i[o]=parseInt(y)}),i.offset!==void 0&&(a=i.offset+1),i.limit!==void 0&&(n=a+i.limit-1)}(s.read_range==="full"||s.read_range===""||s.read_range==="default")&&(n=Math.min(e,2e3)),n=Math.min(n,e);for(let i=a;i<=n;i++)r[i]||(r[i]=[]),r[i].some(u=>u.request_id===s.request_id)||r[i].push(s)}}for(const s in r)r[s].sort((a,n)=>a.timestamp.localeCompare(n.timestamp));return r}function he(e,t){const r=document.getElementById("cvTimeline");if(t.length===0){r.innerHTML='<div class="cv-empty">No requests for this line</div>';return}let s=`<div class="cv-timeline-header">Line ${e} — ${t.length} request${t.length>1?"s":""}</div>`,a="";for(let n=0;n<t.length;n++){const i=t[n],u=n>0?t[n-1].request_body:null,o=i.access_type==="edit"||i.access_type==="write"?"cv-access-edit":i.access_type==="read"?"cv-access-read":"cv-access-search",y=ut(i.request_body),c=pt(i.response_body),m=y===a&&c?c:y,v=y===a&&c?"resp":"prompt";a=y,s+=`<div class="cv-req-card">
      <div class="cv-req-header">
        <span class="cv-req-time">${l(i.timestamp.slice(11,19))}</span>
        <span class="cv-req-id">#${l(i.request_id.slice(0,8))}</span>
        <span class="cv-req-agent">${l(i.agent_type)}</span>
        <span class="${o}">${l(i.access_type)}</span>
      </div>
      ${m?`<div class="cv-req-summary ${v==="resp"?"cv-req-summary-resp":""}">${l(m)}</div>`:""}
      ${i.agent_task?`<div class="cv-req-task">${l(i.agent_task)}</div>`:""}
      <div class="cv-req-meta">${i.input_tokens??"-"} in / ${i.output_tokens??"-"} out${i.duration_ms?" · "+i.duration_ms+"ms":""}</div>
      <details class="cv-req-details"><summary>Prompt</summary><pre class="cv-req-pre">${l(lt(i.request_body,u))}</pre></details>
      <details class="cv-req-details"><summary>Raw Prompt</summary><pre class="cv-req-pre cv-req-raw">${l(dt(i.request_body))}</pre></details>
      <details class="cv-req-details"><summary>Response</summary><pre class="cv-req-pre">${l(ct(i.response_body))}</pre></details>
    </div>`}r.innerHTML=s}function lt(e,t){try{const s=JSON.parse(e).messages||[];let a=0;if(t)try{a=(JSON.parse(t).messages||[]).length}catch{}const n=s.slice(a);n.length===0&&s.length>0&&n.push(...s.slice(-2));const i={};for(const o of n){if(o.role!=="user")continue;const y=Array.isArray(o.content)?o.content:[];for(const c of y)if(c.type==="tool_result"&&c.tool_use_id){const m=typeof c.content=="string"?c.content:Array.isArray(c.content)?c.content.map(g=>g.text||"").join(""):"",v=m.split(`
`);if(v.length>3&&/^\s*\d+→/.test(v[0])){const g=[...v].reverse().find(d=>/^\s*\d+→/.test(d)),_=g?g.match(/^\s*(\d+)→/):null;i[c.tool_use_id]=`${v.length} lines, L1-${_?_[1]:"?"}`}else m.length>200?i[c.tool_use_id]=`${m.length} chars`:i[c.tool_use_id]=m.slice(0,100)}}const u=[];a>0&&s.length>a&&u.push(`(${a} previous messages hidden)`);for(const o of n){const y=o.role||"?";if(typeof o.content=="string"){if(o.content.startsWith("<system-reminder>"))continue;u.push(`[${y}] ${o.content.slice(0,300)}`);continue}if(!Array.isArray(o.content))continue;const c=[];for(const m of o.content)if(m.type==="tool_use"){const v=m.name||"?",b=m.input||{},g=b.file_path||b.path||"",_=g?g.split("/").slice(-2).join("/"):"";if(v==="Read"&&_){const d=b.offset,w=b.limit,E=(i[m.id]||"").match(/^(\d+) lines/);if(d!=null||w!=null){const q=(d||0)+1,S=w||"?";c.push(`[Read: ${_} — L${q}-${typeof S=="number"?q+S-1:"?"} (${S} lines)]`)}else c.push(`[Read: ${_} — ${E?E[1]+" lines, full":"full"}]`)}else v==="Edit"&&_?c.push(`[Edit: ${_}]`):v==="Write"&&_?c.push(`[Write: ${_}]`):_?c.push(`[${v}: ${_}]`):c.push(`[${v}]`)}else{if(m.type==="tool_result")continue;if(m.type==="text"){const v=(m.text||"").trim();v&&!v.startsWith("<system-reminder>")&&c.push(v.length>300?v.slice(0,300)+"…":v)}}c.length>0&&u.push(`[${y}]
${c.join(`
`)}`)}return u.join(`

---

`)}catch{return e||""}}function ot(e){if(typeof e=="string")return e;if(!e||typeof e!="object")return JSON.stringify(e);if(e.type==="tool_result"){const t=typeof e.content=="string"?e.content:Array.isArray(e.content)?e.content.map(s=>s.text||"").join(""):"",r=t.split(`
`);if(r.length>5&&/^\s*\d+→/.test(r[0])){const s=r[0].match(/^\s*(\d+)→/),a=[...r].reverse().find(o=>/^\s*\d+→/.test(o)),n=a?a.match(/^\s*(\d+)→/):null,i=s?s[1]:"?",u=n?n[1]:"?";return`[tool_result: ${r.length} lines → L${i}-${u}]`}return t.length>300?`[tool_result: ${t.length} chars]
${t.slice(0,200)}…`:e.text||t||JSON.stringify(e)}if(e.type==="tool_use"){const t=e.name||"?",r=e.input||{},s=r.file_path||r.path||"";if(s){const a=s.split("/").slice(-2).join("/"),n=r.offset,i=r.limit;if(t==="Read"){if(n!=null||i!=null){const u=(n||0)+1,o=i?u+i-1:"?";return`[${t}: ${a} L${u}-${o}]`}return`[${t}: ${a} full]`}return`[${t}: ${a}]`}return`[${t}: ${JSON.stringify(r).slice(0,100)}]`}return e.text?e.text:JSON.stringify(e)}function ct(e){if(!e)return"(no response)";try{const t=JSON.parse(e);return t.accumulated_content?t.accumulated_content:t.content?Array.isArray(t.content)?t.content.map(r=>ot(r)).join(`
`):typeof t.content=="string"?t.content:JSON.stringify(t.content,null,2):JSON.stringify(t,null,2).slice(0,5e3)}catch{return(e||"").slice(0,5e3)}}function dt(e){try{const t=JSON.parse(e);return JSON.stringify(t,null,2)}catch{return e||""}}function ut(e){try{const r=JSON.parse(e).messages||[];for(let s=r.length-1;s>=0;s--){if(r[s].role!=="user")continue;const a=r[s].content;let n="";if(typeof a=="string")n=a;else if(Array.isArray(a))for(let i=a.length-1;i>=0;i--){const u=a[i].text||"";if(u&&!u.startsWith("<system-reminder>")){n=u;break}}if(n)return n=n.trim().replace(/\n+/g," "),n.length>120?n.slice(0,120)+"…":n}}catch{}return""}function pt(e){if(!e)return"";try{const t=JSON.parse(e);let r="";if(t.accumulated_content?r=t.accumulated_content:t.content&&(Array.isArray(t.content)?r=t.content.map(s=>s.text||"").filter(Boolean).join(" "):typeof t.content=="string"&&(r=t.content)),r)return r=r.trim().replace(/\n+/g," "),r.length>150?r.slice(0,150)+"…":r}catch{}return""}window.openCodeViewer=function(e,t,r){Y(e,t,r)};function N(){if(!D)return;const e=K||{},t=x,r=t.map((s,a)=>`
    <div class="rule-row ${s.enabled?"":"disabled"}" data-rule-id="${l(s.id)}">
      <button class="btn btn-sm" data-rule-up="${a}" title="Move up" ${a===0?"disabled":""}>↑</button>
      <button class="btn btn-sm" data-rule-down="${a}" title="Move down" ${a===t.length-1?"disabled":""}>↓</button>
      <input type="checkbox" class="rule-enabled-cb" data-rule-id="${l(s.id)}" ${s.enabled?"checked":""} title="Enable/Disable">
      <span class="routing-badge">${l(s.category)}</span>
      <span style="flex:1;font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${l(s.target_url)}">${l(s.target_url)}</span>
      ${s.model_override?`<span style="font-size:11px;color:var(--text-muted)">${l(s.model_override)}</span>`:""}
      ${s.label?`<span style="font-size:11px;color:var(--yellow)">${l(s.label)}</span>`:""}
      <button class="btn btn-sm" data-rule-edit="${l(s.id)}">Edit</button>
      <button class="btn btn-sm btn-danger" data-rule-del="${l(s.id)}">✕</button>
    </div>
  `).join("");j.innerHTML=`
    <div class="routing-panel">
      <div class="routing-section">
        <h3>Classifier Settings</h3>
        <div class="meta-row">
          <span class="meta-label">Enabled</span>
          <input type="checkbox" id="rEnabled" ${e.enabled?"checked":""}>
        </div>
        <div class="meta-row">
          <span class="meta-label">Provider URL</span>
          <input type="text" class="memo-input" id="rBaseUrl" value="${l(e.classifier_base_url||"https://api.anthropic.com")}" style="flex:1">
        </div>
        <div class="meta-row">
          <span class="meta-label">Model</span>
          <input type="text" class="memo-input" id="rModel" value="${l(e.classifier_model||"claude-haiku-4-5-20251001")}" style="flex:1">
        </div>
        <div class="meta-row">
          <span class="meta-label">API Key</span>
          <input type="password" class="memo-input" id="rApiKey" value="${l(e.classifier_api_key||"")}" placeholder="Leave empty to use proxy key" style="flex:1">
        </div>
        <div class="meta-row" style="align-items:flex-start">
          <span class="meta-label">System Prompt</span>
          <textarea class="intercept-textarea" id="rPrompt" rows="3" style="flex:1;min-height:60px">${l(e.classifier_prompt||"")}</textarea>
        </div>
      </div>

      <div class="routing-section">
        <h3>Routing Rules <button class="btn btn-sm" id="addRuleBtn" style="margin-left:8px">+ Add Rule</button></h3>
        <div id="rulesList">${r||'<div class="empty-msg" style="padding:8px 0;font-size:12px">No rules yet</div>'}</div>
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
  `,document.getElementById("rSaveAllBtn").addEventListener("click",async()=>{const s={enabled:document.getElementById("rEnabled").checked,classifier_base_url:document.getElementById("rBaseUrl").value.trim(),classifier_api_key:document.getElementById("rApiKey").value.trim(),classifier_model:document.getElementById("rModel").value.trim(),classifier_prompt:document.getElementById("rPrompt").value};K=await Me(s),U(),N()}),document.getElementById("rCloseBtn").addEventListener("click",()=>{D=!1,B?M(B):j.innerHTML='<div class="detail-empty">Select a request to inspect</div>'}),document.getElementById("addRuleBtn").addEventListener("click",()=>{T="new",document.getElementById("ruleForm").style.display="",document.getElementById("rfCategory").value="",document.getElementById("rfTargetUrl").value="",document.getElementById("rfApiKey").value="",document.getElementById("rfModelOverride").value="",document.getElementById("rfLabel").value="",document.getElementById("rfDescription").value="",document.getElementById("rfPromptOverride").value="",document.getElementById("rfEnabled").checked=!0}),document.querySelectorAll("[data-rule-up]").forEach(s=>{s.addEventListener("click",async()=>{const a=parseInt(s.dataset.ruleUp);if(a<=0)return;const n=x.map(i=>i.id);[n[a-1],n[a]]=[n[a],n[a-1]],x=await me(n),N()})}),document.querySelectorAll("[data-rule-down]").forEach(s=>{s.addEventListener("click",async()=>{const a=parseInt(s.dataset.ruleDown);if(a>=x.length-1)return;const n=x.map(i=>i.id);[n[a],n[a+1]]=[n[a+1],n[a]],x=await me(n),N()})}),document.querySelectorAll(".rule-enabled-cb").forEach(s=>{s.addEventListener("change",async()=>{const a=s.dataset.ruleId,n=x.find(u=>u.id===a);if(!n)return;const i={...n,enabled:s.checked};await pe(a,i),x=await Q(),U(),N()})}),document.querySelectorAll("[data-rule-edit]").forEach(s=>{s.addEventListener("click",()=>{const a=s.dataset.ruleEdit,n=x.find(i=>i.id===a);n&&(T=a,document.getElementById("ruleForm").style.display="",document.getElementById("rfCategory").value=n.category,document.getElementById("rfTargetUrl").value=n.target_url,document.getElementById("rfApiKey").value=n.api_key||"",document.getElementById("rfModelOverride").value=n.model_override||"",document.getElementById("rfLabel").value=n.label||"",document.getElementById("rfDescription").value=n.description||"",document.getElementById("rfPromptOverride").value=n.prompt_override||"",document.getElementById("rfEnabled").checked=n.enabled)})}),document.querySelectorAll("[data-rule-del]").forEach(s=>{s.addEventListener("click",async()=>{const a=s.dataset.ruleDel;confirm("Delete this routing rule?")&&(await De(a),x=await Q(),U(),N())})}),document.getElementById("rfSaveBtn").addEventListener("click",async()=>{const s=document.getElementById("rfCategory").value.trim(),a=document.getElementById("rfTargetUrl").value.trim();if(!s||!a){alert("Category and Target URL are required");return}const n={id:T||"",priority:0,enabled:document.getElementById("rfEnabled").checked,category:s,target_url:a,api_key:document.getElementById("rfApiKey").value.trim(),prompt_override:document.getElementById("rfPromptOverride").value,model_override:document.getElementById("rfModelOverride").value.trim(),label:document.getElementById("rfLabel").value.trim(),description:document.getElementById("rfDescription").value.trim()};T==="new"?await Pe(n):T&&await pe(T,{...n,id:T}),T=null,x=await Q(),U(),N()}),document.getElementById("rfCancelBtn").addEventListener("click",()=>{T=null,document.getElementById("ruleForm").style.display="none"}),document.getElementById("rTestBtn").addEventListener("click",async()=>{const s=document.getElementById("rTestPrompt").value.trim(),a=document.getElementById("rTestResult");a.textContent="Testing…";const n=await Je(s);n.error?(a.textContent=`Error: ${n.error}`,a.style.color="var(--red)"):(a.textContent=`Category: ${n.category}`,a.style.color="var(--green)")})}function re(){const e=k.filter(n=>n.pending_count>0).length;nt.textContent=`${k.length} session${k.length!==1?"s":""}${e?` · ${e} active`:""}`;let s=`
  <div class="${$==="__starred__"?"session-item selected":"session-item"}" data-sid="__starred__">
    <div class="session-name"><span style="color:var(--yellow)">★</span> Starred</div>
  </div>
  <div class="${$===null?"session-item selected":"session-item"}" data-sid="">
    <div class="session-name"><span class="sdot live"></span>All sessions</div>
  </div>`;for(const n of k){const i=n.pending_count>0,u=$===n.id,o=n.total_input_tokens+n.total_output_tokens,y=o>0?` · ${o>=1e3?(o/1e3).toFixed(1)+"k":o} tok`:"";s+=`<div class="${u?"session-item selected":"session-item"}" data-sid="${n.id}">
      <div class="session-name">
        <span class="sdot ${i?"live":"idle"}"></span>
        ${l(n.project_name||"unknown")}
        <button class="session-del-btn" data-del-sid="${n.id}" title="Delete session">✕</button>
      </div>
      <div class="session-id" title="${n.id}">${n.id.slice(0,8)}</div>
      <div class="session-cwd">${l(n.cwd||"")}</div>
      <div class="session-stats">${n.request_count} req${y}</div>
    </div>`}const a=H.scrollTop;H.innerHTML=s,H.scrollTop=a,H.querySelectorAll("[data-sid]").forEach(n=>{n.addEventListener("click",i=>{i.target.closest("[data-del-sid]")||($=n.dataset.sid||null,$===""&&($=null),re(),A(),z&&$&&$!=="__starred__"&&le($))})}),H.querySelectorAll("[data-del-sid]").forEach(n=>{n.addEventListener("click",async i=>{i.stopPropagation();const u=n.dataset.delSid;confirm("Delete this session and all its requests?")&&(await Ce(u),$===u&&($=null),await se(),await A())})})}function W(){var r;if(at.textContent=O.length?`(${O.length})`:"",!O.length){P.innerHTML='<div class="empty-msg">No requests yet</div>';return}let e="";for(const s of O){const a=(r=k.find(c=>c.id===s.session_id))==null?void 0:r.project_name,n=be(a||"unknown"),i=Xe(s.input_tokens,s.output_tokens),u=_e(s.duration_ms),o=s.id===B,y=s.id.slice(0,8);e+=`<div class="${o?"req-item selected":"req-item"}" data-rid="${s.id}">
      <div class="req-top">
        <span class="req-id" title="${s.id}">#${y}</span>
        ${s.agent_type&&s.agent_type!=="main"?`<span class="agent-badge agent-${s.agent_type}" title="${l(s.agent_task||"")}">${s.agent_type}${s.agent_task?": "+l(s.agent_task.slice(0,40))+(s.agent_task.length>40?"…":""):""}</span>`:""}
        <span class="badge ${n}">${l(a||"unknown")}</span>
        <span class="req-time">${X(s.timestamp)}</span>
        <button class="star-btn ${s.starred?"starred":""}" data-star-rid="${s.id}" title="${s.starred?"Unstar":"Star"}">${s.starred?"★":"☆"}</button>
      </div>
      <div class="req-bottom">
        ${Ye(s.status)}
        ${i?`<span>${i}</span>`:""}
        ${u?`<span title="Response time">${u}</span>`:""}
        ${s.is_streaming?'<span class="req-tag streaming" title="Server-Sent Events streaming response">stream</span>':'<span class="req-tag" title="Single JSON response">json</span>'}
        ${s.memo?`<span class="req-memo" title="${l(s.memo)}">${l(s.memo)}</span>`:""}
        ${s.routing_category?`<span class="routing-badge" title="Routed: ${l(s.routing_category)}">${l(s.routing_category)}</span>`:""}
      </div>
    </div>`}const t=P.scrollTop;P.innerHTML=e,P.scrollTop=t,P.querySelectorAll("[data-rid]").forEach(s=>{s.addEventListener("click",a=>{a.target.closest("[data-star-rid]")||(B=s.dataset.rid,W(),M(B))})}),P.querySelectorAll("[data-star-rid]").forEach(s=>{s.addEventListener("click",async a=>{a.stopPropagation();const n=s.dataset.starRid,i=await Re(n),u=O.find(o=>o.id===n);u&&(u.starred=i.starred),W()})})}function mt(e){var a;const t={};let r={};for(const n of e.split(`
`))if(n.startsWith("data: "))try{const i=JSON.parse(n.slice(6));if(i.type==="message_start"&&(r={...((a=i.message)==null?void 0:a.usage)||{}}),i.type==="message_delta"&&i.usage&&Object.assign(r,i.usage),i.type==="content_block_start"&&(t[i.index]={type:i.content_block.type,name:i.content_block.name,_buf:""}),i.type==="content_block_delta"){const u=t[i.index];if(!u)continue;i.delta.type==="text_delta"&&(u._buf+=i.delta.text||""),i.delta.type==="input_json_delta"&&(u._buf+=i.delta.partial_json||"")}}catch{}return{blocks:Object.entries(t).sort(([n],[i])=>Number(n)-Number(i)).map(([,n])=>{if(n.type==="text")return{type:"text",text:n._buf};if(n.type==="tool_use"){let i={};try{i=JSON.parse(n._buf)}catch{i=n._buf}return{type:"tool_use",name:n.name,input:i}}return{type:n.type,raw:n._buf}}),usage:r}}function vt(e){if(typeof e=="string")return I(l(e));switch(e.type){case"text":return I(l(e.text||""));case"tool_use":return`<div class="cb-tool-use">
        <div class="cb-label tool-use-label">call · ${l(e.name)}</div>
        ${I(l(JSON.stringify(e.input||{},null,2)))}
      </div>`;case"tool_result":{const t=Array.isArray(e.content)?e.content.map(r=>r.text||JSON.stringify(r)).join(`
`):e.content||"";return`<div class="cb-tool-result">
        <div class="cb-label tool-result-label">result · ${l(e.tool_use_id||"")}</div>
        ${I(l(t))}
      </div>`}default:return I(l(JSON.stringify(e,null,2)))}}function $e(e,t,r){const s=typeof e.content=="string"?I(l(e.content)):Array.isArray(e.content)?e.content.map(vt).join(""):I(l(JSON.stringify(e.content,null,2))),a=t!=null?`<span class="msg-num">#${t+1}${r?" · "+r:""}</span>`:"";return`<div class="msg-block"><div class="msg-role ${e.role}">${a}${e.role}</div>${s}</div>`}function yt(e,t=0,r=[]){var de;const s=(de=k.find(p=>p.id===e.session_id))==null?void 0:de.project_name,a=be(s||"unknown");let n={},i=[],u="",o=null;try{n=JSON.parse(e.request_body),i=n.messages||[],u=n.model||"",o=n.system||null}catch{}let y=[],c={},m=null;if(e.response_body)try{const p=JSON.parse(e.response_body);if(p.raw_sse){const f=mt(p.raw_sse);y=f.blocks,c=f.usage}else m=p}catch{}let v=y.map(p=>p.type==="text"?`<div class="msg-block">
        <div class="msg-role assistant">text</div>
        ${I(l(p.text))}
      </div>`:p.type==="tool_use"?`<div class="msg-block">
        <div class="cb-label tool-use-label">call · ${l(p.name)}</div>
        ${I(l(JSON.stringify(p.input,null,2)))}
      </div>`:`<div class="msg-block">${I(l(JSON.stringify(p,null,2)))}</div>`).join("");!v&&m&&(v=I(l(JSON.stringify(m,null,2)))),v||(v=`<div class="empty-msg" style="padding:12px 0">${e.status==="pending"?"Waiting…":"No content"}</div>`);const b=c.cache_read_input_tokens,g=c.cache_creation_input_tokens,_=b||g?`
    <div class="meta-row">
      <span class="meta-label">Cache read</span>
      <span class="meta-val">${b??0}</span>
    </div>
    <div class="meta-row">
      <span class="meta-label">Cache write</span>
      <span class="meta-val">${g??0}</span>
    </div>`:"";let d={};try{d=JSON.parse(e.request_headers)}catch{}const w=Object.entries(d).map(([p,f])=>`  -H "${p}: ${f}"`).join(` \\
`),C=`curl -X ${e.method} http://localhost:7878${e.path} \\
${w} \\
  -H "x-api-key: $ANTHROPIC_API_KEY" \\
  -d '${e.request_body.replace(/'/g,"'\\''")}'`,E=e.status==="complete"?"status-ok":e.status==="error"?"status-err":e.status==="intercepted"?"status-intercept":e.status==="rejected"?"status-err":"status-pending",q=o?`<div class="msg-block">
    <div class="msg-role system">system</div>
    ${I(l(typeof o=="string"?o:JSON.stringify(o,null,2)))}
  </div>`:"";let S;if(e.status==="intercepted"){const p=(()=>{try{return JSON.stringify(JSON.parse(e.request_body),null,2)}catch{return e.request_body}})();S=`
      <div class="split-header">Intercepted — Edit & Forward</div>
      <div class="split-body">
        <div class="meta-row"><span class="meta-label">Status</span><span class="status-intercept" style="font-weight:500">⏸ intercepted</span></div>
        <div class="intercept-editor">
          <textarea id="interceptBody" class="intercept-textarea" spellcheck="false">${l(p)}</textarea>
          <div class="intercept-actions">
            <button class="btn" id="btnForwardOriginal">Forward Original</button>
            <button class="btn btn-primary" id="btnForwardModified">Forward Modified</button>
            <button class="btn btn-danger" id="btnReject">Reject</button>
          </div>
        </div>
      </div>`}else S=`
      <div class="split-header">Response</div>
      <div class="split-body">
        <div class="meta-row"><span class="meta-label">Status</span><span class="${E}" style="font-weight:500">${e.response_status??"-"} ${e.status}</span></div>
        <div class="meta-row"><span class="meta-label">Output tokens</span><span class="meta-val">${e.output_tokens??"-"}</span></div>
        ${_}
        <div class="meta-row"><span class="meta-label">Duration</span><span class="meta-val">${_e(e.duration_ms)||"-"}</span></div>
        ${v}
      </div>`;j.innerHTML=`
    <div class="detail-topbar">
      <span class="req-id" title="${e.id}">#${e.id.slice(0,8)}</span>
      ${e.agent_type&&e.agent_type!=="main"?`<span class="agent-badge agent-${e.agent_type}">${e.agent_type}${e.agent_task?": "+l(e.agent_task.slice(0,60))+(e.agent_task.length>60?"…":""):""}</span>`:""}
      <span class="badge ${a}">${l(s||"unknown")}</span>
      <span class="detail-method">${e.method} ${e.path}</span>
      <span class="detail-time">${X(e.timestamp)}</span>
      <button class="btn btn-sm" id="copyCurl">Copy curl</button>
    </div>
    ${e.routing_category?`<div class="routing-meta">Routing: <span class="routing-badge">${l(e.routing_category)}</span> → ${l(e.routed_to_url||"default upstream")}</div>`:""}

    <div class="split-pane">
      <div class="split-col">
        <div class="split-header">Request</div>
        <div class="split-body">
          <div class="meta-row"><span class="meta-label">Model</span><span class="meta-val">${l(u||"-")}</span></div>
          <div class="meta-row"><span class="meta-label">Input tokens</span><span class="meta-val">${e.input_tokens??"-"}</span></div>
          ${q}
          ${t>0?`<div class="prev-messages-toggle" id="prevMsgToggle">▶ ${t} previous messages hidden</div><div id="prevMsgContainer" class="prev-messages hidden">${i.slice(0,t).map((p,f)=>$e(p,f,X(r[f]))).join("")}</div><div class="new-messages-divider">── New in this request ──</div>`:""}
          ${(t>0?i.slice(t):i).map((p,f)=>{const L=t>0?t+f:f;return $e(p,L,X(r[L]))}).join("")||'<div class="empty-msg" style="padding:12px 0">No messages</div>'}
        </div>
      </div>

      <div class="split-divider"></div>

      <div class="split-col">
        ${S}
      </div>
    </div>

    <div class="memo-section">
      <div class="memo-header">Memo</div>
      ${e.memo?`<div class="memo-display" id="memoDisplay">${l(e.memo)}<button class="memo-delete-btn" id="memoDeleteBtn" title="Delete memo">&times;</button></div>`:""}
      <div class="memo-form">
        <input type="text" id="memoInput" class="memo-input" placeholder="Write a memo…" value="${l(e.memo||"")}" />
        <button class="btn btn-sm" id="memoSaveBtn">Save</button>
      </div>
    </div>
  `,document.getElementById("copyCurl").addEventListener("click",()=>{navigator.clipboard.writeText(C).catch(()=>{const p=document.createElement("textarea");p.value=C,document.body.appendChild(p),p.select(),document.execCommand("copy"),document.body.removeChild(p)})});const R=document.getElementById("prevMsgToggle");R&&R.addEventListener("click",()=>{const f=document.getElementById("prevMsgContainer").classList.toggle("hidden");R.textContent=f?`▶ ${t} previous messages hidden`:`▼ ${t} previous messages shown`});const J=document.getElementById("memoInput"),F=document.getElementById("memoSaveBtn"),G=document.getElementById("memoDisplay"),oe=async()=>{const p=J.value.trim();if(F.disabled=!0,F.textContent="Saving…",await ue(e.id,p),F.disabled=!1,F.textContent="Save",J.value="",G)G.textContent=p,G.style.display=p?"":"none";else if(p){const L=document.createElement("div");L.className="memo-display",L.id="memoDisplay",L.textContent=p,document.querySelector(".memo-form").before(L)}const f=O.find(L=>L.id===e.id);f&&(f.memo=p),W()};F.addEventListener("click",oe),J.addEventListener("keydown",p=>{p.key==="Enter"&&(p.preventDefault(),oe())});const ce=document.getElementById("memoDeleteBtn");if(ce&&ce.addEventListener("click",async()=>{await ue(e.id,"");const p=O.find(L=>L.id===e.id);p&&(p.memo=""),W();const f=document.getElementById("memoDisplay");f&&f.remove()}),e.status==="intercepted"){const p=async()=>{await new Promise(f=>setTimeout(f,500)),await A(),await M(e.id)};document.getElementById("btnForwardOriginal").addEventListener("click",async f=>{f.target.disabled=!0,f.target.textContent="Forwarding…",await Oe(e.id),await p()}),document.getElementById("btnForwardModified").addEventListener("click",async f=>{f.target.disabled=!0,f.target.textContent="Forwarding…";const L=document.getElementById("interceptBody").value;await je(e.id,L),await p()}),document.getElementById("btnReject").addEventListener("click",async f=>{f.target.disabled=!0,f.target.textContent="Rejecting…",await Ne(e.id),await p()})}}async function le(e){z=e,j.innerHTML='<div class="detail-empty">Loading supervisor analysis…</div>';const[t,r,s]=await Promise.all([Fe(e),He(e),Ue(e)]),a=(s.patterns||[]).length===0?'<div class="empty-msg" style="padding:8px 0">No problematic patterns detected</div>':(s.patterns||[]).map(d=>`
        <div class="sv-pattern sv-${d.severity}">
          <span class="sv-severity">${d.severity}</span>
          <span class="sv-type">${l(d.type)}</span>
          ${l(d.description)}
        </div>`).join(""),n=(r.files||[]).sort((d,w)=>(w.access_count||0)-(d.access_count||0)),i=n.filter(d=>(d.access_types||[]).includes("read")),u=n.filter(d=>d.has_full_read),o=i.filter(d=>!d.has_full_read),y=n.filter(d=>(d.access_types||[]).includes("write")||(d.access_types||[]).includes("edit")),c=n.filter(d=>(d.access_types||[]).includes("search")),m=n.filter(d=>!(d.access_types||[]).includes("read")),v=d=>!d||d.length===0?"":d.map(w=>{if(w==="full")return"full";const C={};w.split(",").forEach(S=>{const[R,J]=S.split(":");C[R]=parseInt(J)});const E=(C.offset||0)+1,q=C.limit?E+C.limit-1:"?";return`L${E}–${q}`}).join(", "),b=n.length===0?"":`
    <div class="sv-stats-bar">
      <span class="sv-stat sv-stat-good">${u.length} full read</span>
      <span class="sv-stat sv-stat-warn">${o.length} partial</span>
      <span class="sv-stat">${y.length} written</span>
      <span class="sv-stat">${c.length} searched</span>
      ${m.length>0?`<span class="sv-stat sv-stat-bad">${m.length} not read</span>`:""}
    </div>`,g=n.length===0?'<div class="empty-msg" style="padding:8px 0">No file access recorded</div>':`${b}
      <table class="sv-table">
        <tr><th>File</th><th>Type</th><th>Read Coverage</th><th>Count</th></tr>
        ${n.map(d=>{const w=d.read_ranges||[],C=(d.access_types||[]).includes("read"),E=d.total_lines>0?d.total_lines:null,q=d.lines_read||0;let S="-";if(d.has_full_read)S=`<span class="sv-full-read">full${E?` (${E}/${E})`:""}</span>`;else if(w.length>0){const R=E?` ${Math.round(q/E*100)}%`:"";S=`<span class="sv-partial-read">${v(w)} (${q}/${E||"?"})${R}</span>`}else C&&(S='<span class="sv-partial-read">partial</span>');return`<tr>
          <td class="sv-filepath"><a href="#" class="sv-file-link" data-path="${l(d.file_path)}">${l(d.file_path)}</a></td>
          <td>${(d.access_types||[]).map(R=>`<span class="sv-access-${R}">${R}</span>`).join(" ")}</td>
          <td>${S}</td>
          <td>${d.access_count}</td>
        </tr>`}).join("")}
      </table>`,_=(t.requests||[]).map(d=>{var w;return`
    <tr>
      <td>${l(((w=d.request_id)==null?void 0:w.slice(0,8))||"")}</td>
      <td><span class="sv-agent">${l(d.agent_type||"")}</span></td>
      <td>${l(d.status||"")}</td>
      <td>${d.input_tokens??"-"}/${d.output_tokens??"-"}</td>
      <td>${d.duration_ms?d.duration_ms+"ms":"-"}</td>
    </tr>`}).join("");j.innerHTML=`
    <div class="sv-panel">
      <div class="sv-header">
        <span>Supervisor Analysis</span>
        <span class="sv-session-id" title="${e}">${e.slice(0,12)}…</span>
        <button class="btn btn-sm" id="svClose">Close</button>
      </div>

      <div class="sv-section">
        <h3>Patterns (${s.pattern_count||0})</h3>
        ${a}
      </div>

      <div class="sv-section">
        <h3>File Coverage (${r.file_count||0} files, ${r.total_accesses||0} accesses)</h3>
        ${g}
      </div>

      <div class="sv-section">
        <h3>Request Summary (${t.request_count||0} requests, ${t.total_tokens||0} tokens)</h3>
        ${t.error_count>0?`<div class="sv-pattern sv-error">${t.error_count} errors detected</div>`:""}
        <table class="sv-table">
          <tr><th>ID</th><th>Agent</th><th>Status</th><th>In/Out</th><th>Duration</th></tr>
          ${_||'<tr><td colspan="5">No requests</td></tr>'}
        </table>
      </div>
    </div>
  `,document.getElementById("svClose").addEventListener("click",()=>{z=null,B?M(B):j.innerHTML='<div class="detail-empty">Select a request to inspect</div>'}),document.querySelectorAll(".sv-file-link").forEach(d=>{d.addEventListener("click",w=>{w.preventDefault(),Y(e,d.dataset.path)})})}async function se(){k=await Ie(),re()}async function A(){$==="__starred__"?O=await te(null,{starred:!0}):O=await te($,{search:we}),W()}async function M(e){const t=await Z(e);let r=0;const s=[];let a=[];try{a=JSON.parse(t.request_body).messages||[]}catch{}const n=a.length;if(t.session_id){const u=[...await te(t.session_id,{limit:500})].sort((c,m)=>c.timestamp.localeCompare(m.timestamp)),o=u.findIndex(c=>c.id===e);let y=0;for(let c=0;c<=o&&c<u.length;c++){const m=u[c];let v,b;if(m.id===e)b=n,v=t;else try{v=await Z(m.id),b=(JSON.parse(v.request_body).messages||[]).length}catch{continue}for(let g=y;g<b;g++)s[g]=v.timestamp;y=b}if(o>0){const c=u[o-1];try{const m=await Z(c.id);r=(JSON.parse(m.request_body).messages||[]).length}catch{}}}for(let i=0;i<n;i++)s[i]||(s[i]=t.timestamp);yt(t,r,s)}async function ft(){try{V=(await qe()).enabled,xe()}catch{}try{K=await Ae(),x=await Q(),U()}catch{}await se(),await A(),We(e=>{var r,s;se(),A();let t=null;try{t=(s=(r=JSON.parse(e.data))==null?void 0:r.data)==null?void 0:s.id}catch{}if(e.type==="request_intercepted"&&t){B=t,M(B);return}B&&t===B&&M(B),z&&Ge(z)})}ft();
