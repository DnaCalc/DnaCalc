// Instrument the real UI/worker route. No values are injected and no evaluator is replaced.
// rAF intervals are paint opportunities in a headless browser, not presentation timestamps.
(() => {
  const grid = () => document.querySelector('[data-dnacalc=grid][data-grid-active=true]');
  const plans = () => Object.values(window.__dnacalcRenderPlans || {}).map(f => JSON.parse(f())).filter(p => p.active);
  const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
  const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
  const ready = async test => {
    const end = performance.now() + 30000;
    while (!test()) { if (performance.now() > end) throw Error('Renderer measurement did not settle: '+JSON.stringify({zoom:document.querySelector('[data-dnacalc=zoom-percent]')?.value,plans:plans().map(p=>({pending:p.pending,scale:p.plan.scale,interrupted:p.plan.interrupted}))})); await frame(); }
    await frame(); await frame();
  };
  // Bound the wait to the real committed surface/status. Observe only these small metadata
  // nodes, disconnect before the frame, and timestamp the first task after that frame.
  const waitPaint = test => new Promise((resolve,reject) => {
    let completed=false;
    const observer=new MutationObserver(check);
    function cleanup(){observer.disconnect();clearTimeout(timer);}
    function check(){
      if(completed || !test())return;
      completed=true;cleanup();requestAnimationFrame(()=>setTimeout(resolve,0));
    }
    const timer=setTimeout(()=>{cleanup();reject(Error('Renderer publication did not paint within 30 seconds'));},30000);
    observer.observe(grid(),{attributes:true,attributeFilter:['data-paint-workbook','data-paint-revision']});
    observer.observe(document.querySelector('[data-dnacalc=status]'),{childList:true,subtree:true,characterData:true});
    check();
  });
  const observation = () => {
    const p = plans()[0];
    return {commands:p.commands, publication:p.plan.publication, presentation:p.plan.presentation,
      cells:p.plan.cells.length, semantic:p.plan.semantic_rows.reduce((n,r) => n+r.cells.length,0),
      formula_provenance:p.plan.cells.filter(c=>c.shown.formula).reduce((counts,c)=>{counts[c.shown.provenance]=(counts[c.shown.provenance]||0)+1;return counts;},{}),
      viewport:p.plan.viewport, zoom:p.plan.scale};
  };
  const sample = () => {
    const g=grid(), c=g.querySelector('canvas');
    return {js_heap:performance.memory?.usedJSHeapSize ?? null, nodes:document.querySelectorAll('*').length,
      grid_nodes:g.querySelectorAll('*').length, canvas_bytes:c.width*c.height*4,
      painter:g.dataset.painter, dpr:devicePixelRatio, observation:observation()};
  };
  const percentile = (xs,p) => [...xs].sort((a,b)=>a-b)[Math.min(xs.length-1,Math.floor(xs.length*p))];
  const m = window.__rendererMeasurement = {version:1, browser:navigator.userAgent,
    trial:window.__rendererTrial, viewport:[innerWidth,innerHeight], edits:[], first_paint_ms:null};
  m.wait = ready;
  document.querySelector('[data-dnacalc=open-file]').addEventListener('change', () => {
    const t=performance.now(), before=grid()?.dataset.paintWorkbook;
    m.openPromise=(async()=>{
      await waitPaint(()=>grid()?.dataset.paintWorkbook && grid().dataset.paintWorkbook!==before
        && document.querySelector('[data-dnacalc=status]').textContent.startsWith('Opened '));
      m.first_paint_ms=performance.now()-t;
      m.open=sample();
    })();
  },{once:true,capture:true});
  document.addEventListener('keydown', e => {
    if(e.key!=='Enter' || !e.target.matches('[data-onecalc=formula],[data-dnacalc=cell-editor]')) return;
    const t=performance.now(), before=grid()?.dataset.paintRevision;
    m.editPromise=(async()=>{
      await waitPaint(()=>grid()?.dataset.paintRevision!==before);
      m.edits.push(performance.now()-t);
    })();
  },true);
  m.afterOpen=async()=>{await m.openPromise;};
  m.finish=async()=>{
    await m.editPromise;
    await document.fonts.ready;
    await ready(()=>plans()[0]?.pending===0 && !document.querySelector('[data-dnacalc=status]').textContent.startsWith('Calculating '));
    m.status_before_scroll=document.querySelector('[data-dnacalc=status]').textContent;
    m.before_scroll=sample();
    const g=grid(), original=[g.scrollLeft,g.scrollTop];
    // Move within the used rows when possible; a small workbook inevitably measures blanks
    // too. The report retains cell counts so this cannot look like a dense-grid benchmark.
    const p=plans()[0].plan, row=p.row_headers[0], rowHeight=row?.rect.h||22;
    const usedMax=Math.max(1,...p.cells.map(c=>c.row));
    const scale=(g.scrollHeight-g.clientHeight)/Math.max(1,p.extent_rows*rowHeight-g.clientHeight);
    const distance=Math.min(900,Math.max(80,usedMax*rowHeight/2))*scale;
    m.scroll_physical_px=distance;
    m.scroll=[];
    for(let sweep=0;sweep<3;sweep++){
      const times=[], counts=[];let previous=await frame();
      for(let i=0;i<180;i++){
        g.scrollTop=original[1]+distance*(i<90?i/90:(180-i)/90);
        const now=await frame();times.push(now-previous);previous=now;
        // Read counts outside the timed loop below; serializing a plan per frame would
        // benchmark inspection and allocate another copy of the display list.
      }
      const nominal=1000/60;
      m.scroll.push({frames:times.length,p50_ms:percentile(times,.5),p95_ms:percentile(times,.95),
        max_ms:Math.max(...times),missed_60hz:times.reduce((n,t)=>n+Math.max(0,Math.round(t/nominal)-1),0),
        slow_intervals:times.filter(t=>t>nominal*1.5).length,intervals_ms:times});
    }
    g.scrollTo(...original); await frame();await frame();await sleep(100);
    m.after_scroll=sample();
    const active=document.getElementById(g.getAttribute('aria-activedescendant'));
    m.accessibility={grid_role:g.getAttribute('role'),rowcount:g.getAttribute('aria-rowcount'),colcount:g.getAttribute('aria-colcount'),
      rows:g.querySelectorAll('[role=row]').length,cells:g.querySelectorAll('[role=gridcell]').length,
      active:active?.getAttribute('aria-label'),active_exists:!!active,
      painted_aria_hidden:[...g.querySelectorAll('.dc-cell,canvas')].every(e=>e.getAttribute('aria-hidden')==='true')};
    return m;
  };
})();
return true;
