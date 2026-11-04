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
  assert.equal(d.window.ShoppeeCart.prepare(doc,cmd(),263600).button.id,'go');assert.equal(clicked,0);
});
test('other selected goods and aggregate checkboxes block cart transition',()=>{
  for(const id of ['other-check','all']){
    const d=cart(),doc=d.window.document;doc.getElementById(id).checked=true;
    assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd(),263600),/其他勾選/);
  }
});
test('wrong SKU, wrong quantity, changed price, cap and footer count stop',()=>{
  const cases=[
    [doc=>doc.getElementById('sku').textContent='Buddh方向盤組(含T119盤面)',/規格/],
    [doc=>doc.getElementById('qty').value='2',/數量/],
    [doc=>doc.getElementById('unit').textContent='$2,637',/單價/],
    [doc=>doc.getElementById('total').textContent='總金額 (2 件商品): $2,736',/只有一項/],
    [doc=>doc.getElementById('total').textContent='總金額 (1 件商品): $3,000',/金額/]
  ];
  for(const [mutate,pattern] of cases){const d=cart(),doc=d.window.document;mutate(doc);assert.throws(()=>d.window.ShoppeeCart.prepare(doc,cmd(),263600),pattern);}
  const d=cart();assert.throws(()=>d.window.ShoppeeCart.prepare(d.window.document,{...cmd(),max_price_minor:200000},263600),/預算/);
});
test('duplicate target rows, unchecked target and disabled checkout stop',()=>{
  const d=cart(),doc=d.window.document,api=d.window.ShoppeeCart;
  doc.getElementById('target').after(doc.getElementById('target').cloneNode(true));
  assert.throws(()=>api.prepare(doc,cmd(),263600),/多個規格列/);
  const e=cart();e.window.document.getElementById('selected').setAttribute('aria-checked','false');
  assert.throws(()=>e.window.ShoppeeCart.prepare(e.window.document,cmd(),263600),/尚未勾選/);
  const f=cart();f.window.document.getElementById('go').disabled=true;
  assert.throws(()=>f.window.ShoppeeCart.prepare(f.window.document,cmd(),263600),/入口不可用/);
});
test('variant match uses full labels, not substring matches',()=>{
  const d=cart();d.window.document.getElementById('sku').textContent='規格：XL';
  assert.throws(()=>d.window.ShoppeeCart.prepare(d.window.document,{...cmd(),selections:[{group:'規格',option:'L'}]},263600),/規格/);
});
test('hidden duplicate rows are ignored, native nested checkboxes count once',()=>{
  const d=cart(),doc=d.window.document,copy=doc.getElementById('target').cloneNode(true);copy.hidden=true;doc.body.appendChild(copy);
  const box=doc.getElementById('selected');box.className='stardust-checkbox stardust-checkbox--checked';box.innerHTML='<input type="checkbox" checked>';
  assert.equal(d.window.ShoppeeCart.prepare(doc,cmd(),263600).button.id,'go');
});
test('modal, expired command, missing price and wrong product stop without guessing',()=>{
  const d=cart(),doc=d.window.document,api=d.window.ShoppeeCart;
  assert.throws(()=>api.prepare(doc,{...cmd(),expires_unix_ms:0},263600),/逾時/);
  assert.throws(()=>api.prepare(doc,{...cmd(),url:'https://shopee.tw/product/8/9'},263600),/尚未找到/);
  assert.throws(()=>api.prepare(doc,cmd(),null),/價格無效/);
  const modal=doc.createElement('div');modal.setAttribute('role','dialog');doc.body.appendChild(modal);
  assert.throws(()=>api.prepare(doc,cmd(),263600),/待處理提示/);
});
test('cart dispatch clicks checkout once after stable data and never final order',async()=>{
  const d=cart(),doc=d.window.document;let time=0,go=0,final=0;
  doc.getElementById('go').onclick=()=>go++;doc.getElementById('final').onclick=()=>final++;
  const result=await d.window.ShoppeeCart.dispatch(doc,cmd(),263600,{now:()=>time,sleep:async ms=>{time+=ms;}});
  assert.equal(result.dispatched,true);assert.equal(go,1);assert.equal(final,0);assert.equal(time,1200);
});
test('cart mutation during wait stops before a checkout click',async()=>{
  const d=cart(),doc=d.window.document;let time=0,go=0;doc.getElementById('go').onclick=()=>go++;
  await assert.rejects(d.window.ShoppeeCart.dispatch(doc,cmd(),263600,{now:()=>time,sleep:async ms=>{time+=ms;doc.getElementById('other-check').checked=true;}}),/其他勾選/);
  assert.equal(go,0);
});
test('cart waits for delayed rendered content before a single checkout click',async()=>{
  const d=cart(),doc=d.window.document,row=doc.getElementById('target');let time=0,go=0;
  row.hidden=true;doc.getElementById('go').onclick=()=>go++;
  await d.window.ShoppeeCart.dispatch(doc,cmd(),263600,{now:()=>time,sleep:async ms=>{time+=ms;if(time>=600)row.hidden=false;}});
  assert.equal(time,1800);assert.equal(go,1);
});
test('content listener rejects a repeated cart command without a second click',async()=>{
  const d=cart(),w=d.window,doc=w.document;let listener,time=0,go=0,final=0;
  w.chrome={runtime:{id:'fixture',onMessage:{addListener:fn=>{listener=fn;}}}};
  w.Date.now=()=>time;w.setTimeout=(fn,ms)=>{time+=ms;Promise.resolve().then(fn);};
  w.eval(fs.readFileSync('companion-extension/cart.js','utf8'));
  doc.getElementById('go').onclick=()=>go++;doc.getElementById('final').onclick=()=>final++;
  const message={type:'CART_COMMAND',command:cmd(),expected_price_minor:263600};
  const first=new Promise(resolve=>listener(message,{id:'fixture'},resolve));
  const second=await new Promise(resolve=>listener(message,{id:'fixture'},resolve));
  assert.match(second.error,/重複/);assert.equal((await first).outcome.dispatched,true);
  assert.equal(go,1);assert.equal(final,0);w.close();
});
function flow(){const context=vm.createContext({URL,Date,Promise,setTimeout});vm.runInContext(fs.readFileSync('companion-extension/flow.js','utf8'),context);return context.ShoppeeFlow;}
test('navigation covers product -> cart -> checkout with one cart command',async()=>{
  let time=0,sends=0;
  const tabs={get:async()=>({status:'complete',url:sends?'https://shopee.tw/checkout':time<200?url:'https://shopee.tw/cart'}),sendMessage:async(id,m)=>{assert.equal(m.type,'CART_COMMAND');sends++;return {outcome:{dispatched:true}};}};
  await flow().checkout(tabs,1,cmd(),{unit_price_minor:263600},{now:()=>time,sleep:async ms=>{time+=ms;}});
  assert.equal(sends,1);
});
test('direct checkout skips cart; login/foreign redirect never dispatch cart',async()=>{
  for(const path of ['https://shopee.tw/checkout','https://shopee.tw/buyer/login','https://example.com/checkout']){
    let sends=0;const run=flow().checkout({get:async()=>({status:'complete',url:path}),sendMessage:async()=>sends++},1,cmd(),{});
    if(path==='https://shopee.tw/checkout')await run;else await assert.rejects(run,/停止/);
    assert.equal(sends,0);
  }
});
test('cart guard errors propagate; unknown dispatch never retries',async()=>{
  for(const response of ['error','disconnect']){
    let sends=0;const tabs={get:async()=>({status:'complete',url:'https://shopee.tw/cart'}),sendMessage:async()=>{sends++;if(response==='disconnect')throw new Error('channel closed');return {error:'fixture guard stopped'};}};
    await assert.rejects(flow().checkout(tabs,1,cmd(),{}),response==='disconnect'?/不會重送/:/fixture guard/);assert.equal(sends,1);
  }
});
test('navigation closing cart response is accepted only if checkout is observed',async()=>{
  let sent=false,time=0;
  const tabs={get:async()=>({status:'complete',url:sent?'https://shopee.tw/checkout':'https://shopee.tw/cart'}),sendMessage:async()=>{sent=true;throw new Error('navigation');}};
  await flow().checkout(tabs,1,cmd(),{},{now:()=>time,sleep:async ms=>{time+=ms;}});
});
test('a cart that does not navigate times out without a second attempt',async()=>{
  let time=0,sends=0;const tabs={get:async()=>({status:'complete',url:'https://shopee.tw/cart'}),sendMessage:async()=>{sends++;return {outcome:{dispatched:true}};}};
  await assert.rejects(flow().checkout(tabs,1,cmd(),{},{now:()=>time,sleep:async ms=>{time+=ms;}}),/未確認結帳頁/);assert.equal(sends,1);
});
