import csv, gzip, sys, collections
src, out = sys.argv[1], sys.argv[2]
rw = collections.defaultdict(list)
for r in csv.DictReader(open(src + '/runways.csv', encoding='utf-8')):
    if r['closed'] == '1': continue
    rw[r['airport_ident']].append(r)
def f(v, nd=5):
    try: s = '%.*f' % (nd, float(v))
    except: return ''
    return s.rstrip('0').rstrip('.') if '.' in s else s
def clean(s): return ' '.join(s.replace('\t', ' ').split())
lines = []; na = nr = 0
for a in csv.DictReader(open(src + '/airports.csv', encoding='utf-8')):
    t = {'large_airport': 'L', 'medium_airport': 'M', 'small_airport': 'S'}.get(a['type'])
    if not t: continue
    rs = [r for r in rw.get(a['ident'], []) if not r['le_ident'].upper().startswith('H')]
    if not rs: continue
    try: lat, lon = float(a['latitude_deg']), float(a['longitude_deg'])
    except: continue
    lines.append('\t'.join(['A', a['ident'], a['iata_code'], t, f(lat), f(lon), f(a['elevation_ft'], 0), clean(a['name']), clean(a['municipality']), a['iso_country'], '1' if a['scheduled_service'] == 'yes' else '0']))
    na += 1
    for r in rs:
        lines.append('\t'.join(['R', f(r['length_ft'], 0), f(r['width_ft'], 0), clean(r['surface'])[:12], r['lighted'] or '0', r['le_ident'], f(r['le_latitude_deg']), f(r['le_longitude_deg']), f(r['le_displaced_threshold_ft'], 0), r['he_ident'], f(r['he_latitude_deg']), f(r['he_longitude_deg']), f(r['he_displaced_threshold_ft'], 0)]))
        nr += 1
data = ('\n'.join(lines) + '\n').encode('utf-8')
open(out, 'wb').write(gzip.compress(data, 9, mtime=0))
print(na, 'airports', nr, 'runways', len(data), 'bytes raw')
