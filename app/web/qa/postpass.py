from headless import *
def run():
 p,log,snap=start_server(18490,OUT/'save-fixed')
 try:
  with sync_playwright() as pw:
   b=pw.chromium.launch(executable_path=r'C:\Program Files\Google\Chrome\Application\chrome.exe',headless=True,args=['--enable-unsafe-swiftshader'])
   for theme in ['classic','caustica']:
    for bright in ['light','dark']:
     for w,h in [(1280,820),(390,844)]:
      c=b.new_context(viewport={'width':w,'height':h},color_scheme=bright)
      c.add_init_script(f"localStorage.setItem('nq-appearance','{theme}');localStorage.setItem('nq-theme','{bright}');")
      c.route('**/api/events*',lambda r:r.abort());c.route('**/api/state',lambda r:r.fulfill(json=snap))
      pg=c.new_page();pg.goto('http://127.0.0.1:18490');pg.wait_for_function('window.__nqAudit?.length>0');pg.wait_for_timeout(500)
      f=OUT/'postpass'/f'{theme}-{bright}-{w}'
      capture(pg,f,'main');menu(pg,'Tools','Options...');capture(pg,f,'servers')
      if w<720:hit(pg,[v['label'] for v in items(pg) if v['label'].startswith('combo:')][0])
      hit(pg,'Scheduler');capture(pg,f,'scheduler');close(pg)
      menu(pg,'View','Statistics...')
      for label in ['Daily transfer','Monthly transfer']:hit(pg,label);capture(pg,f,label.split()[0])
      close(pg);hit(pg,'Queue',0)
      v=next((v for v in items(pg) if v['label']=='Rename...'),None)
      if v:
       r=v['clip'];pg.mouse.move((r[0]+r[2])/2,(r[1]+r[3])/2);pg.wait_for_timeout(200);pg.screenshot(path=str(f/'disabled-hover.png'))
      print(f.name,'PASS',flush=True);c.close()
   b.close()
 finally:p.terminate();p.wait();log.close()
if __name__=='__main__':run()
