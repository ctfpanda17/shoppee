const {test}=require('node:test');const assert=require('node:assert/strict');const fs=require('node:fs');const {JSDOM}=require('jsdom');
const script=fs.readFileSync('companion-extension/dom.js','utf8');
function fixture(){
 const dom=new JSDOM(`<main><div id="product"><div><h1>FlashFire fixture</h1></div><div class="pyzxvq">$2,636</div><section><h2>規格</h2><div><button aria-label="Sepang方向盤組(含T118盤面)" aria-disabled="false" class="selection-box-selected">Sepang方向盤組(含T118盤面)</button><button aria-label="Buddh方向盤組(含T119盤面)" aria-disabled="false" class="selection-box-unselected">Buddh方向盤組(含T119盤面)</button><button aria-label="PS5" aria-disabled="true" class="selection-box-unselected">PS5</button></div></section><section><h2>數量</h2><input role="spinbutton" value="1"><div>還剩9件</div></section><button id="buy" aria-disabled="false">直接購買</button><button id="final">下單</button></div></main>`,{url:'https://shopee.tw/product/116309859/41078532632',runScripts:'outside-only'});
 dom.window.eval(script);return dom;
}
function command(){return {url:'https://shopee.tw/product/116309859/41078532632',selections:[{group:'規格',option:'Sepang方向盤組(含T118盤面)'}],quantity:1,max_price_minor:300000};}
test('extracts rendered SKU states, selected price and stock without account data',()=>{
 const d=fixture(),c=d.window.ShoppeeDom.page(d.window.document,d.window.location.href);
 assert.equal(c.groups[0].options.length,3);assert.equal(c.groups[0].options[2].disabled,true);assert.equal(c.unit_price_minor,263600);assert.equal(c.stock,9);assert.equal(c.groups[0].options[0].selected,true);
});
test('guard returns only the direct-buy entry, never clicks or returns final order',()=>{
 const d=fixture(),doc=d.window.document;let clicks=0;doc.getElementById('buy').onclick=()=>clicks++;
 const b=d.window.ShoppeeDom.validate(d.window.ShoppeeDom.page(doc,d.window.location.href),command(),doc);
 assert.equal(b.id,'buy');assert.equal(clicks,0);assert.notEqual(b.id,'final');
});
test('shipping restriction and price/quantity/selected-SKU changes stop preparation',()=>{
 const d=fixture(),doc=d.window.document,api=d.window.ShoppeeDom;
 let c=api.page(doc,d.window.location.href);c.shipping_blocked=true;assert.throws(()=>api.validate(c,command(),doc),/配送/);
 c=api.page(doc,d.window.location.href);c.unit_price_minor=300001;assert.throws(()=>api.validate(c,command(),doc),/預算/);
 c=api.page(doc,d.window.location.href);c.stock=0;assert.throws(()=>api.validate(c,command(),doc),/庫存/);
 c=api.page(doc,d.window.location.href);c.groups[0].options[1].selected=true;assert.throws(()=>api.validate(c,command(),doc),/規格/);
 c=api.page(doc,d.window.location.href);c.groups[0].options[0].selected=false;assert.throws(()=>api.validate(c,command(),doc),/規格/);
 doc.querySelector('input').value='2';c=api.page(doc,d.window.location.href);assert.throws(()=>api.validate(c,command(),doc),/數量/);
});
test('disabled and duplicate option controls are never guessed',()=>{
 const d=fixture(),doc=d.window.document,api=d.window.ShoppeeDom;
 assert.throws(()=>api.choiceButton(doc,{group:'規格',option:'PS5'}),/停用/);
 const first=doc.querySelector('.selection-box-selected');first.after(first.cloneNode(true));assert.throws(()=>api.choiceButton(doc,command().selections[0]),/不存在/);
});
test('ambiguous price and mismatched product stop, login page is not treated as a product',()=>{
 const d=fixture(),doc=d.window.document,api=d.window.ShoppeeDom;
 doc.querySelector('.pyzxvq').textContent='$841 - $2,636';let c=api.page(doc,d.window.location.href);assert.equal(c.unit_price_minor,null);assert.throws(()=>api.validate(c,command(),doc),/價格/);
 assert.throws(()=>api.page(doc,'https://shopee.tw/buyer/login'),/登入/);
 c=api.page(doc,d.window.location.href);c.canonical_url='https://shopee.tw/product/1/2';assert.throws(()=>api.validate(c,command(),doc),/商品頁/);
});
test('local UI boots without randomUUID and blocks commands until extension connects',async()=>{
 const html=fs.readFileSync('ui/index.html','utf8');const source=fs.readFileSync('ui/app.js','utf8');
 const d=new JSDOM(html,{url:'http://127.0.0.1:8788/#'+'a'.repeat(64),runScripts:'outside-only'});
 Object.defineProperty(d.window.crypto,'randomUUID',{value:undefined});
 d.window.fetch=async()=>({ok:true,json:async()=>({version:'0.3.0',metrics:{successful_ms:{}},job:{phase:'idle',mode:null,target_unix_ms:null,quote:null,request_count:0,schedule_lateness_ms:null,logs:[],browser_catalog:null}})});
 try{
  d.window.eval(source);await new Promise(resolve=>setImmediate(resolve));
  assert.match(d.window.document.documentElement.dataset.shoppeeBridgeNonce,/^[a-f0-9]{32}$/);
  assert.equal(d.window.document.getElementById('start-label').textContent,'選規格並自動前往結帳');
  d.window.document.getElementById('read-browser').click();await new Promise(resolve=>setImmediate(resolve));
  assert.match(d.window.document.getElementById('form-error').textContent,/先安裝擴充功能/);
  assert.equal(d.window.location.hash,'');
 }finally{d.window.close();}
});
