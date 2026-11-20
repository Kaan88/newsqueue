from headless import *
DEST=OUT/'approved-icons'
def center(page,label,index=-1):
 v=[v for v in items(page) if v['label']==label][index]['clip'];return((v[0]+v[2])/2,(v[1]+v[3])/2)
def exercise(page):
 page.mouse.move(*center(page,'Queue',0));page.mouse.down();page.wait_for_timeout(100)
 page.mouse.move(*center(page,'Rename...'));page.wait_for_timeout(50);page.mouse.up();page.wait_for_timeout(100)
 assert not any(v['label']=='OK' for v in items(page)), 'Disabled item activated'
 close(page)
 page.mouse.move(*center(page,'File',0));page.mouse.down();page.wait_for_timeout(180)
 assert any(v['label']=='Add NZB files...' for v in items(page)), 'Menu did not open on press'
 page.mouse.move(*center(page,'View',0));page.wait_for_timeout(180)
 assert any(v['label']=='Statistics...' for v in items(page)), 'Held hover did not switch menu'
 page.mouse.move(*center(page,'Statistics...'));page.wait_for_timeout(100);page.mouse.up();page.wait_for_timeout(220)
 assert any(v['label']=='Daily transfer' for v in items(page)), 'Drag/release did not select'
 close(page)
 # A short click leaves the menu open; clicking its title again closes it.
 hit(page,'File',0);assert any(v['label']=='Add NZB files...' for v in items(page))
 hit(page,'File',0);assert not any(v['label']=='Add NZB files...' for v in items(page))
 page.mouse.move(*center(page,'Tools',0));page.mouse.down();page.wait_for_timeout(120)
 page.mouse.move(1100,500);page.mouse.up();page.wait_for_timeout(150)
 assert not any(v['label']=='Options...' for v in items(page)), 'Outside release did not cancel'
 page.mouse.move(*center(page,'File',0));page.mouse.down();page.wait_for_timeout(100);page.keyboard.press('Escape');page.mouse.up();page.wait_for_timeout(120)
 assert not any(v['label']=='Add NZB files...' for v in items(page)), 'Escape did not cancel'
def run():
 p,log,snap=start_server(18491,OUT/'icon-preview')
 try:
  with sync_playwright() as pw:
   browser=pw.chromium.launch(executable_path=r'C:\Program Files\Google\Chrome\Application\chrome.exe',headless=True,args=['--enable-unsafe-swiftshader'])
   for theme,variant in [('classic','teal'),('caustica','teal'),('caustica-copper','copper')]:
    for bright in ['light','dark']:
     ctx=browser.new_context(viewport={'width':1280,'height':820},color_scheme=bright)
     ctx.add_init_script(f"localStorage.setItem('nq-appearance','{theme}');localStorage.setItem('nq-theme','{bright}');")
     ctx.route('**/api/events*',lambda r:r.abort());ctx.route('**/api/state',lambda r:r.fulfill(json=snap))
     page=ctx.new_page();errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
     page.goto('http://127.0.0.1:18491');page.wait_for_function('window.__nqAudit?.length>0');page.wait_for_timeout(500)
     exercise(page)
     menu(page,'View','Caustica · Copper');assert page.evaluate("localStorage.getItem('nq-appearance')")=='caustica-copper'
     menu(page,'View','Caustica · Sea Glass');assert page.evaluate("localStorage.getItem('nq-appearance')")=='caustica'
     menu(page,'View',{'classic':'Classic','caustica':'Caustica · Sea Glass','caustica-copper':'Caustica · Copper'}[theme])
     page.reload();page.wait_for_function('window.__nqAudit?.length>0');page.wait_for_timeout(350)
     assert page.evaluate("localStorage.getItem('nq-appearance')")==theme
     assert page.evaluate("localStorage.getItem('nq-theme')")==bright
     hit(page,'job:2');folder=DEST/f'{theme}-{variant}-{bright}'
     capture(page,folder,'queue');menu(page,'Tools','Options...');capture(page,folder,'options');close(page)
     assert not errors,errors
     print(theme,variant,bright,'PASS mouse-down, held menu switching, drag/release selection, click toggle, outside release, Escape',flush=True)
     ctx.close()
   browser.close()
 finally:p.terminate();p.wait();log.close()
if __name__=='__main__':run()
