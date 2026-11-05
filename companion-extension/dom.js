/* Public, rendered product DOM only. No cookies, network APIs or app state. */
(function(root){
  'use strict';
  const text=el=>(el.innerText ?? el.textContent ?? '').trim();
  function page(doc,url){
    const u=new URL(url),m=u.pathname.match(/(?:\/product\/|(?:-i\.))(\d+)[/.](\d+)\/?$/);
    if(u.origin!=='https://shopee.tw'||!m)throw new Error('請在蝦皮登入後重新讀取商品；不會自動處理登入或安全驗證。');
    const h=doc.querySelector('h1');
    if(!h)throw new Error('尚未找到商品標題，請確認商品頁已完成載入。');
    const container=h.parentElement.parentElement;
    const groups=[...container.querySelectorAll('section')].map(section=>{
      const buttons=[...section.querySelectorAll('button.selection-box-unselected,button.selection-box-selected')];
      if(!buttons.length)return null;
      const heading=section.querySelector('h2');
      if(!heading)throw new Error('規格結構已改變，停止操作。');
      return {name:text(heading),options:buttons.map(b=>({label:b.getAttribute('aria-label')||text(b),disabled:b.disabled||b.getAttribute('aria-disabled')==='true',selected:b.classList.contains('selection-box-selected')}))};
    }).filter(Boolean);
    const priceEls=[...container.querySelectorAll('.pyzxvq')];
    let price=null;
    if(priceEls.length===1){const match=text(priceEls[0]).match(/^\$\s*([\d,]+)(?:\.(\d{1,2}))?$/);if(match)price=Number(match[1].replaceAll(',',''))*100+Number((match[2]||'').padEnd(2,'0'));}
    if(!Number.isSafeInteger(price)||price<0||price>100_000_000_000)price=null;
    const quantitySection=[...container.querySelectorAll('section')].find(s=>text(s.querySelector('h2')||{})==='數量');
    const stockMatch=quantitySection&&text(quantitySection).match(/還剩\s*([\d,]+)\s*件/);
    return {canonical_url:`https://shopee.tw/product/${m[1]}/${m[2]}`,title:text(h),groups,unit_price_minor:price,stock:stockMatch?Number(stockMatch[1].replaceAll(',','')):null,shipping_blocked:text(container).includes('您選擇的門市或預設地址暫不適用此物流選項')};
  }
  function choiceButton(doc,selection){
    const sections=[...doc.querySelectorAll('section')].filter(s=>text(s.querySelector('h2')||{})===selection.group && s.querySelector('button.selection-box-unselected,button.selection-box-selected'));
    if(sections.length!==1)throw new Error('規格群組不明確，停止操作。');
    const buttons=[...sections[0].querySelectorAll('button.selection-box-unselected,button.selection-box-selected')].filter(b=>(b.getAttribute('aria-label')||text(b))===selection.option);
    if(buttons.length!==1||buttons[0].disabled||buttons[0].getAttribute('aria-disabled')==='true')throw new Error('所選規格目前停用或不存在。');
    return buttons[0];
  }
  function validate(catalog,command,doc){
    if(catalog.canonical_url!==command.url)throw new Error('商品頁與任務不一致。');
    if(!Number.isInteger(command.quantity)||command.quantity<1||command.quantity>100)throw new Error('數量不正確。');
    if(catalog.shipping_blocked)throw new Error('目前預設地址／門市不適用配送，請在蝦皮修改後重新讀取。');
    if(catalog.groups.length!==command.selections.length)throw new Error('尚未選擇所有規格。');
    for(const group of catalog.groups){const selected=command.selections.filter(s=>s.group===group.name);if(selected.length!==1||group.options.filter(o=>o.selected).length!==1||!group.options.some(o=>o.label===selected[0].option&&o.selected&&!o.disabled))throw new Error('頁面實際選取的規格與任務不符。');}
    if(!Number.isSafeInteger(command.max_price_minor)||command.max_price_minor<=0)throw new Error('預算上限無效。');
    if(catalog.stock!==null&&catalog.stock<command.quantity)throw new Error('庫存不足。');
    const inputs=[...doc.querySelectorAll('input[role="spinbutton"]')];
    if(inputs.length!==1||Number(inputs[0].value)!==command.quantity)throw new Error('數量未與任務一致。');
    const buttons=[...doc.querySelectorAll('button')].filter(b=>text(b)==='直接購買');
    if(buttons.length!==1||buttons[0].disabled||buttons[0].getAttribute('aria-disabled')==='true')throw new Error('購買入口不可用，停止操作。');
    return buttons[0];
  }
  root.ShoppeeDom={page,choiceButton,validate};
})(globalThis);
