'use strict';
const $ = id => document.getElementById(id);
const incomingToken = location.hash.slice(1);
let token = /^[a-f0-9]{64}$/.test(incomingToken) ? incomingToken : '';
try {
  if (token) sessionStorage.setItem('shoppee-session', token);
  else token = sessionStorage.getItem('shoppee-session') || '';
} catch { /* A valid URL token still works when browser storage is blocked. */ }
history.replaceState(null, '', location.pathname);
let mode = 'browser', snapshot = null, online = false, polling = false, parsedUrl = '', lastLog = '', submitting = false;
const phases = {browser_inspecting:'讀取真實規格',browser_checkout:'選規格並前往結帳',catalog_ready:'請選擇規格',checkout_ready:'已前往結帳',idle:'尚未開始',scheduled:'等待啟動',monitoring:'監測中',preparing:'取得模擬報價',opening:'開啟商品頁',opened:'已開啟官方頁面',awaiting_confirmation:'等待確認',simulation_confirmed:'模擬完成',cancelled:'已停止',failed:'執行失敗'};
const active = () => snapshot && ['scheduled','monitoring','preparing','opening','browser_inspecting','browser_checkout'].includes(snapshot.job.phase);
async function api(path, data) {
  const opts = {headers:{'x-session-token':token}};
  if (data !== undefined) { opts.method='POST'; opts.headers['Content-Type']='application/json'; opts.body=JSON.stringify(data); }
  const response = await fetch(path, opts);
  let body;
  try { body = await response.json(); } catch { throw new Error('伺服器回應異常，請確認程式仍在執行。'); }
  if (!response.ok) throw new Error(body.error || `操作失敗（${response.status}）`);
  return body;
}
function error(message) { $('form-error').textContent=message; $('form-error').hidden=!message; }
function setMode(value) {
  mode=value;
  $('mode-browser').classList.toggle('selected',value==='browser'); $('mode-browser').setAttribute('aria-pressed',String(value==='browser')); $('browser-settings').hidden=value!=='browser';
  $('mode-shopee').classList.toggle('selected',value==='shopee'); $('mode-mock').classList.toggle('selected',value==='mock');
  $('mode-shopee').setAttribute('aria-pressed',String(value==='shopee')); $('mode-mock').setAttribute('aria-pressed',String(value==='mock'));
  $('mock-settings').hidden=value!=='mock';
  for(const input of $('mock-settings').querySelectorAll('input'))input.disabled=value!=='mock';
  for(const input of $('browser-settings').querySelectorAll('input,select'))input.disabled=value!=='browser';
  $('start-label').textContent=value==='mock'?'開始本機模擬':value==='browser'?'選規格並自動前往結帳':'建立開啟任務';
  $('mode-description').textContent=value==='browser'?'預算限制商品小計（單價 × 數量），不含運費；含運總額請在蝦皮結帳確認。':value==='mock'?'預算會用於檢查模擬報價；所有監測請求都只送往本機。':'預算僅用於任務備註；正式價格與總額請在蝦皮確認。';
  $('form-footnote').textContent=value==='browser'?'會操作登入中的蝦皮商品分頁；最終下單與付款由你確認。':value==='mock'?'模擬商品不代表真實商品，不會建立訂單或付款。':'不會自動下單或付款，不需要輸入蝦皮帳密。';
  error('');
}
async function parseUrl() {
  error('');
  try {
    const input=$('product-url').value.trim();
    const product=await api('/api/parse',{url:input});
    if ($('product-url').value.trim() !== input) return null;
    $('product-ids').textContent=`商店 ${product.shop_id} · 商品 ${product.item_id}`;
    $('product-link').href=product.canonical_url;
    $('product-result').hidden=false; parsedUrl=input;
    return product;
  } catch (e) { $('product-result').hidden=true; parsedUrl=''; error(e.message); return null; }
}
$('mode-browser').addEventListener('click',()=>setMode('browser'));
$('mode-shopee').addEventListener('click',()=>setMode('shopee'));
$('mode-mock').addEventListener('click',()=>setMode('mock'));
$('parse-button').addEventListener('click',parseUrl);
$('product-url').addEventListener('input',()=>{parsedUrl='';$('product-result').hidden=true;});
$('task-form').addEventListener('submit',async event=>{
  event.preventDefault(); if (submitting || active()) return; submitting=true; $('start-button').disabled=true; error('');
  try {
    if (parsedUrl!==$('product-url').value.trim() && !(await parseUrl())) return;
    const price=Math.round(Number($('max-price').value)*100);
    if (!Number.isSafeInteger(price) || price<=0) throw new Error('請輸入有效的預算。');
    const date=$('schedule').value;
    const at=date?new Date(date).getTime():null;
    if (date && !Number.isFinite(at)) throw new Error('啟動時間格式不正確。');
    if(mode==='browser'&&!bridgeConnected)throw new Error('請先安裝擴充功能並連接瀏覽器。');
    const selections=[...document.querySelectorAll('#variant-fields select')].map(select=>({group:select.dataset.group,option:select.value}));
    if(mode==='browser'&&selections.some(s=>!s.option))throw new Error('請選擇所有商品規格。');
    await api('/api/start',{quantity:Number($('quantity').value),selections,url:$('product-url').value.trim(),mode,max_price_minor:price,at_unix_ms:at,interval_ms:Number($('interval').value),budget_secs:Number($('budget').value),mock_release_after_ms:Math.round(Number($('release-delay').value)*1000)});
    await poll();
  } catch(e) { error(e.message); }
  finally { submitting=false; $('start-button').disabled=!online || active(); }
});
$('stop-button').addEventListener('click',async()=>{
  if(!snapshot)return;
  try { await api('/api/cancel',{id:snapshot.job.id}); await poll(); } catch(e){error(e.message);}
});
$('confirm-button').addEventListener('click',async()=>{
  if(!snapshot)return;
  $('confirm-button').disabled=true;
  try { await api('/api/confirm',{id:snapshot.job.id}); await poll(); } catch(e){error(e.message);}
  finally {renderClock();}
});
async function exportReport() {
  try {
    const data=await api('/api/export');
    const blob=new Blob([JSON.stringify(data,null,2)],{type:'application/json'});
    const url=URL.createObjectURL(blob),a=document.createElement('a');a.href=url;a.download=`shoppee-job-${data.job.id}.json`;
    document.body.appendChild(a);a.click();a.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);
  } catch(e){error(e.message);}
}
$('export-button').addEventListener('click',exportReport);$('export-side').addEventListener('click',exportReport);
function renderClock() {
  if(!snapshot)return;
  const j=snapshot.job, running=active();
  let seconds=j.target_unix_ms && j.phase==='scheduled'?Math.max(0,Math.ceil((j.target_unix_ms-Date.now())/1000)):0;
  if(j.phase==='idle') $('countdown').innerHTML='--<span>:</span>--<span>:</span>--';
  else {const h=Math.floor(seconds/3600),m=Math.floor(seconds/60)%60,s=seconds%60;$('countdown').textContent=[h,m,s].map(v=>String(v).padStart(2,'0')).join(':');}
  $('countdown-label').textContent=j.phase==='idle'?'準備好你的下一個任務':j.phase==='scheduled'?'距離任務啟動':phases[j.phase]||j.phase;
  const target=j.target_unix_ms?new Date(j.target_unix_ms).toLocaleString('zh-TW',{hour12:false}):'立即開始';
  $('countdown-caption').textContent=j.phase==='idle'?'完成左側設定，即可開始':running?`啟動設定：${target}`:j.phase==='opened'?'請在蝦皮官方頁面完成購買':j.phase==='simulation_confirmed'?'本次為模擬確認，未建立真實交易':'詳細狀態請查看下方執行紀錄';
  if(j.quote) {
    const left=Math.max(0,Math.ceil((j.quote.expires_unix_ms-Date.now())/1000));
    $('quote-expiry').textContent=j.phase==='simulation_confirmed'?'已確認模擬結果，沒有付款請求。':left?`模擬報價剩餘 ${left} 秒有效`:'模擬報價已過期，請重新執行。';
    $('confirm-button').disabled=j.phase!=='awaiting_confirmation'||left<=0||!online;
  }
}
function render(data) {
  snapshot=data; const j=data.job,run=active(),done=['opened','awaiting_confirmation','simulation_confirmed','checkout_ready'].includes(j.phase);
  $('version').textContent=`v${data.version}`;
  $('status-label').textContent=phases[j.phase]||j.phase;
  $('status-pill').className='status-pill'+(run?' running':done?' success':j.phase==='failed'?' failed':'');
  $('settings').disabled=!!run;$('start-button').disabled=!online||!!run||submitting;$('stop-button').disabled=!online||!run;
  if(j.mode && run) setMode(j.mode);
  $('progress-mid').textContent=j.mode==='mock'?'監測商品':j.mode==='browser'?'選規格與檢查':'等待啟動';$('progress-end').textContent=j.mode==='mock'?'確認模擬報價':j.mode==='browser'?'結帳待確認':'開啟商品頁';
  $('progress-1').className='progress-step'+(j.phase!=='idle'?' done':'');
  $('progress-2').className='progress-step'+(run?' current':done?' done':'');
  $('progress-3').className='progress-step'+(done?' done':'');
  const service=data.metrics.successful_ms.service;
  for(const q of ['p50','p95','p99']) $('metric-'+q).textContent=service?service[q].toFixed(2):'—';
  $('request-count').textContent=String(j.request_count);
  $('schedule-lateness').textContent=j.schedule_lateness_ms===null?'—':`${j.schedule_lateness_ms.toFixed(2)} ms`;
  $('result-box').hidden=!j.quote;
  if(j.quote)$('quote-price').textContent=`NT$${(j.quote.product.price_minor/100).toFixed(2)}`;
  const logKey=JSON.stringify(j.logs);
  if(logKey!==lastLog){
    lastLog=logKey;$('log-count').textContent=`${j.logs.length} 筆`;
    if(j.logs.length){$('log-list').replaceChildren();for(const entry of [...j.logs].reverse()){
      const row=document.createElement('div');row.className='log-row';const time=document.createElement('time');time.textContent=new Date(entry.at_unix_ms).toLocaleTimeString('zh-TW',{hour12:false});const dot=document.createElement('span');dot.className='log-dot';const message=document.createElement('span');message.textContent=entry.message;row.append(time,dot,message);$('log-list').appendChild(row);
    }}
  }
  renderCatalog(j.browser_catalog);
  renderClock();
}
async function poll() {
  if(polling)return; polling=true;
  try {const data=await api('/api/state'); online=true;$('connection-text').textContent='已連線 · 本機';$('connection-dot').classList.remove('offline');render(data);}
  catch(e){online=false;$('connection-text').textContent='連線中斷';$('connection-dot').classList.add('offline');$('start-button').disabled=true;$('stop-button').disabled=true;$('confirm-button').disabled=true;error(e.message);}
  finally{polling=false;}
}

const bridgeNonce=[...crypto.getRandomValues(new Uint8Array(16))].map(b=>b.toString(16).padStart(2,'0')).join('');document.documentElement.dataset.shoppeeBridgeNonce=bridgeNonce;
let bridgeConnected=false, commandLoopRunning=false, catalogKey='', pendingResults=new Map();
$('connect-browser').addEventListener('click',()=>{
  $('browser-status').textContent='等待擴充功能回應…';
  setTimeout(()=>{if(!bridgeConnected)$('browser-status').textContent='尚未偵測到擴充功能；請安裝後重新整理操作頁。';},1500);
});
$('read-browser').addEventListener('click',async()=>{
  error('');try{
    if(!bridgeConnected)throw new Error('請先安裝擴充功能並按「連接瀏覽器」。');
    await api('/api/browser/inspect',{url:$('product-url').value.trim()});await poll();
  }catch(e){error(e.message);}
});
function renderCatalog(catalog){
  const key=JSON.stringify(catalog);
  if(key===catalogKey)return;catalogKey=key;
  $('variant-fields').replaceChildren();
  if(!catalog){$('catalog-info').textContent='讀取後才會顯示真實規格，不會根據網址猜測。';return;}
  $('catalog-info').textContent=catalog.title+(catalog.shipping_blocked?'；目前配送不可用，請先在蝦皮處理後重新讀取。':'；資料為最近一次讀取結果，執行前會再檢查。');
  for(const group of catalog.groups){
    const label=document.createElement('label'),select=document.createElement('select');
    select.id='variant-'+$('variant-fields').childElementCount;select.dataset.group=group.name;label.htmlFor=select.id;label.textContent=group.name;
    const placeholder=document.createElement('option');placeholder.value='';placeholder.textContent='請選擇規格';select.appendChild(placeholder);
    for(const item of group.options){const option=document.createElement('option');option.value=item.label;option.textContent=item.label+(item.disabled?'（目前停用）':'');option.disabled=item.disabled;select.appendChild(option);}
    $('variant-fields').append(label,select);
  }
}
window.addEventListener('message',event=>{
  const data=event.data;
  if(event.source!==window||event.origin!==location.origin||data?.channel!=='SHOPPEE_COMPANION_V1'||data.nonce!==bridgeNonce)return;
  if(data.type==='READY'&&data.connected){bridgeConnected=true;$('browser-status').textContent='已連接擴充功能';browserCommandLoop();}
  if(data.type==='RESULT'){const resolve=pendingResults.get(data.command_id);if(resolve){pendingResults.delete(data.command_id);resolve(data);}}
  if(data.type==='ERROR')error(data.error||'擴充功能錯誤。');
});
async function browserCommandLoop(){
  if(commandLoopRunning)return;commandLoopRunning=true;
  try{while(bridgeConnected){
    const {command}=await api('/api/browser/next',{});if(!command)continue;
    const result=await new Promise(resolve=>{
      const timer=setTimeout(()=>{pendingResults.delete(command.id);resolve({error:'擴充功能操作逾時，請檢查商品分頁。'});},Math.max(1,command.expires_unix_ms-Date.now()));
      pendingResults.set(command.id,data=>{clearTimeout(timer);resolve(data);});
      window.postMessage({channel:'SHOPPEE_UI_V1',type:'COMMAND',nonce:bridgeNonce,command},location.origin);
    });
    try{await api('/api/browser/report',{command_id:command.id,outcome:result.outcome||null,error:result.error||null});}catch(e){error(e.message);}
    await poll();
  }}catch(e){bridgeConnected=false;$('browser-status').textContent='連線中斷，請重新連接';error(e.message);}finally{commandLoopRunning=false;}
}

setMode('browser');
if(!token){error('此頁缺少工作階段。請重新啟動程式，使用程式開啟的完整本機網址。');$('connection-text').textContent='缺少工作階段';$('connection-dot').classList.add('offline');$('start-button').disabled=true;}
else{poll();setInterval(poll,1000);}
setInterval(renderClock,200);
