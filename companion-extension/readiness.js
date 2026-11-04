/* Wait on rendered purchase data, with bounded time and actionable diagnostics. */
(function(root){
  'use strict';
  function reason(c,command,quantity){
    if(c.canonical_url!==command.url)return '商品識別不符，請重新讀取正確的商品。';
    if(command.kind==='inspect')return '';
    if(c.shipping_blocked)return '目前預設地址／門市不適用配送，請在蝦皮修改後重新讀取。';
    if(c.groups.length!==command.selections.length)return '頁面規格群組已變更，請重新讀取並選擇全部規格。';
    for(const s of command.selections){
      const g=c.groups.find(g=>g.name===s.group),o=g?.options.find(o=>o.label===s.option);
      if(!o||o.disabled)return `規格「${s.option}」不存在或已停用，請重新讀取。`;
      if(!o.selected||g.options.filter(o=>o.selected).length!==1)return `規格「${s.option}」尚未在蝦皮選取成功，請檢查商品分頁後重新讀取。`;
    }
    if(!Number.isSafeInteger(c.unit_price_minor)||c.unit_price_minor<=0)return '無法讀取所選規格的明確單價，請確認商品分頁已顯示單一價格後重新讀取。';
    if(quantity!==command.quantity)return '蝦皮數量尚未與任務一致，請檢查數量欄位後重新開始。';
    return '';
  }
  function state(c,quantity){
    // Animated banners, title changes and unrelated page content are not prices.
    return {groups:c.groups,unit_price_minor:c.unit_price_minor,stock:c.stock,shipping_blocked:c.shipping_blocked,quantity};
  }
  async function wait(read,command,readQuantity,options={}){
    const now=options.now||Date.now,sleep=options.sleep||(ms=>new Promise(r=>setTimeout(r,ms)));
    const started=now(),timeout=12000,stableMs=1200;
    let previous=null,since=started,lastReason='',changed=[];
    while(now()-started<=timeout){
      if(now()>=command.expires_unix_ms)throw new Error('操作逾時，請重新開始。');
      const c=read(),quantity=command.kind==='inspect'?null:readQuantity();
      const next=state(c,quantity),key=JSON.stringify(next);
      if(!previous||key!==JSON.stringify(previous)){
        if(previous)changed=Object.keys(next).filter(k=>JSON.stringify(next[k])!==JSON.stringify(previous[k]));
        previous=next;since=now();
      }
      lastReason=reason(c,command,quantity);
      if(!lastReason&&now()-since>=stableMs)return c;
      await sleep(100);
    }
    if(lastReason)throw new Error(lastReason);
    const names={groups:'規格／可選狀態',unit_price_minor:'單價',stock:'庫存',shipping_blocked:'配送狀態',quantity:'數量'};
    throw new Error(`等待商品資料穩定逾時（${changed.map(k=>names[k]).join('、')||'商品資料'}仍在更新），尚未點擊購買；請檢查商品分頁後重新讀取。`);
  }
  root.ShoppeeReadiness={wait};
})(globalThis);
