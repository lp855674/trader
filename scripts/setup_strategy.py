import sqlite3
import time

conn = sqlite3.connect('quantd.db')
cur = conn.cursor()
now = int(time.time())

# Write lstm.service_url
cur.execute("INSERT INTO system_config (id, key, value, updated_at, created_at) VALUES (?, ?, ?, ?, ?)",
            ('lstm.service_url', 'lstm.service_url', 'http://127.0.0.1:8000', now, now))

# Write strategy config for acc_lb_paper
config = '{"type":"lstm","model_type":"lstm","symbol":"AAPL.US","lookback":60,"buy_threshold":0.6,"sell_threshold":-0.6}'
cur.execute("INSERT INTO system_config (id, key, value, updated_at, created_at) VALUES (?, ?, ?, ?, ?)",
            ('strategy.acc_lb_paper', 'strategy.acc_lb_paper', config, now, now))

conn.commit()

print("Done")
print(cur.execute("SELECT key, value FROM system_config WHERE key LIKE 'lstm.%' OR key LIKE 'strategy.%'").fetchall())
