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
// Core observations, independent of measurements.
// Read-only core assertions against the committed plan and the real Canvas2D raster.
// Scratch font probes do not change product state or Canvas pixels.
(() => {
  window.__rendererCoreCheck = spec => {
    const pane=spec.selector?.match(/data-pane\s*=\s*["']?([0-3])/ )?.[1];
    const grid=document.querySelector(pane===undefined?'[data-dnacalc=grid][data-grid-active=true]':`[data-dnacalc=grid][data-pane="${pane}"]`);
    if(grid?.dataset.painter!=='canvas')return {handled:false};
    const plans=Object.values(window.__dnacalcRenderPlans||{}).map(f=>JSON.parse(f())).filter(p=>(pane===undefined?p.active:String(p.pane)===pane)&&p.plan?.publication&&!p.plan.interrupted);
    const plan=plans[0]?.plan, canvas=grid.querySelector('canvas');
    if(!plan||!canvas)return {handled:true,seen:null};
    const address=spec.address||spec.selector?.match(/data-cell\s*=\s*["']?([A-Z]+[0-9]+)/)?.[1];
    const cell=plan.cells.find(c=>c.address===address);
    const target=spec.selector?.includes('cell-decoration')?!!cell&&(cell.row_span>1||cell.col_span>1||!!cell.shown.style.fill||!!cell.shown.style.brush||!!cell.shown.conditional?.fill||!!cell.shown.conditional?.bar):spec.selector?.includes('cell-borders')?!!cell&&(cell.borders||cell.shown.style.borders).some(Boolean)||!!cell?.boundary?.length||cell?.shown.style.diagonal?.some(Boolean):!!cell;
    if(spec.op==='empty')return {handled:true,seen:plan.cells.length===0};
    if(spec.op==='count')return {handled:true,seen:address?(target?1:0):plan.cells.length};
    if(spec.op==='absent')return {handled:true,seen:!target};
    if(!target)return {handled:true,seen:null};
    if(canvas.dataset.drawRevision!==String(plan.publication.next)||canvas.dataset.drawPresentation!==String(plan.presentation.epoch))return {handled:true,seen:null};
    if(spec.op==='rect'||spec.op==='visible'){
      const r=cell.anchor,c=cell.clip||r,b=canvas.getBoundingClientRect();
      const x=Math.max(r.x,c.x,0),y=Math.max(r.y,c.y,0),right=Math.min(r.x+r.w,c.x+c.w,plan.viewport.width),bottom=Math.min(r.y+r.h,c.y+c.h,plan.viewport.height);
      const point=[b.left+(x+right)/2,b.top+(y+bottom)/2],hit=right>x&&bottom>y&&document.elementFromPoint(...point)?.closest('[data-dnacalc=grid]')===grid;
      const rect={left:b.left+r.x,top:b.top+r.y,right:b.left+r.x+r.w,bottom:b.top+r.y+r.h,width:r.w,height:r.h,visible_point:point,reachable:hit};
      return {handled:true,seen:spec.op==='rect'?rect:hit&&rect.left>=0&&rect.top>=0&&rect.right<=innerWidth&&rect.bottom<=innerHeight};
    }
    const s={...cell.shown.style,pixels:cell.paint_pixels??cell.shown.style.pixels}, cf=cell.shown.conditional||{}, scale=plan.scale, rect=cell.rect, anchor=cell.anchor;
    const css=getComputedStyle(grid), token=n=>css.getPropertyValue(n).trim();
    const scratch=document.createElement('canvas'), context=scratch.getContext('2d');scratch.width=4;scratch.height=4;
    const family=s.family?(/^(system-ui|sans-serif|serif|monospace)$/i.test(s.family)?s.family:JSON.stringify(s.family)+',system-ui'):'system-ui';
    const italic=s.italic||cf.italic||['file-cache','refused'].includes(cell.shown.provenance), bold=s.bold||cf.bold;
    const font=`${italic?'italic':'normal'} ${bold?700:400} ${s.pixels*scale}px ${family}`;
    context.font=font;
    const measure=t=>{
      const raw=context.measureText(t), integer=cell.shown.kind==='number'&&['applied','unshowable'].includes(cell.shown.format)&&!s.shrink&&!s.wrap&&s.rotation===0&&!['fill','justify','distributed','centerContinuous'].includes(s.horizontal)&&!/[A-Za-z\n\u0080-\uffff]/.test(t);
      if(!integer)return raw;
      let pen=0,left=Infinity,right=-Infinity,ascent=0,descent=0;
      for(const part of t){const m=context.measureText(part);left=Math.min(left,pen-m.actualBoundingBoxLeft);right=Math.max(right,pen+m.actualBoundingBoxRight);ascent=Math.max(ascent,m.actualBoundingBoxAscent);descent=Math.max(descent,m.actualBoundingBoxDescent);pen+=Math.round(m.width)}
      return {width:pen,actualBoundingBoxLeft:-left,actualBoundingBoxRight:right,actualBoundingBoxAscent:ascent,actualBoundingBoxDescent:descent,fontBoundingBoxAscent:raw.fontBoundingBoxAscent,fontBoundingBoxDescent:raw.fontBoundingBoxDescent};
    };
    const rgb=color=>{context.clearRect(0,0,4,4);context.fillStyle=color;context.fillRect(0,0,4,4);return [...context.getImageData(0,0,1,1).data].slice(0,3);};
    const foreground=cf.font_color||cell.shown.color||s.font_color||token(cell.shown.provenance==='file-cache'?'--dna-ink-3':cell.shown.kind==='error'?'--dna-red-ink':'--dna-ink');
    const background=cf.fill||s.fill||token(cell.shown.provenance==='stale'?'--dna-amber-soft':'--dna-paper');
    const fg=rgb(foreground), bg=rgb(background), d=devicePixelRatio, surface=canvas.getContext('2d');
    const distance=(a,b)=>a.reduce((v,n,i)=>v+Math.abs(n-b[i]),0);
    const crop=(r,inset=0,border=false)=>{
      const clip=(border&&cell.row_span===1&&cell.col_span===1?null:cell.clip)||{x:0,y:0,w:plan.viewport.width,h:plan.viewport.height};
      const x=Math.max(0,Math.ceil(Math.max(r.x+inset,clip.x)*d)),y=Math.max(0,Math.ceil(Math.max(r.y+inset,clip.y)*d));
      const right=Math.min(canvas.width,Math.floor(Math.min(r.x+r.w-inset,clip.x+clip.w)*d)),bottom=Math.min(canvas.height,Math.floor(Math.min(r.y+r.h-inset,clip.y+clip.h)*d));
      if(right<=x||bottom<=y)return null;
      return {x,y,w:right-x,h:bottom-y,pixels:surface.getImageData(x,y,right-x,bottom-y).data};
    };
    const raster=crop(rect,Math.min(2*scale,rect.h/8));
    const mask=()=>{
      if(!raster)return null;let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity;const rows=[];
      for(let y=0;y<raster.h;y++)for(let x=0;x<raster.w;x++){
        const i=(y*raster.w+x)*4,p=[raster.pixels[i],raster.pixels[i+1],raster.pixels[i+2]];
        if(raster.pixels[i+3]>127&&distance(p,fg)<distance(p,bg)&&distance(p,bg)>24){x0=Math.min(x0,x);x1=Math.max(x1,x+1);y0=Math.min(y0,y);y1=Math.max(y1,y+1);rows[y]=true;}
      }
      if(!Number.isFinite(x0))return null;
      const bands=[];for(let y=0;y<rows.length;y++)if(rows[y]){const start=y;while(rows[y+1])y++;bands.push({top:(raster.y+start)/d,bottom:(raster.y+y+1)/d});}
      return {left:(raster.x+x0)/d,right:(raster.x+x1)/d,top:(raster.y+y0)/d,bottom:(raster.y+y1)/d,bands};
    };
    const ink=mask();let painted=cell.shown.text===''||!!ink;
    const hasColour=(colour,r,border=false)=>{const data=crop(r,0,border);if(!data)return false;const want=rgb(colour);for(let i=0;i<data.pixels.length;i+=4)if(data.pixels[i+3]>127&&distance([data.pixels[i],data.pixels[i+1],data.pixels[i+2]],want)<36)return true;return false;};
    const horizontal=s.horizontal&&s.horizontal!=='general'?s.horizontal:cell.shown.kind==='number'?'right':cell.shown.kind==='logical'?'center':'left';
    const vertical=s.vertical||'bottom', leftPad=2*scale, rightPad=2*scale+1, icon=cf.icon?20*scale:0;
    context.font=`normal 400 ${13*scale}px system-ui`;const indent=context.measureText('   ').width*s.indent;context.font=font;
    const left=leftPad+icon+(['left','distributed'].includes(horizontal)?indent:0),right=rightPad+(['right','distributed'].includes(horizontal)?indent:0), capacity=rect.w-left-right;
    const unit=cell.shown.text, repeat=horizontal==='fill'&&unit?Math.max(1,Math.floor(capacity/measure(unit).width)):1, text=unit.repeat(Math.min(repeat,16384/Math.max(1,unit.length)));
    const inset=s.rotation===0&&!s.wrap&&!s.shrink&&!['justify','distributed'].includes(horizontal)&&!unit.includes('\n')?scale:0;
    if(horizontal==='fill'&&unit&&repeat>1)painted=painted&&!!ink&&Math.abs(ink.right-(rect.x+left+measure(text).actualBoundingBoxRight))<3;
    // Canvas has no text span. Check the complete measured line box and its actual glyph
    // raster instead. Unsupported multi-line cases must fail, never imply that clipped ink fits.
    const textBounds=()=>{
      if(!painted||!ink||s.rotation===255||['justify','distributed'].includes(horizontal)||(unit.includes('\n')&&!s.wrap))return false;
      if(s.wrap){
        if(s.rotation!==0||capacity<=0)return false;
        // Independently measure the browser's wrapped line boxes, then require an actual
        // ink band in each box. Comparing clipped ink alone would incorrectly report a fit.
        const probe=document.createElement('span');
        probe.style.cssText=`position:fixed;left:0;top:-10000px;visibility:hidden;pointer-events:none;display:block;padding:0;margin:0;border:0;box-sizing:content-box;font:${font};line-height:1;width:${capacity}px;white-space:pre-wrap;overflow-wrap:anywhere`;
        probe.textContent=unit;document.body.append(probe);
        let count;
        try{const range=document.createRange();range.selectNodeContents(probe);count=new Set([...range.getClientRects()].map(r=>r.top.toFixed(3))).size}finally{probe.remove()}
        const lh=s.pixels*scale,height=count*lh,x=rect.x+left,y=vertical==='top'?rect.y:vertical==='center'?rect.y+(rect.h-height)/2:rect.y+rect.h-height,c=cell.clip||rect;
        if(!count||ink.bands.length!==count||x<anchor.x-1||x+capacity>anchor.x+anchor.w+1||y<anchor.y-1||y+height>anchor.y+anchor.h+1||x<c.x-1||x+capacity>c.x+c.w+1||y<c.y-1||y+height>c.y+c.h+1)return false;
        return ink.left>=x-1&&ink.right<=x+capacity+1&&ink.bands.every((b,i)=>b.top>=y+i*lh-1&&b.bottom<=y+(i+1)*lh+1);
      }
      const metrics=measure(unit),lh=s.pixels*scale,box=measure('Mg');
      const baseline=(lh-box.fontBoundingBoxAscent-box.fontBoundingBoxDescent)/2+box.fontBoundingBoxAscent;
      const angle=s.rotation*Math.PI/180,cos=Math.cos(angle),sin=Math.sin(angle);
      const rw=Math.abs(cos)*metrics.width+Math.abs(sin)*lh,rh=Math.abs(sin)*metrics.width+Math.abs(cos)*lh;
      const x=horizontal==='right'?rect.x+rect.w-right-rw:horizontal==='center'||horizontal==='centerContinuous'?anchor.x+(anchor.w-rw)/2:rect.x+left;
      const shift=s.rotation===0?((box.fontBoundingBoxAscent+box.fontBoundingBoxDescent-lh)/2)*(vertical==='top'?1:vertical==='bottom'?-1:0):0;
      const y=(vertical==='top'?rect.y+inset:vertical==='center'?rect.y+(rect.h-rh)/2:rect.y+rect.h-rh-(vertical==='bottom'?inset+(cell.baseline_inset||0):0))+shift;
      const clip=cell.clip||rect;
      if(x<rect.x-1||x+rw>rect.x+rect.w+1||y<rect.y-1||y+rh>rect.y+rect.h+1||x<clip.x-1||x+rw>clip.x+clip.w+1||y<clip.y-1||y+rh>clip.y+clip.h+1)return false;
      if(x<anchor.x-1||x+rw>anchor.x+anchor.w+1||y<anchor.y-1||y+rh>anchor.y+anchor.h+1)return false;
      const xs=[-metrics.width/2-metrics.actualBoundingBoxLeft,-metrics.width/2+metrics.actualBoundingBoxRight];
      const ys=[-lh/2+baseline-metrics.actualBoundingBoxAscent,-lh/2+baseline+metrics.actualBoundingBoxDescent];
      const corners=xs.flatMap(a=>ys.map(b=>[x+rw/2+a*cos-b*sin,y+rh/2+a*sin+b*cos]));
      const expected={left:Math.min(...corners.map(p=>p[0])),right:Math.max(...corners.map(p=>p[0])),top:Math.min(...corners.map(p=>p[1])),bottom:Math.max(...corners.map(p=>p[1]))};
      return Object.keys(expected).every(k=>Math.abs(ink[k]-expected[k])<3);
    };
    if(spec.op==='bounds')return {handled:true,seen:textBounds()?true:{unfitted:true,address,anchor,rect,clip:cell.clip,ink,rotation:s.rotation,wrap:s.wrap,horizontal}};
    if(spec.op==='text')return {handled:true,seen:painted?text:{unpainted:true,address,rect,viewport:plan.viewport,clip:cell.clip,ink,foreground,background}};
    if(spec.op==='fits')return {handled:true,seen:painted?[measure(text).width,capacity]:null};
    if(spec.op==='hash')return {handled:true,seen:painted&&/^#+$/.test(unit)&&measure(unit).width<=capacity+.1&&measure(unit+'#').width>capacity+.1};
    if(spec.op==='attribute'){
      if(spec.field==='data-icon'){
        const value=cf.icon?.join(':');const visible=[...grid.querySelectorAll('.dc-canvas-icon svg')].some(e=>{const p=e.closest('.dc-canvas-icon'),r=p.getBoundingClientRect(),g=grid.getBoundingClientRect();return Math.abs(r.left-g.left-rect.x)<3&&Math.abs(r.top-g.top-rect.y)<3&&e.getBoundingClientRect().width>0;});
        return {handled:true,seen:visible?value:null};
      }
      const values={'data-kind':cell.shown.kind,'data-provenance':cell.shown.provenance,'data-format':cell.shown.format,'data-formula':String(cell.shown.formula)};
      return {handled:true,seen:painted?values[spec.field]:null};
    }
    if(spec.op==='spill'){
      const first=plan.col_headers.find(h=>h.label===spec.first),last=plan.col_headers.find(h=>h.label===spec.last);
      const ok=painted&&!!first&&!!last&&Math.abs(rect.x-first.rect.x)<1&&Math.abs(rect.x+rect.w-last.rect.x-last.rect.w)<1;
      return {handled:true,seen:ok?true:{painted,address,rect,viewport:plan.viewport,clip:cell.clip,ink,first,last,foreground,background}};
    }
    if(spec.op==='aligned'){
      if(!ink)return {handled:true,seen:false};
      const h=spec.horizontal,v=spec.vertical,metrics=measure(unit),advance=metrics.width;
      let x=h==='right'?rect.x+rect.w-right-advance:h==='center'||h.startsWith('across:')?anchor.x+anchor.w/2-advance/2:rect.x+left;
      const clip=cell.clip||rect;
      const expectedLeft=Math.max(x-metrics.actualBoundingBoxLeft,rect.x+2*scale,clip.x),expectedRight=Math.min(x+metrics.actualBoundingBoxRight,rect.x+rect.w-2*scale,clip.x+clip.w);
      let ok=painted;
      if(h==='spread')ok=ok&&Math.abs(ink.left-(rect.x+left))<4&&Math.abs(ink.right-(rect.x+rect.w-right))<4;
      else if(h==='left')ok=ok&&Math.abs(ink.left-expectedLeft)<4;
      else if(h==='right')ok=ok&&Math.abs(ink.right-expectedRight)<4;
      else ok=ok&&Math.abs(ink.left-expectedLeft)<4&&Math.abs(ink.right-expectedRight)<4;
      const box=measure('Mg'),lh=s.pixels*scale,baseline=(lh-box.fontBoundingBoxAscent-box.fontBoundingBoxDescent)/2+box.fontBoundingBoxAscent;
      const shift=!s.wrap&&!['justify','distributed'].includes(horizontal)&&s.rotation===0&&!unit.includes('\n')?((box.fontBoundingBoxAscent+box.fontBoundingBoxDescent-lh)/2)*(v==='top'?1:v==='bottom'?-1:0):0;
      const topAt=y=>y+shift+baseline-metrics.actualBoundingBoxAscent,bottomAt=y=>y+shift+baseline+metrics.actualBoundingBoxDescent;
      const visibleTop=raster.y/d,visibleBottom=(raster.y+raster.h)/d;
      if(v==='top')ok=ok&&Math.abs(ink.top-Math.max(visibleTop,topAt(rect.y+inset)))<3;
      if(v==='bottom')ok=ok&&Math.abs(ink.bottom-Math.min(visibleBottom,bottomAt(rect.y+rect.h-lh-inset-(cell.baseline_inset||0))))<3;
      if(v==='center')ok=ok&&Math.abs(ink.top-Math.max(visibleTop,topAt(rect.y+(rect.h-lh)/2)))<3&&Math.abs(ink.bottom-Math.min(visibleBottom,bottomAt(rect.y+(rect.h-lh)/2)))<3;
      if(v==='justify'||v==='distributed'){
        const bands=ink.bands,n=bands.length;ok=ok&&n>1;
        if(n>1){
          const positions=bands.map(b=>b.bottom-metrics.actualBoundingBoxDescent-baseline);
          const first=v==='justify'?rect.y:rect.y+(rect.h-n*lh)/(2*n),last=v==='justify'?rect.y+rect.h-lh:first+(n-1)*rect.h/n;
          ok=ok&&Math.abs(positions[0]-first)<3&&Math.abs(positions.at(-1)-last)<3;
          const step=(positions.at(-1)-positions[0])/(n-1);ok=ok&&positions.every((y,i)=>Math.abs(y-positions[0]-step*i)<3);
        }
      }
      return {handled:true,seen:spec.debug?{ok,ink,expectedLeft,expectedRight,x,rect,anchor,metrics:{width:metrics.width,actualBoundingBoxLeft:metrics.actualBoundingBoxLeft,actualBoundingBoxRight:metrics.actualBoundingBoxRight,actualBoundingBoxAscent:metrics.actualBoundingBoxAscent,actualBoundingBoxDescent:metrics.actualBoundingBoxDescent},lh,baseline}:ok};
    }
    if(spec.op==='style'){
      const physical=(spec.selector.includes('cell-decoration')||spec.selector.includes('cell-borders')),r=physical?anchor:rect;
      const line=style=>style==='medium'?[2,'solid']:['mediumDashed','mediumDashDot','mediumDashDotDot'].includes(style)?[2,'dashed']:style==='thick'?[3,'solid']:style==='double'?[3,'double']:style==='hair'?[.5,'solid']:style==='dotted'?[1,'dotted']:style==='dashed'||style?.includes('ashDot')?[1,'dashed']:[1,'solid'];
      const values={left:r.x+'px',top:r.y+'px',width:r.w+'px',height:r.h+'px',color:foreground,'background-color':cf.fill||s.fill||'','font-family':family,'font-weight':bold?'700':'400','font-style':italic?'italic':'normal','font-size':s.pixels*scale+'px','text-align':horizontal==='justify'?'justify':horizontal==='distributed'?'justify':horizontal,'text-align-last':horizontal==='justify'?'left':horizontal==='distributed'?'justify':'auto'};
      // Equivalent Canvas whitespace treatment additionally requires the full glyph raster.
      values['white-space']=(s.wrap||['justify','distributed'].includes(horizontal))&&s.rotation===0?'pre-wrap':'pre';
      const norm=(k,v)=>{const e=document.createElement('span');e.style.setProperty(k,v);return e.style.getPropertyValue(k);};
      let rasterOK=painted;
      const pairs=spec.declarations.split(';').filter(Boolean).map(d=>{
        const i=d.indexOf(':'),key=d.slice(0,i).trim(),expected=d.slice(i+1).trim();let actual=values[key];
        if(key==='color')rasterOK=rasterOK&&hasColour(foreground,rect);
        if(key==='background-color')rasterOK=rasterOK&&!!actual&&hasColour(actual,anchor);
        if(key==='white-space')rasterOK=rasterOK&&textBounds();
        const index=['border-left','border-right','border-top','border-bottom'].indexOf(key);
        if(index>=0){const edge=(cell.borders||s.borders)[index];if(edge){const [w,kind]=line(edge.style),colour=edge.color||'#000000';actual=`${w}px ${kind} ${colour}`;const outside=cell.row_span===1&&cell.col_span===1;const band=index<2?{x:index===0?anchor.x-(outside?w:0):anchor.x+anchor.w-w,y:anchor.y,w,h:anchor.h}:{x:anchor.x,y:index===2?anchor.y-(outside?w:0):anchor.y+anchor.h-w,w:anchor.w,h:w};rasterOK=rasterOK&&hasColour(colour,band,true);}}
        return [key,actual===undefined?'':norm(key,actual),norm(key,expected)];
      });
      return {handled:true,seen:rasterOK?pairs:{unpainted:true,address,anchor,rect,clip:cell.clip,ink,pairs}};
    }
    return {handled:false};
  };
})();

return true;
