'use strict';
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
function current(){return ShoppeeDom.page(document,location.href);}
async function settled(command){
  let previous='',stable=0,result;
  for(let i=0;i<50;i++){
    if(Date.now()>command.expires_unix_ms)throw new Error('操作逾時，請重新開始。');
    result=current();const key=JSON.stringify(result);
    stable=key===previous?stable+1:0;previous=key;
    // Do not act on transient loading prices. Require stable DOM and all choices.
    const chosen=command.kind==='inspect'||command.selections.every(s=>result.groups.some(g=>g.name===s.group&&g.options.some(o=>o.label===s.option&&o.selected)));
    if(stable>=12&&chosen&&(command.kind==='inspect'||result.unit_price_minor!==null))return result;
    await delay(100);
  }
  throw new Error('商品頁仍在變動，停止操作；請確認價格與規格後重新讀取。');
}
chrome.runtime.onMessage.addListener((message,sender,respond)=>{
  if(sender.id!==chrome.runtime.id||message.type!=='PRODUCT_COMMAND')return false;
  const command=message.command;
  (async()=>{
    if(current().canonical_url!==command.url)throw new Error('商品識別不符。');
    if(command.kind==='inspect')return {catalog:await settled(command),checkout_opened:false,payment_sent:false};
    if(command.kind!=='checkout')throw new Error('不支援此操作。');
    for(const selection of command.selections){const button=ShoppeeDom.choiceButton(document,selection);if(!button.classList.contains('selection-box-selected')){button.click();await delay(200);}}
    const inputs=[...document.querySelectorAll('input[role="spinbutton"]')];
    if(inputs.length!==1)throw new Error('數量欄位不明確，停止操作。');
    if(Number(inputs[0].value)!==command.quantity){
      const setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;
      setter.call(inputs[0],String(command.quantity));inputs[0].dispatchEvent(new Event('input',{bubbles:true}));inputs[0].dispatchEvent(new Event('change',{bubbles:true}));inputs[0].blur();
    }
    const catalog=await settled(command);
    const button=ShoppeeDom.validate(catalog,command,document);
    if(Date.now()>command.expires_unix_ms)throw new Error('操作逾時，停止操作。');
    // Return a receipt first; navigation can destroy this content-script context.
    setTimeout(()=>button.click(),0);
    return {catalog,checkout_opened:false,payment_sent:false,dispatched:true};
  })().then(outcome=>respond({outcome}),error=>respond({error:error.message}));
  return true;
});
