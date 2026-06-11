import json
import subprocess
import sys
import time
from pathlib import Path

# Output uses emoji/checkmarks; force UTF-8 with replacement so piping or
# redirecting on a legacy-codepage console (cp1252) can't crash the test.
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

EXE = Path(__file__).parent.parent / 'target' / 'release' / 'OpenControl.exe'
p = subprocess.Popen([str(EXE)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0, text=False)

def req(method, params=None):
    msg = json.dumps({'jsonrpc':'2.0','id':1,'method':method,'params':params or {}}) + '\n'
    p.stdin.write(msg.encode())
    p.stdin.flush()
    resp = p.stdout.readline()
    return json.loads(resp.decode())

try:
    # Initialize
    init = req('initialize', {'protocolVersion':'2024-11-05','capabilities':{},'clientInfo':{'name':'test','version':'1'}})
    print('✓ Initialized:', init.get('result',{}).get('serverInfo',{}))

    # Send initialized notification
    notify = b'{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n'
    p.stdin.write(notify)
    p.stdin.flush()
    time.sleep(0.2)

    # Test screenshot
    print('\n📸 Taking screenshot...')
    r = req('tools/call', {'name':'take_screenshot','arguments':{}})
    if 'result' in r:
        content = r['result']['content'][0]
        img_data = content['text'][:50] + '...' if content.get('type') == 'text' else str(content)[:100]
        print(f'✓ Screenshot captured (base64): {img_data}')

    # Test get_cursor_position
    print('\n🖱️  Get cursor position...')
    r = req('tools/call', {'name':'get_cursor_position','arguments':{}})
    pos = json.loads(r['result']['content'][0]['text'])
    print(f'✓ Cursor at: ({pos["x"]}, {pos["y"]})')

    # Test screen_info
    print('\n📺 Screen info...')
    r = req('tools/call', {'name':'screen_info','arguments':{}})
    info = json.loads(r['result']['content'][0]['text'])
    vd = info.get('virtual_desktop', {})
    print(f'✓ Monitors: {len(info.get("monitors",[]))} | Virtual desktop: {vd.get("width")}x{vd.get("height")}')

    # Test list_processes
    print('\n🔍 List processes (explorer)...')
    r = req('tools/call', {'name':'list_processes','arguments':{'filter':'explorer','limit':3}})
    procs = json.loads(r['result']['content'][0]['text'])
    print(f'✓ Found {procs.get("count",0)} processes')
    for proc in procs.get('processes',[])[:2]:
        print(f'  - {proc["name"]} (PID {proc["pid"]})')

    # Test OCR
    print('\n📖 OCR on screen region...')
    r = req('tools/call', {'name':'ocr','arguments':{'region':[0,0,400,200]}})
    ocr_text = r['result']['content'][0]['text'][:100]
    print(f'✓ OCR detected text: {ocr_text}')

    # Test get_active_window
    print('\n🪟 Get active window...')
    r = req('tools/call', {'name':'get_active_window','arguments':{}})
    window = json.loads(r['result']['content'][0]['text'])
    print(f'✓ Active: {window["title"][:60]}... (PID {window["id"]})')

    # Test get_clipboard
    print('\n📋 Get clipboard...')
    r = req('tools/call', {'name':'get_clipboard','arguments':{}})
    clipboard = r['result']['content'][0]['text'][:80]
    print(f'✓ Clipboard: {clipboard}')

    print('\n✅ All core OpenControl tools working!\n')

finally:
    try:
        p.terminate()
        p.wait(timeout=2)
    except:
        p.kill()
