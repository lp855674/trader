import sqlite3
import json

conn = sqlite3.connect('quantd.db')
cur = conn.cursor()

config_json = json.dumps({'app_key': 'test_key', 'app_secret': 'test_secret', 'access_token': 'test_token'})
cur.execute("INSERT OR REPLACE INTO execution_profiles (id, kind, config_json) VALUES (?, ?, ?)", 
            ('longbridge_paper', 'longbridge_paper', config_json))
cur.execute("INSERT OR IGNORE INTO accounts (id, mode, execution_profile_id, venue) VALUES (?, ?, ?, ?)", 
            ('acc_lb_paper', 'paper', 'longbridge_paper', None))
conn.commit()

print("Done")
print(cur.execute("SELECT * FROM execution_profiles").fetchall())
print(cur.execute("SELECT * FROM accounts").fetchall())
