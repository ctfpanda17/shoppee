const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const {JSDOM}=require('jsdom');
const url='https://shopee.tw/product/116309859/41078532632';
const sku='Sepang方向盤組(含T118盤面)';
const cmd=()=>({id:12,kind:'checkout',url,selections:[{group:'規格',option:sku}],quantity:1,max_price_minor:300000,expires_unix_ms:Date.now()+60000});
function cart(){
  // Synthetic fixture matching the visible layout, not a captured live DOM.
  const d=new JSDOM(`<main>
    <input id="all" type="checkbox" aria-label="全選">
    <section id="target">
      <div role="checkbox" aria-checked="true" id="selected"></div>
      <a href="/wheel-i.116309859.41078532632"><img></a>
      <a href="/product/116309859/41078532632">Wheel fixture</a>
      <div id="sku">規格：${sku}</div><div id="unit">$2,636</div>
      <input id="qty" type="text" value="1"><div id="line">$2,636</div>
    </section>
    <section id="other"><input type="checkbox" id="other-check">
      <a href="/product/1/2">Unrelated fixture</a><input type="text" value="1"><span>$100</span>
    </section>
    <footer><div id="total">總金額 (1 件商品): $2,636</div><button id="go">去買單</button><button id="final">下單</button></footer>
  </main>`,{url:'https://shopee.tw/cart',runScripts:'outside-only'});
  d.window.eval(fs.readFileSync('companion-extension/cart.js','utf8'));
  return d;
}
test('cart validates exact product/SKU/quantity/total and exposes only Go Checkout',()=>{
  const d=cart(),doc=d.window.document;let clicked=0;doc.getElementById('go').onclick=()=>clicked++;
  assert.equal(d.window.ShoppeeCart.prepare(doc,cmd()).button.id,'go');assert.equal(clicked,0);
});
test('reported three raw 2690 amounts do not block a 2636 cart total within 3000',()=>{
  const d=cart(),doc=d.window.document;doc.getElementById('unit').textContent='$2,690';doc.getElementById('line').textContent='$2,690';doc.getElementById('target').insertAdjacentHTML('beforeend','<span>$2,690</span><p>已折 $54</p>');
  const plan=d.window.ShoppeeCart.prepare(doc,cmd());
  assert.equal(plan.cart.total_minor,263600);assert.equal(plan.button.id,'go');
});
test('reference unit prices and row subtotals do not control cart-total budget',()=>{
  const d=cart(),doc=d.window.document;doc.getElementById('unit').textContent='$9,000';doc.getElementById('line').remove();
  assert.equal(d.window.ShoppeeCart.prepare(doc,cmd()).cart.total_minor,263600);
  doc.getElementById('unit').remove();assert.equal(d.window.ShoppeeCart.prepare(doc,cmd()).cart.total_minor,263600);
});
test('cart total above cap stops, equal cap and explicit zero total pass',()=>{
  const d=cart(),doc=d.window.document,api=d.window.ShoppeeCart;
  doc.getElementById('total').textContent='總金額 (1 件商品): $3,000.01';
  assert.throws(()=>api.prepare(doc,cmd()),/總金額 NT\$3000\.01 超過預算上限 NT\$3000\.00/);
  for(const amount of ['$3,000','$0']){doc.getElementById('total').textContent='總金額 (1 件商品): '+amount;assert.equal(api.prepare(doc,cmd()).button.id,'go');}
});
test('missing, malformed and range totals remain blocked',()=>{
  for(const value of ['總金額 (1 件商品): 載入中','總金額 (1 件商品): $2,63','總金額 (1 件商品): $2,636.000','總金額 (1 件商品): $-1','總金額 (1 件商品): $2,636 - $2,690']){
    const d=cart(),doc=d.window.document;doc.getElementById('total').textContent=value;
    assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd()),/總金額/);
  }
});
test('budget reads the checkout footer rather than totals embedded in product rows',()=>{
  const d=cart(),doc=d.window.document;doc.getElementById('target').insertAdjacentHTML('beforeend','<p>總金額 (1 件商品): $1</p>');
  doc.getElementById('total').textContent='總金額 (1 件商品): $5,000';assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd()),/5000/);
  doc.getElementById('total').remove();assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd()),/獨立購物車總金額區塊/);
});
test('visible duplicate totals block but hidden duplicate footer total is ignored',()=>{
  const d=cart(),doc=d.window.document;doc.getElementById('total').insertAdjacentHTML('afterend','<p id="duplicate">總金額 (1 件商品): $2,690</p>');
  assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd()),/多個總金額/);
  doc.getElementById('duplicate').hidden=true;assert.equal(d.window.ShoppeeCart.prepare(doc,cmd()).cart.total_minor,263600);
});
test('cart footer can count the selected row or its explicit quantity',()=>{
  const d=cart(),doc=d.window.document;doc.getElementById('qty').value='2';
  for(const count of [1,2]){doc.getElementById('total').textContent=`總金額 (${count} 件商品): $2,636`;assert.equal(d.window.ShoppeeCart.prepare(doc,{...cmd(),quantity:2}).cart.quantity,2);}
  doc.getElementById('total').textContent='總金額 (3 件商品): $2,636';assert.throws(()=>d.window.ShoppeeCart.prepare(doc,{...cmd(),quantity:2}),/件數/);
});
test('discounted total must settle and verified receipt includes the displayed total',async()=>{
  const d=cart(),doc=d.window.document;let time=0;doc.getElementById('total').textContent='總金額 (1 件商品): $2,690';
  const result=await d.window.ShoppeeCart.dispatch(doc,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;if(time>=500)doc.getElementById('total').textContent='總金額 (1 件商品): $2,636';}});
  assert.equal(time,1700);assert.equal(result.cart.total_minor,263600);
});
test('deferred click rechecks total and never clicks after a budget change',async()=>{
  const d=cart(),doc=d.window.document;let time=0,go=0,queued=[];doc.getElementById('go').onclick=()=>go++;
  const result=await d.window.ShoppeeCart.dispatch(doc,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;},scheduleClick:fn=>queued.push(fn)});
  assert.equal(result.cart.total_minor,263600);assert.equal(go,0);doc.getElementById('total').textContent='總金額 (1 件商品): $5,000';
  assert.throws(()=>queued[0](),/超過預算/);assert.equal(go,0);
});
test('other selected goods and inconsistent select-all state block cart transition',()=>{
  for(const id of ['other-check','all']){
    const d=cart(),doc=d.window.document;doc.getElementById(id).checked=true;
    assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd()),/其他勾選/);
  }
});
function frameworkBox(doc,id,selected=true){
  const box=doc.getElementById(id);box.removeAttribute('aria-checked');
  box.className='stardust-checkbox'+(selected?' stardust-checkbox--checked':'');
  box.innerHTML='<input type="checkbox" class="stardust-checkbox__input" style="display:none"><span class="stardust-checkbox__box"></span>';
  return box;
}
function shopHeader(d){
  const doc=d.window.document,shop=doc.createElement('section'),header=doc.createElement('header'),box=doc.createElement('div');
  const row=doc.getElementById('target');row.before(shop);box.id='shop-selected';header.appendChild(box);shop.append(header,row);
  frameworkBox(doc,'selected');frameworkBox(doc,'shop-selected');return shop;
}
test('rendered framework selection is read even when backing input stays unchecked',()=>{
  const d=cart(),doc=d.window.document,box=frameworkBox(doc,'selected');
  assert.equal(box.querySelector('input').checked,false);
  assert.equal(d.window.ShoppeeCart.prepare(doc,cmd()).button.id,'go');
});
test('ARIA-owned checkbox is not overridden by an unchecked backing input',()=>{
  const d=cart(),doc=d.window.document;doc.getElementById('selected').innerHTML='<input type="checkbox" style="display:none">';
  assert.equal(d.window.ShoppeeCart.prepare(doc,cmd()).button.id,'go');
});
test('an unchecked rendered framework checkbox is not overridden by a checked input',()=>{
  const d=cart(),doc=d.window.document,box=frameworkBox(doc,'selected',false);box.querySelector('input').checked=true;
  assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd()),/尚未勾選/);
});
test('shop header plus its one selected product is one purchase, not two',async()=>{
  const d=cart(),doc=d.window.document;shopHeader(d);let time=0,go=0,final=0;
  doc.getElementById('go').onclick=()=>go++;doc.getElementById('final').onclick=()=>final++;
  await d.window.ShoppeeCart.dispatch(doc,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;}});
  assert.equal(go,1);assert.equal(final,0);
});
test('a store header cannot hide a second selected item even with stale one-item total',()=>{
  const d=cart(),doc=d.window.document,shop=shopHeader(d);shop.appendChild(doc.getElementById('other'));doc.getElementById('other-check').checked=true;
  assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd()),/其他勾選/);
});
test('unknown selected product rows and contradictory rendered state stay blocked',()=>{
  const d=cart(),doc=d.window.document,shop=shopHeader(d),other=doc.getElementById('other');
  shop.appendChild(other);other.querySelector('input[type="text"]').remove();doc.getElementById('other-check').checked=true;
  assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd()),/其他勾選/);
  const e=cart(),box=frameworkBox(e.window.document,'selected');box.setAttribute('aria-checked','false');
  assert.throws(()=>e.window.ShoppeeCart.prepare(e.window.document,cmd()),/尚未同步/);
});
test('delayed visual checkbox state waits before proceeding',async()=>{
  const d=cart(),doc=d.window.document,box=frameworkBox(doc,'selected',false);let time=0,go=0;
  doc.getElementById('go').onclick=()=>go++;
  await d.window.ShoppeeCart.dispatch(doc,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;if(time>=500)box.classList.add('stardust-checkbox--checked');}});
  assert.equal(time,1700);assert.equal(go,1);
});
test('wrong SKU, quantity, total count and budget cap stop',()=>{
  const cases=[
    [doc=>doc.getElementById('sku').textContent='Buddh方向盤組(含T119盤面)',/規格/],
    [doc=>doc.getElementById('qty').value='2',/數量/],
    [doc=>doc.getElementById('total').textContent='總金額 (2 件商品): $2,736',/件數/],
  ];
  for(const [mutate,pattern] of cases){const d=cart(),doc=d.window.document;mutate(doc);assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd()),pattern);}
  const d=cart();assert.throws(()=>d.window.ShoppeeCart.prepare(d.window.document,{...cmd(),max_price_minor:200000}),/預算/);
});
test('duplicate target rows, unchecked target and disabled checkout stop',()=>{
  const d=cart(),doc=d.window.document,api=d.window.ShoppeeCart;
  doc.getElementById('target').after(doc.getElementById('target').cloneNode(true));
  assert.throws(()=>api.prepare(doc,cmd()),/多個規格列/);
  const e=cart();e.window.document.getElementById('selected').setAttribute('aria-checked','false');
  assert.throws(()=>e.window.ShoppeeCart.prepare(e.window.document,cmd()),/尚未勾選/);
  const f=cart();f.window.document.getElementById('go').disabled=true;
  assert.throws(()=>f.window.ShoppeeCart.prepare(f.window.document,cmd()),/入口不可用/);
});
test('variant match uses full labels, not substring matches',()=>{
  const d=cart();d.window.document.getElementById('sku').textContent='規格：XL';
  assert.throws(()=>d.window.ShoppeeCart.prepare(d.window.document,{...cmd(),selections:[{group:'規格',option:'L'}]}),/規格/);
});
test('hidden duplicate rows are ignored, native nested checkboxes count once',()=>{
  const d=cart(),doc=d.window.document,copy=doc.getElementById('target').cloneNode(true);copy.hidden=true;doc.body.appendChild(copy);
  const box=doc.getElementById('selected');box.className='stardust-checkbox stardust-checkbox--checked';box.innerHTML='<input type="checkbox" checked>';
  assert.equal(d.window.ShoppeeCart.prepare(doc,cmd()).button.id,'go');
});
test('modal, expiry, invalid budget and wrong product stop without guessing',()=>{
  const d=cart(),doc=d.window.document,api=d.window.ShoppeeCart;
  assert.throws(()=>api.prepare(doc,{...cmd(),expires_unix_ms:0}),/逾時/);
  assert.throws(()=>api.prepare(doc,{...cmd(),url:'https://shopee.tw/product/8/9'}),/尚未找到/);
  assert.throws(()=>api.prepare(doc,{...cmd(),max_price_minor:0}),/預算上限無效/);
  const modal=doc.createElement('div');modal.setAttribute('role','dialog');doc.body.appendChild(modal);
  assert.throws(()=>api.prepare(doc,cmd()),/待處理提示/);
});
test('cart dispatch clicks checkout once after stable data and never final order',async()=>{
  const d=cart(),doc=d.window.document;let time=0,go=0,final=0;
  doc.getElementById('go').onclick=()=>go++;doc.getElementById('final').onclick=()=>final++;
  const result=await d.window.ShoppeeCart.dispatch(doc,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;}});
  assert.equal(result.dispatched,true);assert.equal(go,1);assert.equal(final,0);assert.equal(time,1200);
});
test('cart mutation during wait stops before a checkout click',async()=>{
  const d=cart(),doc=d.window.document;let time=0,go=0;doc.getElementById('go').onclick=()=>go++;
  await assert.rejects(d.window.ShoppeeCart.dispatch(doc,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;doc.getElementById('other-check').checked=true;}}),/其他勾選/);
  assert.equal(go,0);
});
test('cart waits for delayed rendered content before a single checkout click',async()=>{
  const d=cart(),doc=d.window.document,row=doc.getElementById('target');let time=0,go=0;
  row.hidden=true;doc.getElementById('go').onclick=()=>go++;
  await d.window.ShoppeeCart.dispatch(doc,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;if(time>=600)row.hidden=false;}});
  assert.equal(time,1800);assert.equal(go,1);
});
test('content listener rejects a repeated cart command without a second click',async()=>{
  const d=cart(),w=d.window,doc=w.document;let listener,time=0,go=0,final=0;
  w.chrome={runtime:{id:'fixture',onMessage:{addListener:fn=>{listener=fn;}}}};
  const clicks=[];w.Date.now=()=>time;w.setTimeout=(fn,ms)=>{if(ms===0){clicks.push(fn);return;}time+=ms;Promise.resolve().then(fn);};
  w.eval(fs.readFileSync('companion-extension/cart.js','utf8'));
  doc.getElementById('go').onclick=()=>go++;doc.getElementById('final').onclick=()=>final++;
  const message={type:'CART_COMMAND',command:cmd()};
  const first=new Promise(resolve=>listener(message,{id:'fixture'},resolve));
  const second=await new Promise(resolve=>listener(message,{id:'fixture'},resolve));
  assert.match(second.error,/重複/);const receipt=(await first).outcome;assert.equal(receipt.cart.total_minor,263600);assert.equal(go,0);assert.equal(clicks.length,1);clicks[0]();
  assert.equal(go,1);assert.equal(final,0);w.close();
});
function flow(){const context=vm.createContext({URL,Date,Promise,setTimeout});vm.runInContext(fs.readFileSync('companion-extension/flow.js','utf8'),context);return context.ShoppeeFlow;}
test('navigation covers product -> cart -> checkout with one cart command',async()=>{
  let time=0,sends=0;
  const tabs={get:async()=>({status:'complete',url:sends?'https://shopee.tw/checkout':time<200?url:'https://shopee.tw/cart'}),sendMessage:async(id,m)=>{assert.equal(m.type,'CART_COMMAND');sends++;return {outcome:{dispatched:true,cart:{total_minor:263600,quantity:1,item_count:1}}};}};
  const receipt=await flow().checkout(tabs,1,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;}});
  assert.equal(receipt.total_minor,263600);assert.equal(receipt.quantity,1);assert.equal(sends,1);
});
test('direct checkout without a verified cart total and login/foreign redirects stop',async()=>{
  for(const path of ['https://shopee.tw/checkout','https://shopee.tw/buyer/login','https://example.com/checkout']){
    let sends=0;const run=flow().checkout({get:async()=>({status:'complete',url:path}),sendMessage:async()=>sends++},1,cmd());
    await assert.rejects(run,path==='https://shopee.tw/checkout'?/尚未核對購物車總金額/:/停止/);assert.equal(sends,0);
  }
});
test('cart guard errors propagate; unknown dispatch never retries',async()=>{
  for(const response of ['error','disconnect']){
    let sends=0;const tabs={get:async()=>({status:'complete',url:'https://shopee.tw/cart'}),sendMessage:async()=>{sends++;if(response==='disconnect')throw new Error('channel closed');return {error:'fixture guard stopped'};}};
    await assert.rejects(flow().checkout(tabs,1,cmd()),response==='disconnect'?/不會重送/:/fixture guard/);assert.equal(sends,1);
  }
});
test('navigation cannot fabricate a verified cart total when the response channel closes',async()=>{
  let sent=false,time=0;
  const tabs={get:async()=>({status:'complete',url:sent?'https://shopee.tw/checkout':'https://shopee.tw/cart'}),sendMessage:async()=>{sent=true;throw new Error('navigation');}};
  await assert.rejects(flow().checkout(tabs,1,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;}}),/無法確認購物車總金額/);
});
test('a cart that does not navigate times out without a second attempt',async()=>{
  let time=0,sends=0;const tabs={get:async()=>({status:'complete',url:'https://shopee.tw/cart'}),sendMessage:async()=>{sends++;return {outcome:{dispatched:true,cart:{total_minor:263600,quantity:1,item_count:1}}};}};
  await assert.rejects(flow().checkout(tabs,1,cmd(),{now:()=>time,sleep:async ms=>{time+=ms;}}),/未確認結帳頁/);assert.equal(sends,1);
});

test('missing, over-budget and wrong-quantity cart receipts never report success',async()=>{
  for(const summary of [null,{total_minor:300001,quantity:1,item_count:1},{total_minor:263600,quantity:2,item_count:1},{total_minor:263600,quantity:1,item_count:2}]){
    let sends=0;const tabs={get:async()=>({status:'complete',url:'https://shopee.tw/cart'}),sendMessage:async()=>{sends++;return {outcome:{dispatched:true,cart:summary}};}};
    await assert.rejects(flow().checkout(tabs,1,cmd()),/核對結果無效/);assert.equal(sends,1);
  }
});
