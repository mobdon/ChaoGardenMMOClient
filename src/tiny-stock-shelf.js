// Native shop coordinates from A2NE 02007688, 02007978, 02008B7C and 02008BC4.
// This adapter selects a detail view. Only the existing care adapters can buy.
export function shelfStock(care) {
  const items = care?.tinyShop;
  if (!items?.enabled || items.rulesVersion !== 'adapted-tiny-shop-v1') return [];
  const rows = [];
  const valid = p => Number.isSafeInteger(p) && p >= 0 && p <= 99999;
  for (let i = 0; i < 7; ++i) {
    const f = items.items?.find(f => f.itemId === 'tiny-fruit:' + i);
    if (f && valid(f.price)) rows.push({ slot:i, key:f.itemId, name:f.name, price:f.price, art:'fruit-'+i });
  }
  const shop=care.tinyEggShop, egg=shop?.offers?.[0];
  if (shop?.enabled && !care.tinyCollection?.pendingEgg && Number.isInteger(egg?.eggId) && egg.eggId>=0 && egg.eggId<12 && valid(egg.price))
    rows.push({ slot:7, key:'egg', name:egg.eggId===0?'Normal Egg':'Jewel Egg', price:egg.price, art:'egg-'+egg.eggId });
  const toy=care.tinyToys?.items?.find(t=>t.offered && !t.owned);
  if (Number.isInteger(toy?.id) && toy.id>=0 && toy.id<3 && valid(toy.price))
    rows.push({slot:8,key:'toy:'+toy.id,name:toy.name,price:toy.price,art:'toy-'+toy.id});
  return rows;
}

export function createStockShelf(fruit, eggs, inventory) {
  const doc=fruit.ownerDocument;
  const make=(tag,cls,text)=>{const n=doc.createElement(tag);n.className=cls;if(text)n.textContent=text;return n;};
  const shell=make('section','market'), head=make('header','market-head'), shop=make('button','market-tab','Shop'), bag=make('button','market-tab','Inventory'), wallet=make('span','market-wallet');
  const shelf=make('div','market-shelf'), details=make('div','market-actions'), locker=make('div','market-bag');
  const back=make('button','market-back','Back to garden');
  back.onclick=()=>{if(window.parent!==window)window.parent.postMessage({type:'tiny-phone-open'},location.origin);else doc.querySelector('[data-screen="garden"]')?.click();};
  shelf.setAttribute('role','group');shelf.setAttribute('aria-label','Shop shelf');
  [shop,bag,back].forEach(b=>b.type='button');inventory.classList.add('stock-inventory');
  fruit.before(shell);shell.append(head,shelf,locker,back);head.append(shop,bag,wallet);
  locker.append(inventory);details.append(fruit,eggs);shelf.append(details);
  let selected=null,inBag=false,owner,signature,rows=[],buttons=[];
  function display(){
    if(inBag){if(details.parentNode!==locker)locker.append(details);}
    else {const target=buttons.find(b=>b.dataset.key===selected);
      if(target && target.nextElementSibling!==details)target.after(details);
      else if(!target && details.parentNode!==shelf)shelf.append(details);}
    shop.setAttribute('aria-pressed',String(!inBag));bag.setAttribute('aria-pressed',String(inBag));
    shelf.hidden=inBag;locker.hidden=!inBag;details.hidden=!inBag&&!selected;
    for(const b of buttons)b.setAttribute('aria-expanded',String(selected===b.dataset.key));
    for(const section of details.querySelectorAll('.tiny-shop')){
      const toys=section!==fruit&&section!==eggs;
      const match=section===eggs?selected==='egg':section===fruit?rows.some(r=>r.key===selected&&r.group==='fruit'):selected?.startsWith('toy:');
      section.classList.toggle('stock-hidden',inBag?section===fruit:!match);
      for(const card of section.querySelectorAll('[data-stock-key]'))card.classList.toggle('stock-hidden',inBag?toys&&!card.querySelector('button')?.textContent.match(/^(Play|Watch)/):card.dataset.stockKey!==selected);
    }
  }
  shop.onclick=()=>{inBag=false;display();};bag.onclick=()=>{inBag=true;display();};
  return {update(care,player){
    const next=shelfStock(care).map(row=>({...row,group:row.art.startsWith('fruit')?'fruit':'other'}));
    for(const f of care?.tinyShop?.items||[])if(f.itemId==='garden-fruit' && Number.isSafeInteger(f.price)&&f.price>=0)
      next.push({key:f.itemId,name:f.name,price:f.price,group:'fruit'});
    if(owner!==player?.id||!next.some(r=>r.key===selected))selected=null;
    owner=player?.id;wallet.textContent=Number.isSafeInteger(player?.rings)?player.rings.toLocaleString()+' rings':'Offline';
    const sig=JSON.stringify(next);
    if(sig!==signature){signature=sig;rows=next;const focus=buttons.find(b=>b===doc.activeElement)?.dataset.key;
      buttons.forEach(b=>b.remove());buttons=[];for(const r of rows){
        const b=make('button','market-row');b.type='button';b.dataset.key=r.key;
        if(r.art||r.key==='garden-fruit'){const icon=make('img','market-icon');icon.alt='';icon.src=r.key==='garden-fruit'?'/phone-tiny/__tiny-shop/sa2-garden-fruit.png':'/phone-tiny/__tiny-shop/tiny-native-'+r.art+'-a2ne.png';b.append(icon);}
        b.append(make('span','market-name',r.name));const price=make('span','market-price');price.setAttribute('aria-hidden','true');
        for(const d of String(r.price)){const n=make('i','');n.style.backgroundPosition=(Number(d)*100/9)+'% 0';price.append(n);}b.append(price);
        b.setAttribute('aria-label',r.name+' · '+(r.price?r.price+' rings':'Free'));b.onclick=()=>{selected=selected===r.key?null:r.key;display();};
        shelf.append(b);buttons.push(b);
      }if(focus)(buttons.find(b=>b.dataset.key===focus)||shop).focus({preventScroll:true});}
    display();
  }};
}
