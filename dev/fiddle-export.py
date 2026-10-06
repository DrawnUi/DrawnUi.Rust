"""DrawFiddle's Rust export, for testing templates/fiddle: makes one variant with a snippet.
usage: python3 dev/fiddle-export.py <variant> <snippet.rs> <out dir> <ident> <display name> [RRGGBB]
e.g.   python3 dev/fiddle-export.py "All platforms" pong.rs /tmp/pong pong Pong
The variants and the rules are in templates/fiddle/variants.json."""
import json, os, re, shutil, stat, sys

tpl = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'templates', 'fiddle')
variant, snippet, out, ident, display = sys.argv[1:6]
bg = sys.argv[6] if len(sys.argv) > 6 else None
m = json.load(open(os.path.join(tpl, 'variants.json'), encoding='utf-8'))
if os.path.exists(out):
    shutil.rmtree(out)
pairs = m['common'] + m['variants'][variant]
for src, dst in pairs:
    s, d = os.path.join(tpl, src), os.path.join(out, dst)
    if src.endswith('/'):
        shutil.copytree(s, d, dirs_exist_ok=True)
    else:
        os.makedirs(os.path.dirname(d), exist_ok=True)
        shutil.copy2(s, d)
code = open(snippet, encoding='utf-8').read()
open(os.path.join(out, m['snippet']), 'w', encoding='utf-8', newline='\n').write(code)
has_configure = re.search(r'^\s*fn\s+configure\s*\(', code, re.M) is not None
c = m['configure']
open(os.path.join(out, c['file']), 'w', encoding='utf-8', newline='\n').write(
    "// DrawFiddle writes this file: it calls the fiddle code's `configure` when the code has one.\n"
    + (c['with'] if has_configure else c['without']) + "\n")
for rel in m['text_files']:
    p = os.path.join(out, rel)
    if not os.path.exists(p):
        continue
    t = open(p, encoding='utf-8').read()
    t = t.replace('My App', display).replace('myapp', ident)
    if bg:
        t = t.replace('121218', bg)
    open(p, 'w', encoding='utf-8', newline='').write(t)
for rel in m['executable']:
    p = os.path.join(out, rel)
    if os.path.exists(p):
        os.chmod(p, os.stat(p).st_mode | stat.S_IEXEC)
print(variant, '->', out, 'configure' if has_configure else 'no configure')
