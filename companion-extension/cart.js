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
  function checked(el){
    const native=el.matches('input[type="checkbox"]')?el:el.querySelector('input[type="checkbox"]');
    const aria=el.getAttribute('aria-checked');
    if(aria!==null&&!['true','false'].includes(aria))throw new Error('購物車勾選狀態不明確，停止操作。');
    if(native){
      if(native.indeterminate||(aria!==null&&(aria==='true')!==native.checked))throw new Error('購物車勾選狀態尚未同步，停止操作。');
      return native.checked;
    }
    if(aria!==null)return aria==='true';
    if(el.classList.contains('stardust-checkbox'))return el.classList.contains('stardust-checkbox--checked');
    throw new Error('無法確認購物車勾選狀態，停止操作。');
  }
  function quantities(container){
    return [...container.querySelectorAll('input[role="spinbutton"],input[type="number"],input[type="text"],input:not([type])')]
      .filter(el=>visible(el)&&/^\d+$/.test(el.value));
  }
  function money(value){
    const m=value.match(/^(?:NT\s*\$|\$)\s*([\d,]+)(?:\.(\d{1,2}))?$/);
    if(!m)return null;
    const amount=Number(m[1].replaceAll(',',''))*100+Number((m[2]||'').padEnd(2,'0'));
    return Number.isSafeInteger(amount)?amount:null;
  }
  function prepare(doc,command,expectedPrice){
    if(doc.location.origin!=='https://shopee.tw'||!/^\/cart\/?$/.test(doc.location.pathname))throw new Error('目前不是蝦皮購物車，停止操作。');
    if(Date.now()>=command.expires_unix_ms)throw new Error('購物車操作逾時。');
    if([...doc.querySelectorAll('[role="dialog"],[aria-modal="true"]')].some(visible))throw new Error('購物車有待處理提示，請先在蝦皮查看。');
    if(!Number.isSafeInteger(command.quantity)||command.quantity<1||!Number.isSafeInteger(command.max_price_minor)||!Number.isSafeInteger(expectedPrice)||expectedPrice<=0)throw new Error('購物車任務或價格無效。');
    const links=[...doc.querySelectorAll('a[href]')].filter(a=>visible(a)&&canonical(a.getAttribute('href'))===command.url);
    const rows=new Set();
    for(const link of links){
      for(let row=link.parentElement;row&&row!==doc.body;row=row.parentElement){
        const ids=new Set([...row.querySelectorAll('a[href]')].map(a=>canonical(a.getAttribute('href'))).filter(Boolean));
        if(ids.size===1&&ids.has(command.url)&&widgets(row).length===1&&quantities(row).length===1){rows.add(row);break;}
      }
    }
    if(!rows.size)throw loading('購物車尚未找到可核對的目標商品列，請確認後重新讀取。');
    if(rows.size!==1)throw new Error('購物車目標商品有多個規格列，停止操作；請確認後重新讀取。');
    const row=[...rows][0],checkbox=widgets(row)[0];
    if(!checked(checkbox))throw new Error('目標商品尚未勾選，停止操作；請確認購物車。');
    const selected=widgets(doc).filter(checked);
    if(selected.length!==1||selected[0]!==checkbox)throw new Error('購物車還有其他勾選項目或勾選狀態不明確，停止操作；只保留目標商品後再試。');
    if(Number(quantities(row)[0].value)!==command.quantity)throw new Error('購物車數量與任務不同，停止操作。');
    const labels=command.selections.map(s=>normalize(s.option));
    if(labels.length){
      const expected=labels.join(',');
      const matches=[...row.querySelectorAll('*')].some(el=>normalize(text(el)).replace(/^規格[:：]?/,'')===expected);
      if(!matches)throw new Error('購物車規格與任務不符或無法核對，停止操作。');
    }
    const prices=(text(row).match(/(?:NT\s*\$|\$)\s*[\d,]+(?:\.\d{1,2})?/g)||[]).map(money);
    if(prices.length<2)throw loading('購物車尚未顯示明確單價與小計，請確認後重新讀取。');
    if(prices.length!==2||prices.some(p=>p===null)||prices[0]!==expectedPrice||prices[1]!==expectedPrice*command.quantity)throw new Error('購物車單價／小計已變更或不明確，停止操作；請重新讀取商品。');
    const totals=[...text(doc.body).matchAll(/總金額\s*[（(]\s*(\d+)\s*件商品\s*[）)]\s*[:：]?\s*((?:NT\s*\$|\$)\s*[\d,]+(?:\.\d{1,2})?)/g)];
    if(!totals.length)throw loading('購物車總金額尚未顯示或無法讀取，請確認後重新讀取。');
    if(totals.length!==1||Number(totals[0][1])!==1)throw new Error('無法確認購物車只有一項商品，停止操作。');
    const total=money(totals[0][2]);
    if(total===null||total<=0||total>prices[1]||prices[1]>command.max_price_minor||total>command.max_price_minor)throw new Error('購物車金額不明確或超過預算，停止操作。');
    const buttons=[...doc.querySelectorAll('button,[role="button"]')].filter(b=>visible(b)&&text(b)==='去買單');
    if(!buttons.length)throw loading('購物車「去買單」入口尚未顯示。');
    if(buttons.length!==1||buttons[0].disabled||buttons[0].hasAttribute('disabled')||buttons[0].getAttribute('aria-disabled')==='true')throw new Error('購物車「去買單」入口不可用，停止操作。');
    return {button:buttons[0],fingerprint:JSON.stringify([command.url,labels,command.quantity,prices,total])};
  }
  async function dispatch(doc,command,expectedPrice,options={}){
    const now=options.now||Date.now,sleep=options.sleep||(ms=>new Promise(r=>setTimeout(r,ms)));
    const started=now();let previous='',since=started,plan,lastPending='';
    while(now()-started<=12000&&now()<command.expires_unix_ms){
      try{plan=prepare(doc,command,expectedPrice);lastPending='';}
      catch(error){
        if(!error.pending)throw error;
        lastPending=error.message;previous='';since=now();await sleep(100);continue;
      }
      if(plan.fingerprint!==previous){previous=plan.fingerprint;since=now();}
      if(now()-since>=1200){
        // Recheck immediately before the one permitted cart transition.
        const latest=prepare(doc,command,expectedPrice);
        if(latest.fingerprint!==previous)throw new Error('購物車在操作前改變，停止操作。');
        latest.button.click();return {dispatched:true};
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
    dispatch(document,message.command,message.expected_price_minor).then(outcome=>respond({outcome}),error=>respond({error:error.message}));
    return true;
  });
})(globalThis);
