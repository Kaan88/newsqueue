from headless import *
PORT=18501
KEY='qol-isolated'
def center(page,label,index=-1):
 r=[x for x in items(page) if x['label']==label][index]['clip'];return ((r[0]+r[2])/2,(r[1]+r[3])/2)
def state():
 with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{PORT}/api/state',headers={'X-Api-Key':KEY})) as r:return json.load(r)
def waitfor(f):
 for _ in range(60):
  if f():return
  time.sleep(.1)
 raise AssertionError('State update missing')
def run():
 p,log,snap=start_server(PORT,OUT/'qol',KEY)
 try:
  with sync_playwright() as pw:
   b=pw.chromium.launch(executable_path=r'C:\Program Files\Google\Chrome\Application\chrome.exe',headless=True,args=['--enable-unsafe-swiftshader'])
   for theme in ['caustica','caustica-copper','classic']:
    for bright in ['dark','light']:
     c=b.new_context(viewport={'width':1440,'height':960});c.add_init_script(f"localStorage.setItem('nq-key','{KEY}');localStorage.setItem('nq-appearance','{theme}');localStorage.setItem('nq-theme','{bright}');")
     q=c.new_page();errors=[];q.on('pageerror',lambda e:errors.append(str(e)));q.goto(f'http://127.0.0.1:{PORT}');q.wait_for_function('window.__nqAudit?.length>0');q.wait_for_timeout(400)
     x,y=center(q,'Connections divider');q.mouse.move(x,y);q.mouse.down();q.mouse.move(x,y-140,steps=18);q.mouse.up();q.wait_for_timeout(200)
     assert center(q,'Connections divider')[1]<y-100,'divider did not move'
     saved=q.evaluate("localStorage.getItem('nq-conns-height')");assert float(saved)>300
     q.reload();q.wait_for_function('window.__nqAudit?.length>0');q.wait_for_timeout(300);assert abs(center(q,'Connections divider')[1]-(y-140))<5
     hit(q,'Speed');q.mouse.click(*center(q,'spin'));q.keyboard.press('Control+A');q.keyboard.insert_text('22.50');q.keyboard.press('Tab');q.wait_for_timeout(200);hit(q,'Apply');waitfor(lambda:state()['custom_speed_limit_mbs']==22.5)
     hit(q,'Speed');capture(q,OUT/'qol-shots'/f'{theme}-{bright}','speed');hit(q,'No limit');hit(q,'Apply');waitfor(lambda:state()['custom_speed_limit_mbs'] is None)
     hit(q,'Options');q.wait_for_timeout(300)
     def servers():
      with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{PORT}/api/servers',headers={'X-Api-Key':KEY})) as r:return json.load(r)
     before=servers()[0];hit(q,'Secure connection (TLS)');hit(q,'Apply');waitfor(lambda:servers()[0]['ssl']!=before['ssl']);assert servers()[0]['port']==(119 if before['ssl'] else 563)
     q.wait_for_timeout(300);q.mouse.click(*center(q,'spin',0));q.keyboard.press('Control+A');q.keyboard.insert_text('443');q.keyboard.press('Tab');q.wait_for_timeout(200);hit(q,'Secure connection (TLS)');hit(q,'Apply');waitfor(lambda:servers()[0]['port']==443)
     q.wait_for_timeout(300);q.mouse.click(*center(q,'spin',0));q.keyboard.press('Control+A');q.keyboard.insert_text(str(before['port']));q.keyboard.press('Tab');q.wait_for_timeout(200);hit(q,'Apply');waitfor(lambda:servers()[0]['port']==before['port'])
     q.wait_for_timeout(300)
     q.mouse.move(*center(q,'Servers'));q.mouse.down();q.mouse.move(*center(q,'Folders'),steps=8);q.wait_for_timeout(150);assert any(v['label']=='Download folder' for v in items(q)) or any(v['label']=='field' for v in items(q))
     q.mouse.move(*center(q,'Download'),steps=8);q.mouse.up();q.wait_for_timeout(180);assert any(v['label']=='Stop jobs PAR2 cannot repair' for v in items(q));q.mouse.move(*center(q,'Servers'));q.wait_for_timeout(100);assert any(v['label']=='Stop jobs PAR2 cannot repair' for v in items(q));capture(q,OUT/'qol-shots'/f'{theme}-{bright}','settings')
     close(q);q.mouse.move(1100,400);q.mouse.click(1100,400);q.wait_for_timeout(150);assert q.locator('canvas').evaluate('(e)=>getComputedStyle(e).cursor')=='default','non-editable cursor not arrow'
     q.mouse.move(*center(q,'field',0));q.wait_for_timeout(120);assert q.locator('canvas').evaluate('(e)=>getComputedStyle(e).cursor')=='text','edit cursor not I-beam'
     q.mouse.move(1100,400);capture(q,OUT/'qol-shots'/f'{theme}-{bright}','main')
     for width,height in [(760,600),(390,844),(2560,1440)]:
      q.set_viewport_size({'width':width,'height':height});q.wait_for_timeout(200)
      capture(q,OUT/'qol-shots'/f'{theme}-{bright}',f'main-{width}')
      menu(q,'Tools','Options...')
      for label in ['Servers','Folders','Download','Connection','PAR2 & unpack','Scheduler','RSS feeds','Log & history']:
       if width<720:
        combo=next(v['label'] for v in items(q) if v['label'].startswith('combo:'));hit(q,combo);hit(q,label)
       else:hit(q,label)
       capture(q,OUT/'qol-shots'/f'{theme}-{bright}',f'options-{width}-{label.replace("/","-")}')
      close(q)
     assert not errors,errors
     print(theme,bright,'PASS splitter persistence, custom/unlimited API, category drag, cursor',flush=True)
     c.close()
   b.close()
 finally:p.terminate();p.wait();log.close()
if __name__=='__main__':run()
