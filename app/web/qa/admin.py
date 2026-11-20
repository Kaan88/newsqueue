"""Settings/server/feed editing and failure paths through real browser input."""
from functionality import *

def api(path):
 with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{PORT}'+path,headers={'X-Api-Key':KEY})) as r:return json.load(r)
def until(test):
 for _ in range(50):
  if test():return
  time.sleep(.1)
 raise AssertionError('API did not reach expected state')
def admin(page):
 secret_flag=api('/api/servers')[0]['password_set']
 menu(page,'Tools','Options...');hit(page,'Add');edit(page,'QA local server',1);edit(page,'127.0.0.1',2)
 hit(page,'Use this server');hit(page,'Apply')
 until(lambda:len(api('/api/servers'))==2)
 assert api('/api/servers')[1]['name']=='QA local server' and not api('/api/servers')[1]['enabled']
 assert api('/api/servers')[0]['password_set']==secret_flag
 assert all('password' not in server for server in api('/api/servers'))
 page.wait_for_timeout(350);hit(page,'Remove');hit(page,'Apply');until(lambda:len(api('/api/servers'))==1)
 assert api('/api/servers')[0]['password_set']==secret_flag
 # Failed save keeps the edited draft and allows retry, including OK.
 edit(page,'QA retry preserved',1)
 page.route('**/api/servers',lambda r:r.fulfill(status=503,body='isolated failure') if r.request.method=='POST' else r.continue_())
 hit(page,'OK');page.wait_for_timeout(350)
 assert any(v['label']=='Close dialog' for v in items(page))
 assert any(v['label']=='Apply' and v['enabled'] for v in items(page))
 page.unroute('**/api/servers');hit(page,'Apply')
 until(lambda:api('/api/servers')[0]['name']=='QA retry preserved')
 print('PASS server add/edit/remove, secret preservation, failed save retry',flush=True)
 hit(page,'RSS feeds');hit(page,'Add feed');edit(page,'http://127.0.0.1:18483/qa-empty-feed.xml',1);hit(page,'Apply')
 until(lambda:api('/api/rss')==['http://127.0.0.1:18483/qa-empty-feed.xml'])
 page.wait_for_timeout(350);hit(page,'Remove');hit(page,'Apply');until(lambda:api('/api/rss')==[])
 print('PASS RSS feed add/save/remove',flush=True)
 hit(page,'Scheduler');label=next(v['label'] for v in items(page) if 'green hours' in v['label']);before=state()['settings']['scheduler_enabled'];hit(page,label);hit(page,'Apply');until(lambda:state()['settings']['scheduler_enabled']!=before)
 hit(page,label);hit(page,'OK');until(lambda:state()['settings']['scheduler_enabled']==before)
 print('PASS scheduler setting',flush=True)
 count=len(state()['queue']);menu(page,'File','Add NZB files...')
 with page.expect_file_chooser() as chooser:hit(page,'Browse...')
 with page.expect_response(lambda r:'/api/nzb?' in r.url and r.request.method=='POST') as response:
  chooser.value.set_files({'name':'invalid.nzb','mimeType':'application/x-nzb','buffer':b'not XML'})
 assert response.value.status>=400
 assert len(state()['queue'])==count
 print('PASS malformed NZB rejected without queue changes',flush=True)
 menu(page,'File','Add NZB files...')
 with page.expect_file_chooser() as chooser:hit(page,'Browse...')
 chooser.value.set_files([]);page.wait_for_timeout(250);assert len(state()['queue'])==count
 print('PASS canceled file selection preserves queue',flush=True)

def run():
 with sync_playwright() as pw:
  b=pw.chromium.launch(executable_path=r'C:\Program Files\Google\Chrome\Application\chrome.exe',headless=True,args=['--enable-unsafe-swiftshader'])
  for theme in ['classic','caustica']:
   for bright in ['light','dark']:
    p,log,_=start_server(PORT,OUT/'save-fixed',KEY)
    c=b.new_context(viewport={'width':1280,'height':820},color_scheme=bright)
    c.add_init_script(f"localStorage.setItem('nq-appearance','{theme}');localStorage.setItem('nq-theme','{bright}');localStorage.setItem('nq-key','{KEY}');")
    q=c.new_page()
    try:
     q.goto(f'http://127.0.0.1:{PORT}');q.wait_for_function('window.__nqState?.queue.length > 0');q.wait_for_timeout(250);admin(q)
     print(theme,bright,'ADMIN PASS',flush=True)
    except Exception:capture(q,OUT/'admin',f'FAILED-{theme}-{bright}');raise
    finally:c.close();p.terminate();p.wait();log.close()
  b.close()
if __name__=='__main__':run()
