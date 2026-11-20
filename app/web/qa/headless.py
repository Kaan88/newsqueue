"""Headless real-browser QA. Uses isolated demo servers; never desktop input."""
import sys, json, time, subprocess, urllib.request, os
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'app/target/qa-python'))
from playwright.sync_api import sync_playwright
OUT=ROOT/'app/target/web-audit'
BASE_PORT=int(os.environ.get('NQ_AUDIT_PORT','18481'))
BUILD=os.environ.get('NQ_AUDIT_BUILD','current')
RUN=os.environ.get('NQ_AUDIT_RUN','matrix')

def start_server(port,web,key=None):
    log=open(OUT/f'server-{port}.log','w')
    p=subprocess.Popen([str(ROOT/'app/target/release/newsqueue-server.exe'),'--bind',f'127.0.0.1:{port}','--config',str(OUT/f'config-{port}'),'--web',str(web),'--demo']+(['--api-key',key] if key else []),stdout=log,stderr=log,creationflags=subprocess.CREATE_NO_WINDOW)
    for _ in range(80):
        try:
            with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{port}/api/state',headers={'X-Api-Key':key} if key else {})) as r: snap=json.load(r)
            return p,log,snap
        except Exception: time.sleep(.1)
    p.kill();raise RuntimeError('Server failed')

def items(page):return page.evaluate('window.__nqAudit || []')
def hit(page,label,index=-1):
    found=[i for i in items(page) if i['label']==label and i['enabled']]
    if not found: raise AssertionError(f'Missing {label}: {[i["label"] for i in items(page)]}')
    r=found[index]['clip'];page.mouse.click((r[0]+r[2])/2,(r[1]+r[3])/2);page.wait_for_timeout(180)


def capture(page, folder, name):
    page.mouse.move(page.viewport_size['width']-2,page.viewport_size['height']-2);page.wait_for_timeout(100)
    folder.mkdir(parents=True,exist_ok=True)
    page.screenshot(path=str(folder/(name+'.png')))
    (folder/(name+'.json')).write_text(json.dumps(items(page)))

def menu(page,top,choice):
    hit(page,top,0);hit(page,choice)

def close(page):
    page.keyboard.press('Escape');page.wait_for_timeout(150)

def matrix(browser,snap):
    pages=['Servers','Folders','Download','Connection','PAR2 & unpack','Scheduler','RSS feeds','Log & history']
    results=[]
    for appearance in os.environ.get('NQ_AUDIT_THEMES','classic,caustica').split(','):
      for brightness in ['light','dark']:
       for width,height in [(1280,820),(760,600),(390,844)]:
        for scale in [1,2]:
            name=f'{appearance}-{brightness}-{width}-{scale}'
            folder=OUT/RUN/name
            if (folder/'PASS').exists():continue
            ctx=browser.new_context(viewport={'width':width,'height':height},device_scale_factor=scale,color_scheme=brightness)
            # Chrome emulation reports CSS pixels in devicePixelContentBoxSize.
            # Force eframe's supported contentBoxSize * DPR fallback in this harness.
            ctx.add_init_script("delete ResizeObserverEntry.prototype.devicePixelContentBoxSize")
            ctx.add_init_script(f"localStorage.setItem('nq-appearance','{appearance}');localStorage.setItem('nq-theme','{brightness}');")
            ctx.route('**/api/events*',lambda route:route.abort())
            ctx.route('**/api/state',lambda route:route.fulfill(json=snap))
            page=ctx.new_page();errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
            page.goto(f'http://127.0.0.1:{BASE_PORT}');page.wait_for_function('window.__nqAudit?.length > 0');page.wait_for_timeout(800)
            tabs=['Download queue' if width>=720 else 'Queue','History','PAR2','Log','RSS']
            coords=[]
            for i,tab in enumerate(tabs):
                found=[i['clip'] for i in items(page) if i['label']==tab]
                if not found:
                    capture(page,folder,'FAILED');raise AssertionError((name,tab,items(page)))
                r=found[-1];coords.append(((r[0]+r[2])/2,(r[1]+r[3])/2))
                hit(page,tab);capture(page,folder,f'tab-{i}')
            hit(page,tabs[0]);hit(page,'job:2')
            for top in ['File','Queue','View','Tools','Help']:
                hit(page,top,0);capture(page,folder,'menu-'+top);close(page)
            for top,choice,label in [('File','Add NZB files...','add'),('Queue','Rename...','rename'),('Queue','Archive password...','password'),('Queue','Speed limit...','speed'),('Queue','Pause for...','pause'),('Queue','Delete','delete'),('Help','About NewsQueue','about')]:
                menu(page,top,choice);capture(page,folder,'dialog-'+label);close(page)
            menu(page,'View','Statistics...')
            for i,label in enumerate(['Daily transfer','Monthly transfer','Totals']):
                hit(page,label);capture(page,folder,f'stats-{i}')
            close(page)
            menu(page,'Tools','Options...')
            for i,label in enumerate(pages):
                if width<720:
                    combo=[v['label'] for v in items(page) if v['label'].startswith('combo:')][0]
                    hit(page,combo);hit(page,label)
                else:hit(page,label)
                capture(page,folder,f'options-{i}')
                # Exercise and capture the bottom of every scrollable settings page.
                page.mouse.move(width*.68,height*.55);page.mouse.wheel(0,1800);page.wait_for_timeout(150)
                capture(page,folder,f'options-{i}-bottom')
                page.mouse.move(width*.68,height*.55);page.mouse.wheel(0,-1800);page.wait_for_timeout(120)
            close(page)
            assert not errors,(name,errors)
            # Compare the unchanged Classic main surfaces to the exact prior WASM build.
            if appearance=='classic':
                baseline=ctx.new_page();baseline.goto(f'http://127.0.0.1:{BASE_PORT+1}');baseline.wait_for_timeout(500)
                for i,(x,y) in enumerate(coords):
                    baseline.mouse.click(x,y);baseline.wait_for_timeout(150)
                    baseline.mouse.move(width-2,height-2);baseline.wait_for_timeout(100)
                    baseline.screenshot(path=str(folder/f'baseline-tab-{i}.png'))
                baseline.close()
            results.append({'case':name,'captures':len(list(folder.glob('*.png'))),'errors':errors})
            (folder/'PASS').write_text('passed')
            print(name,'PASS',flush=True);ctx.close()
    (OUT/(RUN+'-'+os.environ.get('NQ_AUDIT_THEMES','all')+'-results.json')).write_text(json.dumps(results,indent=2))

def run():
    servers=[]
    try:
        p,log,snap=start_server(BASE_PORT,OUT/BUILD);servers.append((p,log))
        p,log,_=start_server(BASE_PORT+1,OUT/'classic-baseline');servers.append((p,log))
        with sync_playwright() as pw:
            browser=pw.chromium.launch(executable_path=r'C:\Program Files\Google\Chrome\Application\chrome.exe',headless=True,args=['--disable-background-networking','--enable-unsafe-swiftshader'])
            matrix(browser,snap)
            browser.close()
    finally:
        for p,log in servers:p.terminate();p.wait();log.close()
if __name__=='__main__':run()
