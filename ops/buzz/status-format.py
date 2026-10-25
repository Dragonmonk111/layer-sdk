import json, sys
j = json.load(sys.stdin)
probes = j['latest']['probes']
ok = [p['ok'] for p in probes]
down = [p['name'].split(' (')[0] for p in probes if not p['ok']]
mark = 'OK' if all(ok) else 'DEGRADED'
msg = '[%s] seq %s | probes %d/3 up | uptime %.3f%%' % (
    mark, format(j['tip'], ','), len(ok) - len(down),
    j['uptime_pct_since_start'])
if down:
    msg += ' | DOWN: ' + ','.join(down)
print(msg)
print(mark)
