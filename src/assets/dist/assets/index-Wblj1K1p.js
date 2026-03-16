(function(){const s=document.createElement("link").relList;if(s&&s.supports&&s.supports("modulepreload"))return;for(const i of document.querySelectorAll('link[rel="modulepreload"]'))t(i);new MutationObserver(i=>{for(const n of i)if(n.type==="childList")for(const a of n.addedNodes)a.tagName==="LINK"&&a.rel==="modulepreload"&&t(a)}).observe(document,{childList:!0,subtree:!0});function o(i){const n={};return i.integrity&&(n.integrity=i.integrity),i.referrerPolicy&&(n.referrerPolicy=i.referrerPolicy),i.crossOrigin==="use-credentials"?n.credentials="include":i.crossOrigin==="anonymous"?n.credentials="omit":n.credentials="same-origin",n}function t(i){if(i.ep)return;i.ep=!0;const n=o(i);fetch(i.href,n)}})();const w="";async function qe(){return(await fetch(`${w}/api/sessions`)).json()}async function ne(e,{starred:s=!1,search:o="",limit:t=100,offset:i=0}={}){const n=new URLSearchParams({limit:t,offset:i});return s?n.set("starred","1"):e&&n.set("session_id",e),o&&n.set("search",o),(await fetch(`${w}/api/requests?${n}`)).json()}async function de(e){return(await fetch(`${w}/api/requests/${e}`)).json()}async function Te(e){return(await fetch(`${w}/api/sessions/${e}`,{method:"DELETE"})).json()}async function Re(e){return(await fetch(`${w}/api/requests/${e}/star`,{method:"POST"})).json()}async function ue(e,s){return(await fetch(`${w}/api/requests/${e}/memo`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({memo:s})})).json()}async function je(){return(await fetch(`${w}/api/intercept/status`)).json()}async function Oe(){return(await fetch(`${w}/api/intercept/toggle`,{method:"POST"})).json()}async function Me(e){return(await fetch(`${w}/api/intercept/${e}/forward`,{method:"POST"})).json()}async function Ae(e,s){return(await fetch(`${w}/api/intercept/${e}/forward-modified`,{method:"POST",headers:{"content-type":"application/json"},body:typeof s=="string"?s:JSON.stringify(s)})).json()}async function Ne(e){return(await fetch(`${w}/api/intercept/${e}/reject`,{method:"POST"})).json()}async function Pe(){return(await fetch(`${w}/api/routing/config`)).json()}async function De(e){return(await fetch(`${w}/api/routing/config`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(e)})).json()}async function Z(){return(await fetch(`${w}/api/routing/rules`)).json()}async function Fe(e){return(await fetch(`${w}/api/routing/rules`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(e)})).json()}async function pe(e,s){return(await fetch(`${w}/api/routing/rules/${e}`,{method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify(s)})).json()}async function Je(e){return(await fetch(`${w}/api/routing/rules/${e}`,{method:"DELETE"})).json()}async function me(e){return(await fetch(`${w}/api/routing/reorder`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({ids:e})})).json()}async function He(e,s=""){return(await fetch(`${w}/api/routing/test`,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({prompt:e,system:s})})).json()}async function ze(e){return(await fetch(`${w}/api/supervisor/summary/${e}`)).json()}async function be(e){return(await fetch(`${w}/api/supervisor/coverage/${e}`)).json()}async function Ue(e){return(await fetch(`${w}/api/supervisor/patterns/${e}`)).json()}async function Ke(e){return(await fetch(`${w}/api/files/tree/${e}`)).json()}async function Ve(e,s){return(await fetch(`${w}/api/files/content/${e}?path=${encodeURIComponent(s)}`)).json()}async function We(e,s){return(await fetch(`${w}/api/files/requests/${e}?path=${encodeURIComponent(s)}`)).json()}function Ge(e){let s;function o(){s=new EventSource(`${w}/events`),s.addEventListener("request_update",t=>e(t)),s.addEventListener("request_intercepted",t=>e(t)),s.addEventListener("session_update",t=>e(t)),s.onerror=()=>{s.close(),setTimeout(o,2e3)}}return o(),()=>s==null?void 0:s.close()}const ve=["c0","c1","c2","c3","c4","c5","c6","c7"],se={};let Xe=0;function _e(e){return e?(se[e]||(se[e]=ve[Xe++%ve.length]),se[e]):"c0"}function ee(e){return new Date(e).toLocaleTimeString("en-US",{hour12:!1})}function Qe(e,s){if(e==null&&s==null)return"";const o=t=>t>=1e3?(t/1e3).toFixed(1)+"k":String(t);return`in ${e!=null?o(e):"-"} / out ${s!=null?o(s):"-"} tok`}function Ee(e){return e==null?"":e>=1e3?(e/1e3).toFixed(2)+"s":e+"ms"}function d(e){return String(e??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;")}function Ye(e){return e==="complete"?'<span class="status-ok">✓</span>':e==="error"?'<span class="status-err">✗</span>':e==="intercepted"?'<span class="status-intercept">⏸</span>':e==="rejected"?'<span class="status-err">⊘</span>':'<span class="status-pending">⏳</span>'}let q=[],O=[],x=null,k=null,V=!1,we="",fe=null,F=!1,W=null,I=[],j=null,G=null,ye=null;const Ze=e=>{clearTimeout(ye),ye=setTimeout(()=>ce(e),2e3)};document.querySelector("#app").innerHTML=`
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
  <button class="settings-btn" id="settingsBtn" title="Configure Summarizer LLM">&#x2699; Settings</button>
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
`;const Se=document.getElementById("interceptToggle"),et=document.getElementById("interceptDot"),tt=document.getElementById("interceptLabel"),xe=document.getElementById("routingBtn"),st=document.getElementById("routingLabel"),nt=document.getElementById("supervisorBtn");document.getElementById("supervisorLabel");const ie=document.getElementById("codeBtn"),oe=document.getElementById("promptViewBtn"),at=document.getElementById("hMeta"),U=document.getElementById("sessionList"),it=document.getElementById("reqCount"),H=document.getElementById("reqList"),R=document.getElementById("detail"),ge=document.getElementById("searchInput");ge.addEventListener("input",()=>{clearTimeout(fe),fe=setTimeout(()=>{we=ge.value.trim(),J()},300)});function T(e,s="cb-code"){return`<div class="code-wrap"><button class="copy-btn">Copy</button><pre class="code ${s}">${e}</pre></div>`}R.addEventListener("click",e=>{var i;const s=e.target.closest(".copy-btn");if(!s)return;const o=(i=s.closest(".code-wrap"))==null?void 0:i.querySelector("pre");if(!o)return;const t=o.textContent;navigator.clipboard.writeText(t).catch(()=>{const n=document.createElement("textarea");n.value=t,document.body.appendChild(n),n.select(),document.execCommand("copy"),document.body.removeChild(n)}),s.textContent="Copied!",s.classList.add("copied"),setTimeout(()=>{s.textContent="Copy",s.classList.remove("copied")},1500)});function Le(){et.className=`intercept-dot ${V?"on":"off"}`,tt.textContent=V?"Intercept ON":"Intercept",Se.classList.toggle("active",V)}Se.addEventListener("click",async()=>{V=(await Oe()).enabled,Le()});function K(){const e=I.filter(t=>t.enabled).length,s=`⇄ Routes${e>0?" · "+e:""}`;st.textContent=s;const o=W&&W.enabled&&I.length>0;xe.classList.toggle("active",o)}xe.addEventListener("click",()=>{F=!F,F?D():k?A(k):R.innerHTML='<div class="detail-empty">Select a request to inspect</div>'});document.getElementById("settingsBtn").addEventListener("click",async()=>{F=!1;const s=await(await fetch("/api/summarizer/config")).json(),o={anthropic:{url:"https://api.anthropic.com",models:["claude-haiku-4-5-20251001","claude-sonnet-4-20250514","claude-opus-4-20250514","claude-3-5-haiku-20241022","claude-3-5-sonnet-20241022"]},openai:{url:"https://api.openai.com",models:["gpt-4o-mini","gpt-4o","gpt-4.1-mini","gpt-4.1","gpt-4.1-nano","o4-mini","o3","o3-mini"]},deepseek:{url:"https://api.deepseek.com",models:["deepseek-chat","deepseek-reasoner"]},kimi:{url:"https://api.moonshot.cn",models:["moonshot-v1-8k","moonshot-v1-32k","moonshot-v1-128k"]}},t=s.provider||"anthropic";let i=null;if(s.configured)try{const c=await(await fetch("/api/summarizer/models",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({provider:t,base_url:s.base_url||o[t].url,api_key:"***"})})).json();c.models&&c.models.length>0&&(i=c.models)}catch{}const n=i||o[t].models;R.innerHTML=`
    <div class="sv-panel">
      <div class="sv-header"><span>Summarizer LLM Settings</span><button class="btn btn-sm" id="settingsClose">Close</button></div>
      <div class="sv-section">
        <div class="meta-row"><span class="meta-label">Provider</span>
          <select class="memo-input" id="sumProvider" style="flex:1">
            <option value="anthropic" ${t==="anthropic"?"selected":""}>Anthropic (Claude)</option>
            <option value="openai" ${t==="openai"?"selected":""}>OpenAI (GPT)</option>
            <option value="deepseek" ${t==="deepseek"?"selected":""}>DeepSeek</option>
            <option value="kimi" ${t==="kimi"?"selected":""}>Kimi (Moonshot)</option>
          </select>
        </div>
        <div class="meta-row"><span class="meta-label">API Endpoint</span><input type="text" class="memo-input" id="sumBaseUrl" value="${d(s.base_url||o[t].url)}" style="flex:1"></div>
        <div class="meta-row"><span class="meta-label">API Key</span><input type="text" class="memo-input" id="sumApiKey" value="${d(s.api_key||"")}" placeholder="Enter API key" style="flex:1"></div>
        <div class="meta-row"><span class="meta-label">Model</span>
          <select class="memo-input" id="sumModel" style="flex:1">
            ${n.map(a=>`<option value="${a}" ${a===(s.model||n[0])?"selected":""}>${a}</option>`).join("")}
          </select>
          <button class="btn btn-sm" id="sumFetchModels" style="margin-left:6px" title="Fetch latest models from provider">Fetch</button>
        </div>
        <div class="meta-row"><span class="meta-label">Language</span>
          <select class="memo-input" id="sumLanguage" style="flex:1">
            ${["English","Korean","Japanese","Chinese","Spanish","German","French","Russian","Portuguese"].map(a=>`<option value="${a}" ${a===(s.language||"English")?"selected":""}>${a}</option>`).join("")}
          </select>
        </div>
        <div style="margin-top:12px"><button class="btn btn-primary" id="sumSave">Save</button> <span id="sumStatus" style="font-size:12px;color:var(--text-muted)"></span></div>
      </div>
    </div>`,document.getElementById("sumProvider").addEventListener("change",a=>{const c=o[a.target.value];if(c){document.getElementById("sumBaseUrl").value=c.url;const u=document.getElementById("sumModel");u.innerHTML=c.models.map(p=>`<option value="${p}">${p}</option>`).join("")}}),document.getElementById("sumFetchModels").addEventListener("click",async()=>{const a=document.getElementById("sumFetchModels");a.disabled=!0,a.textContent="...";try{const u=await(await fetch("/api/summarizer/models",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({provider:document.getElementById("sumProvider").value,base_url:document.getElementById("sumBaseUrl").value.trim(),api_key:document.getElementById("sumApiKey").value.trim()})})).json();if(u.error)a.textContent="Error",a.title=u.error;else if(u.models&&u.models.length>0){const p=document.getElementById("sumModel"),r=p.value;p.innerHTML=u.models.map(y=>`<option value="${y}" ${y===r?"selected":""}>${y}</option>`).join(""),a.textContent=`${u.models.length} models`}else a.textContent="0 models"}catch(c){a.textContent="Error",a.title=c.message}a.disabled=!1,setTimeout(()=>{a.textContent="Fetch"},3e3)}),document.getElementById("settingsClose").addEventListener("click",()=>{k?A(k):R.innerHTML='<div class="detail-empty">Select a request to inspect</div>'}),document.getElementById("sumSave").addEventListener("click",async()=>{const a=document.getElementById("sumStatus");a.textContent="Saving…",await fetch("/api/summarizer/config",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({provider:document.getElementById("sumProvider").value,base_url:document.getElementById("sumBaseUrl").value.trim(),api_key:document.getElementById("sumApiKey").value.trim(),model:document.getElementById("sumModel").value,language:document.getElementById("sumLanguage").value})}),a.textContent="Saved!",a.style.color="var(--green)",setTimeout(()=>{a.textContent=""},2e3)})});nt.addEventListener("click",()=>{const e=x&&x!=="__starred__"?x:q.length>0?q[0].id:null;if(!e){R.innerHTML='<div class="detail-empty">No session to analyze</div>';return}F=!1,ce(e)});let le=null;ie.addEventListener("click",()=>{const e=x&&x!=="__starred__"?x:q.length>0?q[0].id:null;e&&te(e)});async function te(e,s,o){le=e,oe.style.display="",ie.style.display="none";const t=q.find(r=>r.id===e);t!=null&&t.project_name,document.querySelector(".layout").style.display="none";let i=document.getElementById("codeViewerRoot");i||(i=document.createElement("div"),i.id="codeViewerRoot",i.className="cv-layout",document.querySelector(".layout").after(i)),i.style.display="flex",i.innerHTML=`
    <div class="cv-body">
      <nav class="sidebar" id="cvSidebar">
        <div class="sidebar-section">Sessions</div>
        <div id="cvSessionList"></div>
        <div class="cv-sidebar-actions">
          <button class="btn btn-sm cv-sidebar-btn" id="cvBack">&larr; Back</button>
        </div>
      </nav>
      <div class="cv-tree" id="cvTree"><div class="cv-loading">Loading tree…</div></div>
      <div class="cv-resize-handle" data-resize="tree"></div>
      <div class="cv-code" id="cvCode"><div class="cv-empty">Select a file from the tree</div></div>
      <div class="cv-resize-handle" data-resize="timeline"></div>
      <div class="cv-timeline" id="cvTimeline"><div class="cv-empty">Click an annotation to see request details</div></div>
    </div>
  `;const n=document.getElementById("cvSessionList");let a="";for(const r of q){const y=r.id===e,v=r.pending_count>0,S=r.total_input_tokens+r.total_output_tokens,$=S>0?` · ${S>=1e3?(S/1e3).toFixed(1)+"k":S} tok`:"";a+=`<div class="${y?"session-item selected":"session-item"}" data-cv-sid="${r.id}">
      <div class="session-name"><span class="sdot ${v?"live":"idle"}"></span>${d(r.project_name||"unknown")}</div>
      <div class="session-id" title="${r.id}">${r.id.slice(0,8)}</div>
      <div class="session-cwd">${d(r.cwd||"")}</div>
      <div class="session-stats">${r.request_count} req${$}</div>
    </div>`}n.innerHTML=a,n.querySelectorAll("[data-cv-sid]").forEach(r=>{r.addEventListener("click",()=>{te(r.dataset.cvSid)})}),document.getElementById("cvBack").addEventListener("click",Be),document.querySelectorAll(".cv-resize-handle").forEach(r=>{r.addEventListener("mousedown",y=>{y.preventDefault();const v=r.dataset.resize,S=y.clientX,$=document.getElementById("cvTree"),_=document.getElementById("cvTimeline"),l=v==="tree"?$.offsetWidth:_.offsetWidth,m=h=>{const L=h.clientX-S;v==="tree"?$.style.width=Math.max(100,l+L)+"px":_.style.width=Math.max(150,l-L)+"px"},g=()=>{document.removeEventListener("mousemove",m),document.removeEventListener("mouseup",g)};document.addEventListener("mousemove",m),document.addEventListener("mouseup",g)})});const[c,u]=await Promise.all([Ke(e),be(e)]),p={};for(const r of u.files||[])p[r.file_path]={lines_read:r.lines_read||0,total_lines:r.total_lines||0,has_full_read:r.has_full_read||!1};document.getElementById("cvTree").innerHTML=ke(c,e,0,p),ot(e),s&&await Ce(e,s,o)}function Be(){le=null,oe.style.display="none",ie.style.display="";const e=document.getElementById("codeViewerRoot");e&&(e.style.display="none",e.innerHTML=""),document.querySelector(".layout").style.display="flex"}oe.addEventListener("click",()=>{const e=le;Be(),e&&(x=e,re(),J())});function ke(e,s,o=0,t={}){if(!Array.isArray(e)||e.length===0)return'<div class="cv-empty">No files</div>';let i='<ul class="cv-tree-list">';for(const n of e)if(n.type==="dir"){const a=Ie(n,t),c=a.total>0?`(${a.covered}/${a.total})`:"",u=a.total>0&&a.covered===a.total?" cv-tree-full":"";i+=`<li class="cv-tree-dir" style="padding-left:${o*12}px">
        <span class="cv-tree-toggle${u}" data-expanded="false">▶ ${c?`<span class="cv-tree-cov">${c}</span> `:""}${d(n.name)}</span>
        <div class="cv-tree-children" style="display:none">${ke(n.children||[],s,o+1,t)}</div>
      </li>`}else{const a=t[n.path],c=a?a.total_lines:n.total_lines>0?n.total_lines:0,u=a?a.lines_read:0;let p="";c>0&&u>=c&&(p=" cv-tree-full");const r=c>0?`<span class="cv-tree-cov">(${u}/${c})</span> `:"";i+=`<li class="cv-tree-file${p}" style="padding-left:${o*12+16}px" data-path="${d(n.path)}" data-sid="${s}">
        ${r}${d(n.name)}
      </li>`}return i+="</ul>",i}function Ie(e,s){let o=0,t=0;const i=e.children||[];for(const n of i)if(n.type==="dir"){const a=Ie(n,s);o+=a.covered,t+=a.total}else{const a=s[n.path],c=a?a.total_lines:n.total_lines>0?n.total_lines:0,u=a?a.lines_read:0;o+=u,t+=c}return{covered:o,total:t}}function ot(e){document.querySelectorAll(".cv-tree-toggle").forEach(s=>{s.addEventListener("click",o=>{o.stopPropagation();const i=s.closest(".cv-tree-dir").querySelector(".cv-tree-children"),n=s.dataset.expanded==="true";i.style.display=n?"none":"block";const a=s.textContent.replace(/^[▶▼]\s*/,"");s.textContent=n?`▶ ${a}`:`▼ ${a}`,s.dataset.expanded=n?"false":"true"})}),document.querySelectorAll(".cv-tree-file").forEach(s=>{s.addEventListener("click",()=>{document.querySelectorAll(".cv-tree-file.selected").forEach(o=>o.classList.remove("selected")),s.classList.add("selected"),Ce(e,s.dataset.path)})})}async function Ce(e,s,o){const t=document.getElementById("cvCode");t.innerHTML='<div class="cv-loading">Loading…</div>',document.getElementById("cvTimeline").innerHTML='<div class="cv-empty">Click an annotation to see request details</div>';const[i,n]=await Promise.all([Ve(e,s),We(e,s)]);if(i.error){t.innerHTML=`<div class="cv-empty">${d(i.error)}</div>`;return}const a=i.lines||[],c=n.requests||[],u=i.functions||[],p=i.language||null,r=lt(a.length,c),y=u.map(l=>{const m=new Map;for(let g=l.start_line;g<=l.end_line;g++)for(const h of r[g]||[])m.has(h.request_id)||m.set(h.request_id,h);return{...l,requests:[...m.values()],covered:m.size>0}}),v=y.filter(l=>l.covered).length,S=y.length;let $="";if(u.length>0){const l=new Set;c.forEach(m=>l.add(m.request_id)),$+=`<div class="cv-file-summary">
      <div class="cv-summary-header">
        <span class="cv-summary-title">${d(s.split("/").pop())}${p?` <span class="cv-lang">${p}</span>`:""}</span>
        <span class="cv-summary-stats">${v}/${S} functions covered · ${l.size} requests · ${a.length} lines</span>
      </div>
      <table class="cv-func-table">
        <tr><th>Function</th><th>Lines</th><th>Coverage</th><th>Requests</th></tr>
        ${y.map(m=>{const g=`L${m.start_line}-${m.end_line}`,h=m.covered?"cv-func-covered":"cv-func-uncovered",L=m.requests.slice(0,5).map(E=>`<span class="cv-func-req ${E.access_type==="edit"||E.access_type==="write"?"cv-edit":E.access_type==="read"?"cv-read":"cv-search"}" title="#${E.request_id.slice(0,8)} ${E.agent_type} ${E.access_type}">${E.agent_type}</span>`).join(""),B=m.requests.length>5?`<span class="cv-func-more">+${m.requests.length-5}</span>`:"";return`<tr class="cv-func-row" data-start="${m.start_line}">
            <td><span class="cv-func-name">${d(m.name)}</span> <span class="cv-func-kind">${m.kind}</span></td>
            <td class="cv-func-lines">${g}</td>
            <td><span class="${h}">${m.covered?"covered":"NOT covered"}</span></td>
            <td>${L}${B}</td>
          </tr>`}).join("")}
      </table>
    </div>`}new Set(u.map(l=>l.start_line));const _={};for(const l of y)_[l.start_line]=l;$+='<div class="cv-code-inner">';for(let l=0;l<a.length;l++){const m=l+1,g=r[m]||[],h=g.length>0?g[g.length-1]:null,L=g.length>1?g.length-1:0,B=h?(()=>{const N=h.access_type==="edit"||h.access_type==="write"?"cv-ann-edit":h.access_type==="read"?"cv-ann-read":"cv-ann-search",z=h.read_range&&h.read_range!=="full"&&h.read_range!==""?(()=>{const P={};return h.read_range.split(",").forEach(Q=>{const[Y,f]=Q.split(":");P[Y]=parseInt(f)}),` L${(P.offset||0)+1}-${(P.offset||0)+(P.limit||0)}`})():h.read_range==="full"?" full":"";return`<span class="cv-annotation ${N}" data-line="${m}">#${h.request_id.slice(0,6)} ${h.agent_type} ${h.access_type}${z}${L>0?` +${L}`:""}</span>`})():"",E=_[m],M=E?`<div class="cv-func-marker ${E.covered?"cv-func-marker-covered":"cv-func-marker-uncovered"}">${d(E.kind)}: ${d(E.name)} (L${E.start_line}-${E.end_line}) ${E.covered?`— ${E.requests.length} req`:"— NOT COVERED"}</div>`:"";$+=`${M}<div class="cv-line${o===m?" cv-highlight":""}${h?" cv-line-touched":""}" data-line="${m}">
      <span class="cv-linenum">${m}</span>
      <span class="cv-text">${d(a[l])}</span>
      ${B}
    </div>`}if($+="</div>",t.innerHTML=$,o){const l=t.querySelector(`[data-line="${o}"]`);l&&l.scrollIntoView({block:"center"})}t.querySelectorAll(".cv-annotation").forEach(l=>{l.addEventListener("click",m=>{m.stopPropagation();const g=parseInt(l.dataset.line),h=r[g]||[];he(g,h),t.querySelectorAll(".cv-annotation.active").forEach(L=>L.classList.remove("active")),l.classList.add("active")})}),t.querySelectorAll(".cv-line-touched").forEach(l=>{l.addEventListener("click",()=>{const m=parseInt(l.dataset.line),g=r[m]||[];g.length>0&&he(m,g)})}),document.querySelectorAll(".cv-func-row").forEach(l=>{l.addEventListener("click",()=>{const m=parseInt(l.dataset.start),g=t.querySelector(`[data-line="${m}"]`);g&&g.scrollIntoView({block:"center",behavior:"smooth"})})})}function lt(e,s){const o={};for(const t of s){let i=1,n=e;if(t.access_type!=="search"){if(t.read_range&&t.read_range!=="full"&&t.read_range!==""){const a={};t.read_range.split(",").forEach(c=>{const[u,p]=c.split(":");a[u]=parseInt(p)}),a.offset!==void 0&&(i=a.offset+1),a.limit!==void 0&&(n=i+a.limit-1)}(t.read_range==="full"||t.read_range===""||t.read_range==="default")&&(n=Math.min(e,2e3)),n=Math.min(n,e);for(let a=i;a<=n;a++)o[a]||(o[a]=[]),o[a].some(c=>c.request_id===t.request_id)||o[a].push(t)}}for(const t in o)o[t].sort((i,n)=>i.timestamp.localeCompare(n.timestamp));return o}function he(e,s){var u;const o=document.getElementById("cvTimeline");if(s.length===0){o.innerHTML='<div class="cv-empty">No requests for this line</div>';return}const t=new Set,n=s.filter(p=>t.has(p.request_id)?!1:(t.add(p.request_id),!0)).map(p=>{let r=0;try{r=(JSON.parse(p.request_body).messages||[]).length}catch{}return{...p,_msgCount:r}}).sort((p,r)=>p.timestamp.localeCompare(r.timestamp));window._currentTimelineData={lineNum:e,requests:n};let a=`<div class="cv-timeline-header">Line ${e} — ${n.length} request${n.length>1?"s":""} <button class="btn btn-sm cv-summarize-btn" id="cvSummarize">Summarize</button></div>`;for(let p=0;p<n.length;p++){const r=n[p],y=p>0&&((u=n.slice(0,p).sort((S,$)=>$._msgCount-S._msgCount).find(S=>S._msgCount<r._msgCount))==null?void 0:u.request_body)||null,v=r.access_type==="edit"||r.access_type==="write"?"cv-access-edit":r.access_type==="read"?"cv-access-read":"cv-access-search";pt(r.request_body),mt(r.response_body),a+=`<div class="cv-req-card">
      <div class="cv-req-header">
        <span class="cv-req-time">${d(r.timestamp.slice(11,19))}</span>
        <span class="cv-req-id">#${d(r.request_id.slice(0,8))}</span>
        <span class="cv-req-agent">${d(r.agent_type)}</span>
        <span class="${v}">${d(r.access_type)}</span>
      </div>
      <div class="cv-req-meta">${r.input_tokens??"-"} in / ${r.output_tokens??"-"} out${r.duration_ms?" · "+r.duration_ms+"ms":""}</div>
      <details class="cv-req-details"><summary>Prompt</summary><pre class="cv-req-pre">${d(rt(r.request_body,y))}</pre></details>
      <details class="cv-req-details"><summary>Raw Prompt</summary><pre class="cv-req-pre cv-req-raw">${d(ut(r.request_body))}</pre></details>
      <details class="cv-req-details"><summary>Response</summary><pre class="cv-req-pre">${d(dt(r.response_body))}</pre></details>
    </div>`}o.innerHTML=a;const c=document.getElementById("cvSummarize");c&&c.addEventListener("click",async()=>{c.disabled=!0,c.textContent="Summarizing…";try{const p=window._currentTimelineData,y=await(await fetch("/api/summarize",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({line:p.lineNum,requests:p.requests.map(v=>({request_id:v.request_id,agent_type:v.agent_type,access_type:v.access_type,read_range:v.read_range,timestamp:v.timestamp,request_body:v.request_body,response_body:v.response_body}))})})).json();if(y.error)c.textContent="Error",c.title=y.error;else{const v=document.createElement("div");v.className="cv-summary-result",v.innerHTML=`<div class="cv-summary-label">AI Summary</div><pre class="cv-req-pre">${d(y.summary)}</pre>`,o.querySelector(".cv-timeline-header").after(v),c.textContent="Summarize"}}catch(p){c.textContent="Error",c.title=p.message}c.disabled=!1})}function rt(e,s){try{const t=JSON.parse(e).messages||[];let i=0;if(s)try{i=(JSON.parse(s).messages||[]).length}catch{}const n=t.slice(i);n.length===0&&t.length>0&&n.push(...t.slice(-2));const a={};for(const u of n){if(u.role!=="user")continue;const p=Array.isArray(u.content)?u.content:[];for(const r of p)if(r.type==="tool_result"&&r.tool_use_id){const y=typeof r.content=="string"?r.content:Array.isArray(r.content)?r.content.map($=>$.text||"").join(""):"",v=y.split(`
`);if(v.length>3&&/^\s*\d+→/.test(v[0])){const $=[...v].reverse().find(l=>/^\s*\d+→/.test(l)),_=$?$.match(/^\s*(\d+)→/):null;a[r.tool_use_id]=`${v.length} lines, L1-${_?_[1]:"?"}`}else{const $=v.filter(_=>_.trim().startsWith("/")||_.includes("/"));if($.length>0){const _=$.slice(0,8).map(m=>m.trim().split("/").slice(-2).join("/")),l=$.length>8?` +${$.length-8} more`:"";a[r.tool_use_id]=`${$.length} files: ${_.join(", ")}${l}`}else y.length>200?a[r.tool_use_id]=`${y.length} chars`:a[r.tool_use_id]=y.slice(0,150)}}}const c=[];i>0&&t.length>i&&c.push(`(${i} previous messages hidden)`);for(const u of n){const p=u.role||"?";if(typeof u.content=="string"){if(u.content.startsWith("<system-reminder>"))continue;c.push(`[${p}] ${u.content.slice(0,300)}`);continue}if(!Array.isArray(u.content))continue;const r=[];for(const y of u.content)if(y.type==="tool_use"){const v=y.name||"?",S=y.input||{},$=S.file_path||S.path||"",_=$?$.split("/").slice(-2).join("/"):"";if(v==="Read"&&_){const l=S.offset,m=S.limit,h=(a[y.id]||"").match(/^(\d+) lines/);if(l!=null||m!=null){const L=(l||0)+1,B=m||"?";r.push(`[Read: ${_} — L${L}-${typeof B=="number"?L+B-1:"?"} (${B} lines)]`)}else r.push(`[Read: ${_} — ${h?h[1]+" lines, full":"full"}]`)}else if(v==="Edit"&&_)r.push(`[Edit: ${_}]`);else if(v==="Write"&&_)r.push(`[Write: ${_}]`);else if(v==="Glob"){const l=S.pattern||"*",m=$?$.split("/").slice(-2).join("/"):".",g=a[y.id]||"";r.push(`[Glob: ${m}/${l}${g?` → ${g}`:""}]`)}else if(v==="Grep"){const l=S.pattern||"",m=$?$.split("/").slice(-2).join("/"):".",g=a[y.id]||"";r.push(`[Grep: "${l}" in ${m}${g?` → ${g}`:""}]`)}else if(_)r.push(`[${v}: ${_}]`);else{const l=a[y.id]||"";r.push(`[${v}${l?`: ${l}`:""}]`)}}else{if(y.type==="tool_result")continue;if(y.type==="text"){const v=(y.text||"").trim();v&&!v.startsWith("<system-reminder>")&&r.push(v.length>300?v.slice(0,300)+"…":v)}}r.length>0&&c.push(`[${p}]
${r.join(`
`)}`)}return c.join(`

---

`)}catch{return e||""}}function ct(e){if(typeof e=="string")return e;if(!e||typeof e!="object")return JSON.stringify(e);if(e.type==="tool_result"){const s=typeof e.content=="string"?e.content:Array.isArray(e.content)?e.content.map(t=>t.text||"").join(""):"",o=s.split(`
`);if(o.length>5&&/^\s*\d+→/.test(o[0])){const t=o[0].match(/^\s*(\d+)→/),i=[...o].reverse().find(u=>/^\s*\d+→/.test(u)),n=i?i.match(/^\s*(\d+)→/):null,a=t?t[1]:"?",c=n?n[1]:"?";return`[tool_result: ${o.length} lines → L${a}-${c}]`}return s.length>300?`[tool_result: ${s.length} chars]
${s.slice(0,200)}…`:e.text||s||JSON.stringify(e)}if(e.type==="tool_use"){const s=e.name||"?",o=e.input||{},t=o.file_path||o.path||"";if(t){const i=t.split("/").slice(-2).join("/"),n=o.offset,a=o.limit;if(s==="Read"){if(n!=null||a!=null){const c=(n||0)+1,u=a?c+a-1:"?";return`[${s}: ${i} L${c}-${u}]`}return`[${s}: ${i} full]`}return`[${s}: ${i}]`}return`[${s}: ${JSON.stringify(o).slice(0,100)}]`}return e.text?e.text:JSON.stringify(e)}function dt(e){if(!e)return"(no response)";try{const s=JSON.parse(e);return s.accumulated_content?s.accumulated_content:s.content?Array.isArray(s.content)?s.content.map(o=>ct(o)).join(`
`):typeof s.content=="string"?s.content:JSON.stringify(s.content,null,2):JSON.stringify(s,null,2).slice(0,5e3)}catch{return(e||"").slice(0,5e3)}}function ut(e){try{const s=JSON.parse(e);return JSON.stringify(s,null,2)}catch{return e||""}}function pt(e){try{const o=JSON.parse(e).messages||[];for(let t=o.length-1;t>=0;t--){if(o[t].role!=="user")continue;const i=o[t].content;let n="";if(typeof i=="string")n=i;else if(Array.isArray(i))for(let a=i.length-1;a>=0;a--){const c=i[a].text||"";if(c&&!c.startsWith("<system-reminder>")){n=c;break}}if(n)return n=n.trim().replace(/\n+/g," "),n.length>120?n.slice(0,120)+"…":n}}catch{}return""}function mt(e){if(!e)return"";try{const s=JSON.parse(e);let o="";if(s.accumulated_content?o=s.accumulated_content:s.content&&(Array.isArray(s.content)?o=s.content.map(t=>t.text||"").filter(Boolean).join(" "):typeof s.content=="string"&&(o=s.content)),o)return o=o.trim().replace(/\n+/g," "),o.length>150?o.slice(0,150)+"…":o}catch{}return""}window.openCodeViewer=function(e,s,o){te(e,s,o)};function D(){if(!F)return;const e=W||{},s=I,o=s.map((t,i)=>`
    <div class="rule-row ${t.enabled?"":"disabled"}" data-rule-id="${d(t.id)}">
      <button class="btn btn-sm" data-rule-up="${i}" title="Move up" ${i===0?"disabled":""}>↑</button>
      <button class="btn btn-sm" data-rule-down="${i}" title="Move down" ${i===s.length-1?"disabled":""}>↓</button>
      <input type="checkbox" class="rule-enabled-cb" data-rule-id="${d(t.id)}" ${t.enabled?"checked":""} title="Enable/Disable">
      <span class="routing-badge">${d(t.category)}</span>
      <span style="flex:1;font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${d(t.target_url)}">${d(t.target_url)}</span>
      ${t.model_override?`<span style="font-size:11px;color:var(--text-muted)">${d(t.model_override)}</span>`:""}
      ${t.label?`<span style="font-size:11px;color:var(--yellow)">${d(t.label)}</span>`:""}
      <button class="btn btn-sm" data-rule-edit="${d(t.id)}">Edit</button>
      <button class="btn btn-sm btn-danger" data-rule-del="${d(t.id)}">✕</button>
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
          <input type="text" class="memo-input" id="rBaseUrl" value="${d(e.classifier_base_url||"https://api.anthropic.com")}" style="flex:1">
        </div>
        <div class="meta-row">
          <span class="meta-label">Model</span>
          <input type="text" class="memo-input" id="rModel" value="${d(e.classifier_model||"claude-haiku-4-5-20251001")}" style="flex:1">
        </div>
        <div class="meta-row">
          <span class="meta-label">API Key</span>
          <input type="password" class="memo-input" id="rApiKey" value="${d(e.classifier_api_key||"")}" placeholder="Leave empty to use proxy key" style="flex:1">
        </div>
        <div class="meta-row" style="align-items:flex-start">
          <span class="meta-label">System Prompt</span>
          <textarea class="intercept-textarea" id="rPrompt" rows="3" style="flex:1;min-height:60px">${d(e.classifier_prompt||"")}</textarea>
        </div>
      </div>

      <div class="routing-section">
        <h3>Routing Rules <button class="btn btn-sm" id="addRuleBtn" style="margin-left:8px">+ Add Rule</button></h3>
        <div id="rulesList">${o||'<div class="empty-msg" style="padding:8px 0;font-size:12px">No rules yet</div>'}</div>
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
  `,document.getElementById("rSaveAllBtn").addEventListener("click",async()=>{const t={enabled:document.getElementById("rEnabled").checked,classifier_base_url:document.getElementById("rBaseUrl").value.trim(),classifier_api_key:document.getElementById("rApiKey").value.trim(),classifier_model:document.getElementById("rModel").value.trim(),classifier_prompt:document.getElementById("rPrompt").value};W=await De(t),K(),D()}),document.getElementById("rCloseBtn").addEventListener("click",()=>{F=!1,k?A(k):R.innerHTML='<div class="detail-empty">Select a request to inspect</div>'}),document.getElementById("addRuleBtn").addEventListener("click",()=>{j="new",document.getElementById("ruleForm").style.display="",document.getElementById("rfCategory").value="",document.getElementById("rfTargetUrl").value="",document.getElementById("rfApiKey").value="",document.getElementById("rfModelOverride").value="",document.getElementById("rfLabel").value="",document.getElementById("rfDescription").value="",document.getElementById("rfPromptOverride").value="",document.getElementById("rfEnabled").checked=!0}),document.querySelectorAll("[data-rule-up]").forEach(t=>{t.addEventListener("click",async()=>{const i=parseInt(t.dataset.ruleUp);if(i<=0)return;const n=I.map(a=>a.id);[n[i-1],n[i]]=[n[i],n[i-1]],I=await me(n),D()})}),document.querySelectorAll("[data-rule-down]").forEach(t=>{t.addEventListener("click",async()=>{const i=parseInt(t.dataset.ruleDown);if(i>=I.length-1)return;const n=I.map(a=>a.id);[n[i],n[i+1]]=[n[i+1],n[i]],I=await me(n),D()})}),document.querySelectorAll(".rule-enabled-cb").forEach(t=>{t.addEventListener("change",async()=>{const i=t.dataset.ruleId,n=I.find(c=>c.id===i);if(!n)return;const a={...n,enabled:t.checked};await pe(i,a),I=await Z(),K(),D()})}),document.querySelectorAll("[data-rule-edit]").forEach(t=>{t.addEventListener("click",()=>{const i=t.dataset.ruleEdit,n=I.find(a=>a.id===i);n&&(j=i,document.getElementById("ruleForm").style.display="",document.getElementById("rfCategory").value=n.category,document.getElementById("rfTargetUrl").value=n.target_url,document.getElementById("rfApiKey").value=n.api_key||"",document.getElementById("rfModelOverride").value=n.model_override||"",document.getElementById("rfLabel").value=n.label||"",document.getElementById("rfDescription").value=n.description||"",document.getElementById("rfPromptOverride").value=n.prompt_override||"",document.getElementById("rfEnabled").checked=n.enabled)})}),document.querySelectorAll("[data-rule-del]").forEach(t=>{t.addEventListener("click",async()=>{const i=t.dataset.ruleDel;confirm("Delete this routing rule?")&&(await Je(i),I=await Z(),K(),D())})}),document.getElementById("rfSaveBtn").addEventListener("click",async()=>{const t=document.getElementById("rfCategory").value.trim(),i=document.getElementById("rfTargetUrl").value.trim();if(!t||!i){alert("Category and Target URL are required");return}const n={id:j||"",priority:0,enabled:document.getElementById("rfEnabled").checked,category:t,target_url:i,api_key:document.getElementById("rfApiKey").value.trim(),prompt_override:document.getElementById("rfPromptOverride").value,model_override:document.getElementById("rfModelOverride").value.trim(),label:document.getElementById("rfLabel").value.trim(),description:document.getElementById("rfDescription").value.trim()};j==="new"?await Fe(n):j&&await pe(j,{...n,id:j}),j=null,I=await Z(),K(),D()}),document.getElementById("rfCancelBtn").addEventListener("click",()=>{j=null,document.getElementById("ruleForm").style.display="none"}),document.getElementById("rTestBtn").addEventListener("click",async()=>{const t=document.getElementById("rTestPrompt").value.trim(),i=document.getElementById("rTestResult");i.textContent="Testing…";const n=await He(t);n.error?(i.textContent=`Error: ${n.error}`,i.style.color="var(--red)"):(i.textContent=`Category: ${n.category}`,i.style.color="var(--green)")})}function re(){const e=q.filter(n=>n.pending_count>0).length;at.textContent=`${q.length} session${q.length!==1?"s":""}${e?` · ${e} active`:""}`;let t=`
  <div class="${x==="__starred__"?"session-item selected":"session-item"}" data-sid="__starred__">
    <div class="session-name"><span style="color:var(--yellow)">★</span> Starred</div>
  </div>
  <div class="${x===null?"session-item selected":"session-item"}" data-sid="">
    <div class="session-name"><span class="sdot live"></span>All sessions</div>
  </div>`;for(const n of q){const a=n.pending_count>0,c=x===n.id,u=n.total_input_tokens+n.total_output_tokens,p=u>0?` · ${u>=1e3?(u/1e3).toFixed(1)+"k":u} tok`:"";t+=`<div class="${c?"session-item selected":"session-item"}" data-sid="${n.id}">
      <div class="session-name">
        <span class="sdot ${a?"live":"idle"}"></span>
        ${d(n.project_name||"unknown")}
        <button class="session-del-btn" data-del-sid="${n.id}" title="Delete session">✕</button>
      </div>
      <div class="session-id" title="${n.id}">${n.id.slice(0,8)}</div>
      <div class="session-cwd">${d(n.cwd||"")}</div>
      <div class="session-stats">${n.request_count} req${p}</div>
    </div>`}const i=U.scrollTop;U.innerHTML=t,U.scrollTop=i,U.querySelectorAll("[data-sid]").forEach(n=>{n.addEventListener("click",a=>{a.target.closest("[data-del-sid]")||(x=n.dataset.sid||null,x===""&&(x=null),re(),J(),G&&x&&x!=="__starred__"&&ce(x))})}),U.querySelectorAll("[data-del-sid]").forEach(n=>{n.addEventListener("click",async a=>{a.stopPropagation();const c=n.dataset.delSid;confirm("Delete this session and all its requests?")&&(await Te(c),x===c&&(x=null),await ae(),await J())})})}function X(){var o;if(it.textContent=O.length?`(${O.length})`:"",!O.length){H.innerHTML='<div class="empty-msg">No requests yet</div>';return}let e="";for(const t of O){const i=(o=q.find(r=>r.id===t.session_id))==null?void 0:o.project_name,n=_e(i||"unknown"),a=Qe(t.input_tokens,t.output_tokens),c=Ee(t.duration_ms),u=t.id===k,p=t.id.slice(0,8);e+=`<div class="${u?"req-item selected":"req-item"}" data-rid="${t.id}">
      <div class="req-top">
        <span class="req-id" title="${t.id}">#${p}</span>
        ${t.agent_type&&t.agent_type!=="main"?`<span class="agent-badge agent-${t.agent_type}" title="${d(t.agent_task||"")}">${t.agent_type}${t.agent_task?": "+d(t.agent_task.slice(0,40))+(t.agent_task.length>40?"…":""):""}</span>`:""}
        <span class="badge ${n}">${d(i||"unknown")}</span>
        <span class="req-time">${ee(t.timestamp)}</span>
        <button class="star-btn ${t.starred?"starred":""}" data-star-rid="${t.id}" title="${t.starred?"Unstar":"Star"}">${t.starred?"★":"☆"}</button>
      </div>
      <div class="req-bottom">
        ${Ye(t.status)}
        ${a?`<span>${a}</span>`:""}
        ${c?`<span title="Response time">${c}</span>`:""}
        ${t.is_streaming?'<span class="req-tag streaming" title="Server-Sent Events streaming response">stream</span>':'<span class="req-tag" title="Single JSON response">json</span>'}
        ${t.memo?`<span class="req-memo" title="${d(t.memo)}">${d(t.memo)}</span>`:""}
        ${t.routing_category?`<span class="routing-badge" title="Routed: ${d(t.routing_category)}">${d(t.routing_category)}</span>`:""}
      </div>
    </div>`}const s=H.scrollTop;H.innerHTML=e,H.scrollTop=s,H.querySelectorAll("[data-rid]").forEach(t=>{t.addEventListener("click",i=>{i.target.closest("[data-star-rid]")||(k=t.dataset.rid,X(),A(k))})}),H.querySelectorAll("[data-star-rid]").forEach(t=>{t.addEventListener("click",async i=>{i.stopPropagation();const n=t.dataset.starRid,a=await Re(n),c=O.find(u=>u.id===n);c&&(c.starred=a.starred),X()})})}function vt(e){var i;const s={};let o={};for(const n of e.split(`
`))if(n.startsWith("data: "))try{const a=JSON.parse(n.slice(6));if(a.type==="message_start"&&(o={...((i=a.message)==null?void 0:i.usage)||{}}),a.type==="message_delta"&&a.usage&&Object.assign(o,a.usage),a.type==="content_block_start"&&(s[a.index]={type:a.content_block.type,name:a.content_block.name,_buf:""}),a.type==="content_block_delta"){const c=s[a.index];if(!c)continue;a.delta.type==="text_delta"&&(c._buf+=a.delta.text||""),a.delta.type==="input_json_delta"&&(c._buf+=a.delta.partial_json||"")}}catch{}return{blocks:Object.entries(s).sort(([n],[a])=>Number(n)-Number(a)).map(([,n])=>{if(n.type==="text")return{type:"text",text:n._buf};if(n.type==="tool_use"){let a={};try{a=JSON.parse(n._buf)}catch{a=n._buf}return{type:"tool_use",name:n.name,input:a}}return{type:n.type,raw:n._buf}}),usage:o}}function ft(e){if(typeof e=="string")return T(d(e));switch(e.type){case"text":return T(d(e.text||""));case"tool_use":return`<div class="cb-tool-use">
        <div class="cb-label tool-use-label">call · ${d(e.name)}</div>
        ${T(d(JSON.stringify(e.input||{},null,2)))}
      </div>`;case"tool_result":{const s=Array.isArray(e.content)?e.content.map(o=>o.text||JSON.stringify(o)).join(`
`):e.content||"";return`<div class="cb-tool-result">
        <div class="cb-label tool-result-label">result · ${d(e.tool_use_id||"")}</div>
        ${T(d(s))}
      </div>`}default:return T(d(JSON.stringify(e,null,2)))}}function $e(e,s,o){const t=typeof e.content=="string"?T(d(e.content)):Array.isArray(e.content)?e.content.map(ft).join(""):T(d(JSON.stringify(e.content,null,2))),i=s!=null?`<span class="msg-num">#${s+1}${o?" · "+o:""}</span>`:"";return`<div class="msg-block"><div class="msg-role ${e.role}">${i}${e.role}</div>${t}</div>`}function yt(e,s=0,o=[]){var Y;const t=(Y=q.find(f=>f.id===e.session_id))==null?void 0:Y.project_name,i=_e(t||"unknown");let n={},a=[],c="",u=null;try{n=JSON.parse(e.request_body),a=n.messages||[],c=n.model||"",u=n.system||null}catch{}let p=[],r={},y=null;if(e.response_body)try{const f=JSON.parse(e.response_body);if(f.raw_sse){const b=vt(f.raw_sse);p=b.blocks,r=b.usage}else y=f}catch{}let v=p.map(f=>f.type==="text"?`<div class="msg-block">
        <div class="msg-role assistant">text</div>
        ${T(d(f.text))}
      </div>`:f.type==="tool_use"?`<div class="msg-block">
        <div class="cb-label tool-use-label">call · ${d(f.name)}</div>
        ${T(d(JSON.stringify(f.input,null,2)))}
      </div>`:`<div class="msg-block">${T(d(JSON.stringify(f,null,2)))}</div>`).join("");!v&&y&&(v=T(d(JSON.stringify(y,null,2)))),v||(v=`<div class="empty-msg" style="padding:12px 0">${e.status==="pending"?"Waiting…":"No content"}</div>`);const S=r.cache_read_input_tokens,$=r.cache_creation_input_tokens,_=S||$?`
    <div class="meta-row">
      <span class="meta-label">Cache read</span>
      <span class="meta-val">${S??0}</span>
    </div>
    <div class="meta-row">
      <span class="meta-label">Cache write</span>
      <span class="meta-val">${$??0}</span>
    </div>`:"";let l={};try{l=JSON.parse(e.request_headers)}catch{}const m=Object.entries(l).map(([f,b])=>`  -H "${f}: ${b}"`).join(` \\
`),g=`curl -X ${e.method} http://localhost:7878${e.path} \\
${m} \\
  -H "x-api-key: $ANTHROPIC_API_KEY" \\
  -d '${e.request_body.replace(/'/g,"'\\''")}'`,h=e.status==="complete"?"status-ok":e.status==="error"?"status-err":e.status==="intercepted"?"status-intercept":e.status==="rejected"?"status-err":"status-pending",L=u?`<div class="msg-block">
    <div class="msg-role system">system</div>
    ${T(d(typeof u=="string"?u:JSON.stringify(u,null,2)))}
  </div>`:"";let B;if(e.status==="intercepted"){const f=(()=>{try{return JSON.stringify(JSON.parse(e.request_body),null,2)}catch{return e.request_body}})();B=`
      <div class="split-header">Intercepted — Edit & Forward</div>
      <div class="split-body">
        <div class="meta-row"><span class="meta-label">Status</span><span class="status-intercept" style="font-weight:500">⏸ intercepted</span></div>
        <div class="intercept-editor">
          <textarea id="interceptBody" class="intercept-textarea" spellcheck="false">${d(f)}</textarea>
          <div class="intercept-actions">
            <button class="btn" id="btnForwardOriginal">Forward Original</button>
            <button class="btn btn-primary" id="btnForwardModified">Forward Modified</button>
            <button class="btn btn-danger" id="btnReject">Reject</button>
          </div>
        </div>
      </div>`}else B=`
      <div class="split-header">Response</div>
      <div class="split-body">
        <div class="meta-row"><span class="meta-label">Status</span><span class="${h}" style="font-weight:500">${e.response_status??"-"} ${e.status}</span></div>
        <div class="meta-row"><span class="meta-label">Output tokens</span><span class="meta-val">${e.output_tokens??"-"}</span></div>
        ${_}
        <div class="meta-row"><span class="meta-label">Duration</span><span class="meta-val">${Ee(e.duration_ms)||"-"}</span></div>
        ${v}
      </div>`;R.innerHTML=`
    <div class="detail-topbar">
      <span class="req-id" title="${e.id}">#${e.id.slice(0,8)}</span>
      ${e.agent_type&&e.agent_type!=="main"?`<span class="agent-badge agent-${e.agent_type}">${e.agent_type}${e.agent_task?": "+d(e.agent_task.slice(0,60))+(e.agent_task.length>60?"…":""):""}</span>`:""}
      <span class="badge ${i}">${d(t||"unknown")}</span>
      <span class="detail-method">${e.method} ${e.path}</span>
      <span class="detail-time">${ee(e.timestamp)}</span>
      <button class="btn btn-sm" id="copyCurl">Copy curl</button>
    </div>
    ${e.routing_category?`<div class="routing-meta">Routing: <span class="routing-badge">${d(e.routing_category)}</span> → ${d(e.routed_to_url||"default upstream")}</div>`:""}

    <div class="split-pane">
      <div class="split-col">
        <div class="split-header">Request</div>
        <div class="split-body">
          <div class="meta-row"><span class="meta-label">Model</span><span class="meta-val">${d(c||"-")}</span></div>
          <div class="meta-row"><span class="meta-label">Input tokens</span><span class="meta-val">${e.input_tokens??"-"}</span></div>
          ${L}
          ${s>0?`<div class="prev-messages-toggle" id="prevMsgToggle">▶ ${s} previous messages hidden</div><div id="prevMsgContainer" class="prev-messages hidden">${a.slice(0,s).map((f,b)=>$e(f,b,ee(o[b]))).join("")}</div><div class="new-messages-divider">── New in this request ──</div>`:""}
          ${(s>0?a.slice(s):a).map((f,b)=>{const C=s>0?s+b:b;return $e(f,C,ee(o[C]))}).join("")||'<div class="empty-msg" style="padding:12px 0">No messages</div>'}
        </div>
      </div>

      <div class="split-divider"></div>

      <div class="split-col">
        ${B}
      </div>
    </div>

    <div class="memo-section">
      <div class="memo-header">Memo</div>
      ${e.memo?`<div class="memo-display" id="memoDisplay">${d(e.memo)}<button class="memo-delete-btn" id="memoDeleteBtn" title="Delete memo">&times;</button></div>`:""}
      <div class="memo-form">
        <input type="text" id="memoInput" class="memo-input" placeholder="Write a memo…" value="${d(e.memo||"")}" />
        <button class="btn btn-sm" id="memoSaveBtn">Save</button>
      </div>
    </div>
  `,document.getElementById("copyCurl").addEventListener("click",()=>{navigator.clipboard.writeText(g).catch(()=>{const f=document.createElement("textarea");f.value=g,document.body.appendChild(f),f.select(),document.execCommand("copy"),document.body.removeChild(f)})});const E=document.getElementById("prevMsgToggle");E&&E.addEventListener("click",()=>{const b=document.getElementById("prevMsgContainer").classList.toggle("hidden");E.textContent=b?`▶ ${s} previous messages hidden`:`▼ ${s} previous messages shown`});const M=document.getElementById("memoInput"),N=document.getElementById("memoSaveBtn"),z=document.getElementById("memoDisplay"),P=async()=>{const f=M.value.trim();if(N.disabled=!0,N.textContent="Saving…",await ue(e.id,f),N.disabled=!1,N.textContent="Save",M.value="",z)z.textContent=f,z.style.display=f?"":"none";else if(f){const C=document.createElement("div");C.className="memo-display",C.id="memoDisplay",C.textContent=f,document.querySelector(".memo-form").before(C)}const b=O.find(C=>C.id===e.id);b&&(b.memo=f),X()};N.addEventListener("click",P),M.addEventListener("keydown",f=>{f.key==="Enter"&&(f.preventDefault(),P())});const Q=document.getElementById("memoDeleteBtn");if(Q&&Q.addEventListener("click",async()=>{await ue(e.id,"");const f=O.find(C=>C.id===e.id);f&&(f.memo=""),X();const b=document.getElementById("memoDisplay");b&&b.remove()}),e.status==="intercepted"){const f=async()=>{await new Promise(b=>setTimeout(b,500)),await J(),await A(e.id)};document.getElementById("btnForwardOriginal").addEventListener("click",async b=>{b.target.disabled=!0,b.target.textContent="Forwarding…",await Me(e.id),await f()}),document.getElementById("btnForwardModified").addEventListener("click",async b=>{b.target.disabled=!0,b.target.textContent="Forwarding…";const C=document.getElementById("interceptBody").value;await Ae(e.id,C),await f()}),document.getElementById("btnReject").addEventListener("click",async b=>{b.target.disabled=!0,b.target.textContent="Rejecting…",await Ne(e.id),await f()})}}async function ce(e){G=e,R.innerHTML='<div class="detail-empty">Loading supervisor analysis…</div>';const[s,o,t]=await Promise.all([ze(e),be(e),Ue(e)]),i=(t.patterns||[]).length===0?'<div class="empty-msg" style="padding:8px 0">No problematic patterns detected</div>':(t.patterns||[]).map(l=>`
        <div class="sv-pattern sv-${l.severity}">
          <span class="sv-severity">${l.severity}</span>
          <span class="sv-type">${d(l.type)}</span>
          ${d(l.description)}
        </div>`).join(""),n=(o.files||[]).sort((l,m)=>(m.access_count||0)-(l.access_count||0)),a=n.filter(l=>(l.access_types||[]).includes("read")),c=n.filter(l=>l.has_full_read),u=a.filter(l=>!l.has_full_read),p=n.filter(l=>(l.access_types||[]).includes("write")||(l.access_types||[]).includes("edit")),r=n.filter(l=>(l.access_types||[]).includes("search")),y=n.filter(l=>!(l.access_types||[]).includes("read")),v=l=>!l||l.length===0?"":l.map(m=>{if(m==="full")return"full";const g={};m.split(",").forEach(B=>{const[E,M]=B.split(":");g[E]=parseInt(M)});const h=(g.offset||0)+1,L=g.limit?h+g.limit-1:"?";return`L${h}–${L}`}).join(", "),S=n.length===0?"":`
    <div class="sv-stats-bar">
      <span class="sv-stat sv-stat-good">${c.length} full read</span>
      <span class="sv-stat sv-stat-warn">${u.length} partial</span>
      <span class="sv-stat">${p.length} written</span>
      <span class="sv-stat">${r.length} searched</span>
      ${y.length>0?`<span class="sv-stat sv-stat-bad">${y.length} not read</span>`:""}
    </div>`,$=n.length===0?'<div class="empty-msg" style="padding:8px 0">No file access recorded</div>':`${S}
      <table class="sv-table">
        <tr><th>File</th><th>Type</th><th>Read Coverage</th><th>Count</th></tr>
        ${n.map(l=>{const m=l.read_ranges||[],g=(l.access_types||[]).includes("read"),h=l.total_lines>0?l.total_lines:null,L=l.lines_read||0;let B="-";if(l.has_full_read)B=`<span class="sv-full-read">full${h?` (${h}/${h})`:""}</span>`;else if(m.length>0){const E=h?` ${Math.round(L/h*100)}%`:"";B=`<span class="sv-partial-read">${v(m)} (${L}/${h||"?"})${E}</span>`}else g&&(B='<span class="sv-partial-read">partial</span>');return`<tr>
          <td class="sv-filepath"><a href="#" class="sv-file-link" data-path="${d(l.file_path)}">${d(l.file_path)}</a></td>
          <td>${(l.access_types||[]).map(E=>`<span class="sv-access-${E}">${E}</span>`).join(" ")}</td>
          <td>${B}</td>
          <td>${l.access_count}</td>
        </tr>`}).join("")}
      </table>`,_=(s.requests||[]).map(l=>{var m;return`
    <tr>
      <td>${d(((m=l.request_id)==null?void 0:m.slice(0,8))||"")}</td>
      <td><span class="sv-agent">${d(l.agent_type||"")}</span></td>
      <td>${d(l.status||"")}</td>
      <td>${l.input_tokens??"-"}/${l.output_tokens??"-"}</td>
      <td>${l.duration_ms?l.duration_ms+"ms":"-"}</td>
    </tr>`}).join("");R.innerHTML=`
    <div class="sv-panel">
      <div class="sv-header">
        <span>Supervisor Analysis</span>
        <span class="sv-session-id" title="${e}">${e.slice(0,12)}…</span>
        <button class="btn btn-sm" id="svClose">Close</button>
      </div>

      <div class="sv-section">
        <h3>Patterns (${t.pattern_count||0})</h3>
        ${i}
      </div>

      <div class="sv-section">
        <h3>File Coverage (${o.file_count||0} files, ${o.total_accesses||0} accesses)</h3>
        ${$}
      </div>

      <div class="sv-section">
        <h3>Request Summary (${s.request_count||0} requests, ${s.total_tokens||0} tokens)</h3>
        ${s.error_count>0?`<div class="sv-pattern sv-error">${s.error_count} errors detected</div>`:""}
        <table class="sv-table">
          <tr><th>ID</th><th>Agent</th><th>Status</th><th>In/Out</th><th>Duration</th></tr>
          ${_||'<tr><td colspan="5">No requests</td></tr>'}
        </table>
      </div>
    </div>
  `,document.getElementById("svClose").addEventListener("click",()=>{G=null,k?A(k):R.innerHTML='<div class="detail-empty">Select a request to inspect</div>'}),document.querySelectorAll(".sv-file-link").forEach(l=>{l.addEventListener("click",m=>{m.preventDefault(),te(e,l.dataset.path)})})}async function ae(){q=await qe(),re()}async function J(){x==="__starred__"?O=await ne(null,{starred:!0}):O=await ne(x,{search:we}),X()}async function A(e){const s=await de(e);let o=0;const t=[];let i=[];try{i=JSON.parse(s.request_body).messages||[]}catch{}const n=i.length;if(s.session_id){const c=[...await ne(s.session_id,{limit:500})].sort((p,r)=>p.timestamp.localeCompare(r.timestamp)),u=c.findIndex(p=>p.id===e);if(u>0)try{const p=await de(c[u-1].id);o=(JSON.parse(p.request_body).messages||[]).length}catch{}for(let p=0;p<n;p++)t[p]=p<o&&u>0?c[u-1].timestamp:s.timestamp}for(let a=0;a<n;a++)t[a]||(t[a]=s.timestamp);yt(s,o,t)}async function gt(){try{V=(await je()).enabled,Le()}catch{}try{W=await Pe(),I=await Z(),K()}catch{}await ae(),await J();let e=null;Ge(s=>{var t,i;clearTimeout(e),e=setTimeout(()=>{ae(),J()},500);let o=null;try{o=(i=(t=JSON.parse(s.data))==null?void 0:t.data)==null?void 0:i.id}catch{}if(s.type==="request_intercepted"&&o){k=o,A(k);return}k&&o===k&&A(k),G&&Ze(G)})}gt();
