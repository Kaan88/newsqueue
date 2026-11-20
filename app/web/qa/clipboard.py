import sys,json,copy
from pathlib import Path
r=Path(__file__).resolve().parents[3];sys.path.insert(0,str(Path(__file__).resolve().parent))
from headless import *
p,log,snap=start_server(18503,ROOT/'app/target/copy-web-audit','copy-test')
snap['queue'][0]['expanded']=True
snap['queue'][0]['file_rows']=[{'name':'Example volume.part01.rar','bytes':10000000,'progress':.3,'held':False,'done':False}]
snap['queue'][0]['unrar']=False
snap['history']=[{'name':'Finished example','status':'Completed','size':'10 MB','date':'Today','source':''}]
snap['par2']=[{'name':'Repair example','size_mb':1.,'blocks':2,'info':'Verified','unrar':False,'progress':1.,'status':'Completed'}]
snap['log_tail']=[{'time':'12:00','level':'Info','message':'Example log message'}]
snap['rss']=[{'name':'Feed example','date':'Today','description':'Example feed item'}]
snap['threads']=[{'number':1,'action':'Article','description':'Example article','status':'Waiting','server':'news.example:563'}]
def click(q,label,button='left'):
 x=next(x for x in items(q) if x['label']==label and x['enabled']);a=x['clip'];q.mouse.click((a[0]+a[2])/2,(a[1]+a[3])/2,button=button);q.wait_for_timeout(180)
def clip(q):return q.evaluate('navigator.clipboard.readText()')
try:
 with sync_playwright() as pw:
  b=pw.chromium.launch(executable_path=r'C:\Program Files\Google\Chrome\Application\chrome.exe',headless=True,args=['--enable-unsafe-swiftshader'])
  for theme in ['classic','caustica']:
   for light in ['light','dark']:
    for width in [1440,760]:
     if os.environ.get('NQ_CLIPBOARD_QUICK') and (theme,light,width)!=('classic','light',1440):continue
     c=b.new_context(viewport={'width':width,'height':960},permissions=['clipboard-read','clipboard-write'])
     c.add_init_script(f"localStorage.setItem('nq-key','copy-test');localStorage.setItem('nq-appearance','{theme}');localStorage.setItem('nq-theme','{light}');")
     c.route('**/api/events*',lambda r:r.abort());c.route('**/api/state',lambda r:r.fulfill(json=snap))
     q=c.new_page();errors=[];q.on('pageerror',lambda e:errors.append(str(e)));q.goto('http://127.0.0.1:18503');q.wait_for_function('window.__nqAudit?.length > 0');q.wait_for_timeout(700)
     cid=snap['queue'][0]['id'];name=snap['queue'][0]['subject'];job=f'job:{cid}';file=f'file:{cid}:0'
     click(q,job);q.keyboard.press('Control+c');q.wait_for_timeout(150);assert clip(q)==name,('parent',clip(q))
     click(q,file);q.keyboard.press('Control+c');q.wait_for_timeout(150);assert clip(q)=='Example volume.part01.rar',('child',clip(q))
     q.keyboard.press('Delete');q.wait_for_timeout(150);assert not any(x['label']=='Delete job' for x in items(q))
     click(q,file,'right');click(q,'Copy details');assert 'Example volume.part01.rar' in clip(q) and '30%' in clip(q)
     click(q,file,'right');click(q,'Copy filename');assert clip(q)=='Example volume.part01.rar'
     click(q,job,'right');click(q,'Copy name');assert clip(q)==name
     click(q,job,'right')
     with q.expect_response(lambda response: response.url.endswith(f'/api/queue/{cid}/unpack-on')) as chosen:
      click(q,'Unpack after download')
     assert chosen.value.status==202

     click(q,job);q.keyboard.press('F2');q.wait_for_timeout(180);q.keyboard.press('Control+a');q.keyboard.press('Control+c');q.wait_for_timeout(150);assert clip(q)==name,('edit',clip(q));q.keyboard.press('Escape');q.wait_for_timeout(180)
     for tab,row,expected in [('History','history:0','Finished example'),('PAR2','par2:0','Repair example'),('Log','log:0','Example log message'),('RSS','rss:0','Feed example')]:
      hit(q,tab);click(q,row);q.keyboard.press('Control+c');q.wait_for_timeout(150);assert expected in clip(q),(tab,clip(q))
      click(q,row,'right');click(q,'Copy details');assert expected in clip(q)
      if tab=='Log':
       original=snap['log_tail'][0]
       snap['log_tail'].insert(0,{'time':'12:01','level':'Info','message':'New log entry'})
       q.wait_for_timeout(1800);q.keyboard.press('Control+c');q.wait_for_timeout(150);assert 'Example log message' in clip(q) and 'New log entry' not in clip(q)
       snap['log_tail']=[original]
      if tab=='RSS':
       original=snap['rss'];snap['rss']=[];q.wait_for_timeout(1800)
       q.evaluate("navigator.clipboard.writeText('clipboard unchanged')");q.keyboard.press('Control+c');q.wait_for_timeout(150);assert clip(q)=='clipboard unchanged'
       snap['rss']=original

     hit(q,'Download queue');click(q,'connection:0');q.keyboard.press('Control+c');q.wait_for_timeout(150);assert 'Example article' in clip(q)
     click(q,file,'right');q.screenshot(path=str(OUT/f'copy-{theme}-{light}-{width}.png'));q.keyboard.press('Escape')
     assert not errors,errors
     c.close();print('PASS',theme,light,width,flush=True)
  b.close()
finally:p.terminate();p.wait();log.close()
