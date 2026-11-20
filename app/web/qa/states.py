"""Headless edge-state and scrolling checks using explicit synthetic snapshots."""
from headless import *
import copy

def run():
 p,log,snap=start_server(18489,OUT/'final')
 try:
  with sync_playwright() as pw:
   browser=pw.chromium.launch(executable_path=r'C:\Program Files\Google\Chrome\Application\chrome.exe',headless=True,args=['--enable-unsafe-swiftshader'])
   for theme in ['classic','caustica']:
    for bright in ['light','dark']:
     for width,height in [(1280,820),(390,844)]:
      name=f'{theme}-{bright}-{width}';folder=OUT/'states'/name
      ctx=browser.new_context(viewport={'width':width,'height':height},color_scheme=bright)
      ctx.add_init_script(f"localStorage.setItem('nq-appearance','{theme}');localStorage.setItem('nq-theme','{bright}');")
      ctx.route('**/api/events*',lambda r:r.abort())
      current=copy.deepcopy(snap)
      ctx.route('**/api/state',lambda r:r.fulfill(json=current))
      page=ctx.new_page();errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
      page.goto('http://127.0.0.1:18489');page.wait_for_function('window.__nqState?.queue.length > 0')
      for key in ['queue','history','par2','rss','threads','log_tail']:current[key]=[]
      page.wait_for_function('window.__nqState?.queue.length === 0')
      tabs=['Download queue' if width>=720 else 'Queue','History','PAR2','Log','RSS']
      for n,t in enumerate(tabs):hit(page,t);capture(page,folder,f'empty-{n}')
      hit(page,tabs[0]);current=copy.deepcopy(snap)
      current['queue']=[]
      for i in range(500):
       q=copy.deepcopy(snap['queue'][1]);q.update(id=1000+i,subject=f'Queue stress {i:04} '+('Istanbul İşğü 日本語 ' * 15 if i==0 else ''),expanded=False);current['queue'].append(q)
      page.wait_for_function('window.__nqState?.queue.length === 500');page.wait_for_timeout(200)
      capture(page,folder,'large-top')
      before=[v['label'] for v in items(page) if v['label'].startswith('job:')]
      page.mouse.move(width*.5,height*.45);page.mouse.wheel(0,2200);page.wait_for_timeout(350)
      after=[v['label'] for v in items(page) if v['label'].startswith('job:')]
      assert before!=after and after,(name,'scroll stalled')
      capture(page,folder,'large-scrolled')
      page.mouse.wheel(0,-3000);page.wait_for_timeout(250)
      # Focus styling and typing in the queue filter.
      hit(page,'field',0);page.keyboard.insert_text('Queue stress 0000');page.wait_for_timeout(250)
      assert len([v for v in items(page) if v['label'].startswith('job:')])==1
      capture(page,folder,'filter-focused')
      # Simulate a server outage and recovery through the real polling path.
      ctx.unroute('**/api/state');ctx.route('**/api/state',lambda r:r.fulfill(status=503,body='isolated outage'))
      page.wait_for_timeout(1400);capture(page,folder,'offline')
      ctx.unroute('**/api/state');ctx.route('**/api/state',lambda r:r.fulfill(json=current))
      page.wait_for_timeout(1400);capture(page,folder,'reconnected')
      assert not errors,(name,errors)
      print(name,'PASS empty states, 500-row scrolling, long Unicode, filter, outage/recovery',flush=True)
      ctx.close()
   browser.close()
 finally:p.terminate();p.wait();log.close()
if __name__=='__main__':run()
