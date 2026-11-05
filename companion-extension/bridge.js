'use strict';
(function(){
  if(window.top!==window||location.hostname!=='127.0.0.1')return;
  let connected=false;
  const send=data=>window.postMessage({channel:'SHOPPEE_COMPANION_V1',nonce:document.documentElement.dataset.shoppeeBridgeNonce,...data},location.origin);
  document.addEventListener('click',event=>{
    if(!event.isTrusted||!event.target.closest('#connect-browser')||!document.querySelector('meta[name="shoppee-app"][content="local-ui"]'))return;
    chrome.runtime.sendMessage({type:'CONNECT'}).then(result=>{connected=!!result.connected;send({type:'READY',connected});},()=>send({type:'ERROR',error:'無法連接擴充功能。'}));
  },true);
  window.addEventListener('message',event=>{
    const data=event.data;
    if(event.source!==window||event.origin!==location.origin||!connected||data?.channel!=='SHOPPEE_UI_V1'||data.nonce!==document.documentElement.dataset.shoppeeBridgeNonce||data.type!=='COMMAND')return;
    chrome.runtime.sendMessage({type:'COMMAND',command:data.command}).then(result=>send({type:'RESULT',command_id:data.command.id,...result}),()=>send({type:'RESULT',command_id:data.command.id,error:'擴充功能連線中斷。'}));
  });
})();
