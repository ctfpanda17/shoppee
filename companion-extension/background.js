'use strict';
const pending=new Map();
function local(sender){try{const u=new URL(sender.tab.url);return sender.frameId===0&&u.protocol==='http:'&&u.hostname==='127.0.0.1';}catch{return false;}}
function valid(command){
  if(!command||!Number.isSafeInteger(command.id)||!['inspect','checkout'].includes(command.kind)||!Number.isFinite(command.expires_unix_ms)||command.expires_unix_ms<=Date.now())throw new Error('操作無效或逾時。');
  const u=new URL(command.url);if(u.origin!=='https://shopee.tw'||!/^\/product\/[1-9]\d*\/[1-9]\d*$/.test(u.pathname)||u.search||u.hash)throw new Error('只接受已解析的蝦皮商品網址。');
}
async function execute(sender,command){
  valid(command);
  const key=`ui-${sender.tab.id}`;
  const state=(await chrome.storage.session.get(key))[key];
  if(!state)throw new Error('尚未連接瀏覽器。');
  let tab;
  if(state.productTab){try{tab=await chrome.tabs.get(state.productTab);}catch{}}
  if(command.kind==='inspect'){
    if(!tab){tab=await chrome.tabs.create({url:command.url,active:true});state.productTab=tab.id;await chrome.storage.session.set({[key]:state});}
    else {await chrome.tabs.update(tab.id,{url:command.url,active:true});}
  }else if(!tab)throw new Error('商品分頁已關閉，請重新讀取規格。');
  // Wait for normal page load; no refresh, fingerprint or private API operations.
  let result;
  for(let i=0;i<60;i++){
    if(Date.now()>command.expires_unix_ms)throw new Error('操作逾時。');
    const current=await chrome.tabs.get(tab.id);
    if(current.status==='complete'){
      const u=new URL(current.url);
      if(u.pathname.startsWith('/buyer/')||u.pathname.startsWith('/verify/'))throw new Error('請在商品分頁完成登入或網站驗證，再重新讀取規格。');
      try{result=await chrome.tabs.sendMessage(tab.id,{type:'PRODUCT_COMMAND',command});break;}catch(e){if(command.kind==='checkout')throw new Error('無法確認購買入口是否派發，已停止且不會重送；請檢查商品分頁。');if(i>5)throw new Error('擴充功能尚未載入商品頁，請重新開啟商品後再試。');}
    }
    await new Promise(resolve=>setTimeout(resolve,200));
  }
  if(!result)throw new Error('商品頁載入逾時。');
  if(result.error)throw new Error(result.error);
  if(command.kind==='checkout'){
    if(!result.outcome?.dispatched)throw new Error('購買入口未派發。');
    for(let i=0;i<75;i++){
      const current=await chrome.tabs.get(tab.id);const path=new URL(current.url).pathname;
      if(path==='/checkout'||path.startsWith('/checkout/')){
        delete result.outcome.dispatched;result.outcome.checkout_opened=true;return result.outcome;
      }
      if(path.startsWith('/verify/')||path.startsWith('/buyer/'))throw new Error('網站要求驗證或重新登入，已停止操作。');
      await new Promise(resolve=>setTimeout(resolve,200));
    }
    throw new Error('沒有確認進入結帳頁，請檢查商品分頁；程式不會重送購買。');
  }
  return result.outcome;
}
chrome.runtime.onMessage.addListener((message,sender,respond)=>{
  if(!local(sender))return false;
  if(message.type==='CONNECT'){
    chrome.storage.session.set({[`ui-${sender.tab.id}`]:{productTab:null}}).then(()=>respond({connected:true}));return true;
  }
  if(message.type!=='COMMAND')return false;
  const key=`${sender.tab.id}:${message.command?.id}`;
  // Same UI command is never dispatched twice within this worker lifetime.
  if(!pending.has(key)){pending.set(key,execute(sender,message.command).then(outcome=>({outcome}),error=>({error:error.message})));if(pending.size>128)pending.delete(pending.keys().next().value);}
  pending.get(key).then(respond);return true;
});
chrome.tabs.onRemoved.addListener(tabId=>{chrome.storage.session.remove(`ui-${tabId}`);});
