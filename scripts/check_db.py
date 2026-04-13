import sqlite3
conn = sqlite3.connect('quantd.db')
cur = conn.cursor()
print(cur.execute("SELECT key, value FROM system_config WHERE key = 'strategy.acc_lb_paper'").fetchall())
