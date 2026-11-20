"""Real browser input -> egui -> authenticated JSON API verification."""
from headless import *
KEY='isolated-web-qa'
PORT=18483

def state():
 with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{PORT}/api/state',headers={'X-Api-Key':KEY})) as r:return json.load(r)
def check(predicate,description):
 for _ in range(50):
  if predicate(state()):print('PASS',description,flush=True);return
  time.sleep(.1)
 raise AssertionError(description)
def edit(page,text,index=-1,label='field'):
 hit(page,label,index);page.keyboard.press('Control+A');page.keyboard.press('Backspace');page.keyboard.insert_text(text)
def row(page,id,button='left'):
 r=[i['clip'] for i in items(page) if i['label']==f'job:{id}'][-1]
 page.mouse.click(r[0]+80,(r[1]+r[3])/2,button=button);page.wait_for_timeout(200)
def work(page,theme,bright):
 page.wait_for_function('window.__nqAudit?.some(x=>x.label=="OK")')
 edit(page,'wrong-isolated-key');hit(page,'OK');page.wait_for_timeout(400)
 assert any(i['label']=='OK' for i in items(page))
 edit(page,KEY);hit(page,'OK');page.wait_for_function('window.__nqAudit?.some(x=>x.label=="Download queue")')
 print('PASS authentication reject/accept',theme,bright,flush=True)
 page.keyboard.press('F1');check(lambda s:s['paused'],'global pause')
 page.keyboard.press('F1');check(lambda s:not s['paused'],'global resume')
 row(page,2,'right');hit(page,'Pause');check(lambda s:next(q for q in s['queue'] if q['id']==2)['status']=='Paused','context pause')
 row(page,2,'right');hit(page,'Resume');check(lambda s:next(q for q in s['queue'] if q['id']==2)['status']!='Paused','context resume')
 row(page,2);page.keyboard.press('F2');page.wait_for_timeout(200);edit(page,'QA renamed collection');hit(page,'OK')
 check(lambda s:next(q for q in s['queue'] if q['id']==2)['subject']=='QA renamed collection','rename')
 row(page,2);hit(page,'Top');check(lambda s:s['queue'][0]['id']==2,'move to top')
 hit(page,'Down');check(lambda s:s['queue'][1]['id']==2,'move down')
 menu(page,'Queue','Archive password...');edit(page,'synthetic-password');hit(page,'OK');check(lambda s:next(q for q in s['queue'] if q['id']==2)['has_password'],'archive password')
 menu(page,'View','Collapse all');check(lambda s:all(not q['expanded'] for q in s['queue']),'collapse all')
 menu(page,'View','Expand all');check(lambda s:all(q['expanded'] for q in s['queue']),'expand all')
 edit(page,'QA renamed',0);page.wait_for_timeout(300);assert len([i for i in items(page) if i['label'].startswith('job:')])==1
 edit(page,'',0);page.keyboard.press('Tab');print('PASS queue filter',flush=True)
 menu(page,'Queue','Speed limit...');label=next(i['label'] for i in items(page) if i['label'].startswith('50 %'));hit(page,label);hit(page,'OK');check(lambda s:s['speed_limit_tag']==5,'speed limit')
 menu(page,'Queue','Pause for...');hit(page,'5 min');check(lambda s:s['paused'],'timed pause');page.wait_for_function('window.__nqState?.paused === true');page.keyboard.press('F1');check(lambda s:not s['paused'],'resume timed pause')
 menu(page,'Tools','Options...');hit(page,'Connection');before=state()['settings']['proxy_enabled'];hit(page,'Use a proxy');hit(page,'Cancel');assert state()['settings']['proxy_enabled']==before
 menu(page,'Tools','Options...');hit(page,'Connection');hit(page,'Use a proxy');hit(page,'Apply');check(lambda s:s['settings']['proxy_enabled']!=before,'settings apply')
 hit(page,'Use a proxy');hit(page,'OK');check(lambda s:s['settings']['proxy_enabled']==before,'settings restore')
 menu(page,'File','Add NZB files...');hit(page,'Add them paused')
 xml=b'<?xml version="1.0"?><nzb><file poster="qa@example.invalid" date="1795132800" subject="&quot;qa.bin&quot; yEnc (1/1)"><groups><group>alt.test</group></groups><segments><segment bytes="32" number="1">qa@example.invalid</segment></segments></file></nzb>'
 with page.expect_file_chooser() as choice:hit(page,'Browse...')
 choice.value.set_files([{'name':'qa-one.nzb','mimeType':'application/x-nzb','buffer':xml},{'name':'qa-two.nzb','mimeType':'application/x-nzb','buffer':xml}])
 check(lambda s:len(s['queue'])==5 and all(q['status']=='Paused' for q in s['queue'][-2:]),'multiselect paused NZB import')
 page.wait_for_timeout(400);new_id=state()['queue'][-1]['id'];row(page,new_id);page.keyboard.press('Delete');page.wait_for_timeout(200);hit(page,'No');assert len(state()['queue'])==5
 row(page,new_id);page.keyboard.press('Delete');page.wait_for_timeout(200);hit(page,'Yes');check(lambda s:len(s['queue'])==4,'delete confirmation/cancel')
 # Theme changes must preserve the active queue, selection and independent brightness.
 other='Classic' if theme=='caustica' else 'Caustica'
 menu(page,'View',other);page.wait_for_timeout(300);assert page.evaluate("localStorage.getItem('nq-theme')")==bright
 assert page.evaluate("localStorage.getItem('nq-appearance')")==other.lower()
 page.reload();page.wait_for_function('window.__nqAudit?.some(x=>x.label=="Download queue")');assert len(state()['queue'])==4
 menu(page,'View','System colors');assert page.evaluate("localStorage.getItem('nq-theme')")=='system'
 page.emulate_media(color_scheme='dark');page.wait_for_timeout(200);capture(page,OUT/'function',f'{theme}-{bright}-system-dark')
 page.emulate_media(color_scheme='light');page.wait_for_timeout(300);capture(page,OUT/'function',f'{theme}-{bright}-system-light')
 menu(page,'File','Sign out');page.wait_for_function('window.__nqAudit?.some(x=>x.label=="OK")');print('PASS theme/brightness/reload/signout',flush=True)

def run():
 with sync_playwright() as pw:
  b=pw.chromium.launch(executable_path=r'C:\Program Files\Google\Chrome\Application\chrome.exe',headless=True,args=['--enable-unsafe-swiftshader','--disable-background-networking'])
  for theme in ['classic','caustica']:
   for bright in ['light','dark']:
    p,log,_=start_server(PORT,OUT/'save-fixed',KEY)
    c=b.new_context(viewport={'width':1280,'height':820},color_scheme=bright)
    c.add_init_script(f"if(!localStorage.getItem('nq-appearance'))localStorage.setItem('nq-appearance','{theme}');if(!localStorage.getItem('nq-theme'))localStorage.setItem('nq-theme','{bright}');")
    page=c.new_page();errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
    try:
     page.goto(f'http://127.0.0.1:{PORT}');work(page,theme,bright);assert not errors,errors
    except Exception:
     capture(page,OUT/'function',f'FAILED-{theme}-{bright}');raise
    finally:c.close();p.terminate();p.wait();log.close()
  b.close()
if __name__=='__main__':run()
