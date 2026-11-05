/* Navigation state machine; cart transition can be dispatched only once. */
(function(root){
  'use strict';
  async function checkout(tabs,tabId,command,options={}){
    const now=options.now||Date.now,sleep=options.sleep||(ms=>new Promise(r=>setTimeout(r,ms)));
    const deadline=Math.min(command.expires_unix_ms,now()+30000);
    let cartAttempted=false,verifiedCart=null;
    while(now()<deadline){
      const tab=await tabs.get(tabId),u=new URL(tab.url),path=u.pathname;
      if(u.origin!=='https://shopee.tw')throw new Error('商品分頁離開蝦皮，已停止操作。');
      if(path.startsWith('/verify/')||path.startsWith('/buyer/'))throw new Error('網站要求驗證或重新登入，已停止操作。');
      if(tab.status==='complete'&&(path==='/checkout'||path.startsWith('/checkout/'))){
        if(!verifiedCart)throw new Error('已進入結帳頁，但尚未核對購物車總金額；請人工確認，程式不會下單或付款。');
        return verifiedCart;
      }
      if(tab.status==='complete'&&/^\/cart\/?$/.test(path)&&!cartAttempted){
        cartAttempted=true;
        let result;
        try{result=await tabs.sendMessage(tabId,{type:'CART_COMMAND',command});}
        catch{
          throw new Error('無法確認購物車總金額與操作結果，已停止且不會重送；請檢查購物車。');
        }
        if(result?.error)throw new Error(result.error);
        if(!result?.outcome?.dispatched)throw new Error('購物車沒有確認派發，已停止且不會重送。');
        const cart=result.outcome.cart;
        if(!cart||!Number.isSafeInteger(cart.total_minor)||cart.total_minor<0||cart.total_minor>command.max_price_minor||cart.quantity!==command.quantity||![1,command.quantity].includes(cart.item_count))throw new Error('購物車總金額或數量核對結果無效；請重新載入新版擴充功能與蝦皮分頁。');
        verifiedCart=cart;
      }
      await sleep(200);
    }
    throw new Error(cartAttempted?'已嘗試前往結帳，但未確認結帳頁；請檢查購物車，程式不會重送。':'沒有確認進入購物車或結帳頁，請檢查商品分頁；程式不會重送購買。');
  }
  root.ShoppeeFlow={checkout};
})(globalThis);
