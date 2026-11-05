/* Cart transition only. No order/payment, address or account controls. */
(function(root){
  'use strict';
  const text=el=>(el.innerText??el.textContent??'').trim();
  const normalize=value=>value.normalize('NFKC').replace(/\s+/g,'');
  const checkboxSelector='input[type="checkbox"],[role="checkbox"],.stardust-checkbox';
  function loading(message){const error=new Error(message);error.pending=true;return error;}
  function visible(el){
    if(el.closest('[hidden],[aria-hidden="true"]'))return false;
    for(let p=el;p;p=p.parentElement){const s=p.ownerDocument.defaultView.getComputedStyle(p);if(s.display==='none'||s.visibility==='hidden')return false;}
    return true;
  }
  function canonical(href){
    try{
      const u=new URL(href,'https://shopee.tw');
      const m=u.pathname.match(/(?:\/product\/|(?:-i\.))(\d+)[/.](\d+)\/?$/);
      return u.origin==='https://shopee.tw'&&m?`https://shopee.tw/product/${m[1]}/${m[2]}`:null;
    }catch{return null;}
  }
  function widgets(container){
    const all=[...container.querySelectorAll(checkboxSelector)].filter(visible);
    // A native input nested inside a rendered checkbox is one control.
    return all.filter(el=>!all.some(other=>other!==el&&other.contains(el)));
  }
  function checked(el,aggregate=false){
    const native=el.matches('input[type="checkbox"]')?el:el.querySelector('input[type="checkbox"]');
    const aria=el.getAttribute('aria-checked');
    if(aria==='mixed'||el.classList.contains('stardust-checkbox--indeterminate')||native?.indeterminate){
      if(aggregate)return null;
      throw loading('購物車商品勾選狀態尚未同步，請稍候再試。');
    }
    if(aria!==null&&!['true','false'].includes(aria))throw new Error('購物車勾選狀態不明確，停止操作。');
    if(el.classList.contains('stardust-checkbox')){
      // The rendered framework control owns its state. Its backing input can
      // remain unchecked even while the visible checkbox is selected.
      const selected=el.classList.contains('stardust-checkbox--checked');
      if(aria!==null&&(aria==='true')!==selected)throw loading('購物車顯示與無障礙勾選狀態尚未同步，請稍候再試。');
      return selected;
    }
    if(aria!==null)return aria==='true';
    if(native)return native.checked;
    throw new Error('無法確認購物車勾選狀態，停止操作。');
  }
  function quantities(container){
    return [...container.querySelectorAll('input[role="spinbutton"],input[type="number"],input[type="text"],input:not([type])')]
      .filter(el=>visible(el)&&/^\d+$/.test(el.value));
  }
  function money(value){
    const m=value.match(/^(?:NT\s*\$|\$)\s*((?:\d{1,3}(?:,\d{3})+|\d+))(?:\.(\d{1,2}))?$/);
    if(!m)return null;
    const amount=Number(m[1].replaceAll(',',''))*100+Number((m[2]||'').padEnd(2,'0'));
    return Number.isSafeInteger(amount)?amount:null;
  }
  function struck(el,row){
    for(let p=el;p;p=p.parentElement){
      const style=p.ownerDocument.defaultView.getComputedStyle(p);
      if(p.matches('del,s,strike')||`${style.textDecorationLine} ${style.textDecoration}`.includes('line-through'))return true;
      if(p===row)break;
    }
    return false;
  }
  const displayMoney=value=>Number.isSafeInteger(value)?`NT$${(value/100).toFixed(2)}`:'無法讀取';
  function renderedText(el,scope){
    if(!visible(el)||struck(el,scope))return '';
    return [...el.childNodes].map(child=>child.nodeType===3?child.textContent:child.nodeType===1?renderedText(child,scope):'').join(' ');
  }
  function footerTotal(button,doc){
    for(let scope=button.parentElement;scope&&scope!==doc.body;scope=scope.parentElement){
      // Never scan product rows or a whole-cart ancestor for a budget amount.
      if([...scope.querySelectorAll('a[href]')].some(a=>visible(a)&&canonical(a.getAttribute('href'))))break;
      const value=renderedText(scope,scope);
      if(!value.includes('總金額'))continue;
      const matches=[...value.matchAll(/總金額\s*[（(]\s*(\d+)\s*件商品\s*[）)]\s*[:：]?\s*((?:NT\s*\$|\$)\s*[\d,]+(?:\.\d+)?)/g)];
      if(!matches.length)throw loading('購物車底部總金額尚未顯示或無法讀取。');
      if(matches.length!==1)throw loading('購物車底部有多個總金額，尚未確認可用金額。');
      const match=matches[0],total=money(match[2]),tail=value.slice(match.index+match[0].length);
      if(total===null||/^\s*(?:-|~|～|–|至|到)\s*(?:NT\s*\$|\$)/.test(tail))throw loading('購物車底部總金額格式不明確。');
      return {total_minor:total,item_count:Number(match[1])};
    }
    throw loading('尚未找到含有「去買單」的獨立購物車總金額區塊。');
  }
  function itemRows(doc){
    const rows=new Map();
    for(const link of [...doc.querySelectorAll('a[href]')].filter(a=>visible(a)&&canonical(a.getAttribute('href')))){
      const id=canonical(link.getAttribute('href'));
      for(let row=link.parentElement;row&&row!==doc.body;row=row.parentElement){
        const ids=new Set([...row.querySelectorAll('a[href]')].filter(visible).map(a=>canonical(a.getAttribute('href'))).filter(Boolean));
        if(ids.size===1&&ids.has(id)&&widgets(row).length===1&&quantities(row).length===1){rows.set(row,id);break;}
      }
    }
    return rows;
  }
  function aggregateRows(widget,rows,doc){
    // A shop/cart header is outside every item row, precedes the rows it
    // controls, and has no product/quantity controls in its own header branch.
    let branch=widget;
    for(let scope=widget.parentElement;scope&&scope!==doc.body;branch=scope,scope=scope.parentElement){
      const covered=rows.filter(row=>scope.contains(row)&&!branch.contains(row));
      if(!covered.length)continue;
      if(quantities(branch).length||[...branch.querySelectorAll('a[href]')].some(a=>visible(a)&&canonical(a.getAttribute('href'))))return null;
      const following=doc.defaultView.Node.DOCUMENT_POSITION_FOLLOWING;
      if(!covered.every(row=>widget.compareDocumentPosition(row)&following))return null;
      const links=[...scope.querySelectorAll('a[href]')].filter(a=>visible(a)&&canonical(a.getAttribute('href')));
      if(!links.every(a=>covered.some(row=>row.contains(a))))return null;
      return covered;
    }
    return null;
  }
  function prepare(doc,command){
    if(doc.location.origin!=='https://shopee.tw'||!/^\/cart\/?$/.test(doc.location.pathname))throw new Error('目前不是蝦皮購物車，停止操作。');
    if(Date.now()>=command.expires_unix_ms)throw new Error('購物車操作逾時。');
    if([...doc.querySelectorAll('[role="dialog"],[aria-modal="true"]')].some(visible))throw new Error('購物車有待處理提示，請先在蝦皮查看。');
    if(!Number.isSafeInteger(command.quantity)||command.quantity<1||!Number.isSafeInteger(command.max_price_minor)||command.quantity>100||command.max_price_minor<=0)throw new Error('購物車任務或預算上限無效。');
    const allRows=itemRows(doc),rows=[...allRows].filter(([,id])=>id===command.url).map(([row])=>row);
    if(!rows.length)throw loading('購物車尚未找到可核對的目標商品列，請確認後重新讀取。');
    if(rows.length!==1)throw new Error('購物車目標商品有多個規格列，停止操作；請確認後重新讀取。');
    const row=rows[0],checkbox=widgets(row)[0],knownRows=[...allRows.keys()];
    if(!checked(checkbox))throw loading('目標商品尚未勾選或顯示仍在更新，請確認購物車。');
    for(const widget of widgets(doc)){
      if(widget===checkbox)continue;
      const owner=knownRows.find(row=>row.contains(widget));
      const aggregate=owner?null:aggregateRows(widget,knownRows,doc);
      if(aggregate){
        if(checked(widget,true)===true&&!aggregate.every(row=>checked(widgets(row)[0])))throw loading('購物車全選與商品勾選尚未同步，其他勾選狀態不明確。');
        continue;
      }
      if(checked(widget))throw new Error('購物車還有其他勾選商品或無法辨識的勾選項目，停止操作；只保留目標商品後再試。');
    }
    if(Number(quantities(row)[0].value)!==command.quantity)throw new Error('購物車數量與任務不同，停止操作。');
    const labels=command.selections.map(s=>normalize(s.option));
    if(labels.length){
      const expected=labels.join(',');
      const matches=[...row.querySelectorAll('*')].some(el=>normalize(text(el)).replace(/^規格[:：]?/,'')===expected);
      if(!matches)throw new Error('購物車規格與任務不符或無法核對，停止操作。');
    }
    const buttons=[...doc.querySelectorAll('button,[role="button"]')].filter(b=>visible(b)&&text(b)==='去買單');
    if(!buttons.length)throw loading('購物車「去買單」入口尚未顯示。');
    if(buttons.length!==1||buttons[0].disabled||buttons[0].hasAttribute('disabled')||buttons[0].getAttribute('aria-disabled')==='true')throw new Error('購物車「去買單」入口不可用，停止操作。');
    const cart={...footerTotal(buttons[0],doc),quantity:command.quantity};
    if(cart.item_count!==1&&cart.item_count!==command.quantity)throw new Error('購物車總金額的商品件數與目標數量不符，停止操作。');
    if(cart.total_minor>command.max_price_minor)throw new Error(`購物車總金額 ${displayMoney(cart.total_minor)} 超過預算上限 ${displayMoney(command.max_price_minor)}，尚未點擊去買單。`);
    return {button:buttons[0],cart,fingerprint:JSON.stringify([command.url,labels,cart])};
  }
  async function dispatch(doc,command,options={}){
    const now=options.now||Date.now,sleep=options.sleep||(ms=>new Promise(r=>setTimeout(r,ms)));
    const started=now();let previous='',since=started,plan,lastPending='';
    while(now()-started<=12000&&now()<command.expires_unix_ms){
      try{plan=prepare(doc,command);lastPending='';}
      catch(error){
        if(!error.pending)throw error;
        lastPending=error.message;previous='';since=now();await sleep(100);continue;
      }
      if(plan.fingerprint!==previous){previous=plan.fingerprint;since=now();}
      if(now()-since>=1200){
        // Recheck immediately before the one permitted cart transition.
        const latest=prepare(doc,command);
        if(latest.fingerprint!==previous)throw new Error('購物車在操作前改變，停止操作。');
        const click=()=>{
          const current=prepare(doc,command);
          if(current.fingerprint!==previous)throw new Error('購物車總金額或商品在點擊前改變，停止操作。');
          current.button.click();
        };
        if(options.scheduleClick)options.scheduleClick(click);else click();
        return {dispatched:true,cart:latest.cart};
      }
      await sleep(100);
    }
    throw new Error(lastPending||'購物車資料穩定等待逾時，尚未點擊去買單。');
  }
  root.ShoppeeCart={prepare,dispatch};
  if(typeof chrome==='undefined')return;
  const dispatched=new Set();
  chrome.runtime.onMessage.addListener((message,sender,respond)=>{
    if(sender.id!==chrome.runtime.id||message.type!=='CART_COMMAND')return false;
    const id=message.command?.id;
    if(!Number.isSafeInteger(id)||dispatched.has(id)){respond({error:'購物車指令重複或無效，不會重送。'});return false;}
    dispatched.add(id);
    dispatch(document,message.command,{scheduleClick:fn=>setTimeout(()=>{try{fn();}catch{/* Recheck failed: no click, no retry. */}},0)}).then(outcome=>respond({outcome}),error=>respond({error:error.message}));
    return true;
  });
})(globalThis);
