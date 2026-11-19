# Restore drill

Run `bash docs/ops/restore-drill.sh` (needs cargo, curl, jq, sqlite3, openssl, python3). Mock vendor, fake key
`sk-proj-DRILLFAKE-KEY-1234`, temp databases, random port; kills only the servers it started.
It proves: a backup taken while the server runs restores into a new database, the stored key decrypts with the
same master key and fails with `no_key` under a different one, and the deletion replay removes a deleted account.

Last run: 2026-11-18 (macOS, debug build). Output:

```
== 1. original server: user, fake key, app, grant, one completed request
PASS  original: request completed
== 2. backup while the server is running
backup:    /tmp/drill/backups/masterkey-20261118T100810000Z.db
size:      135168 bytes
integrity: ok
PASS  backup integrity
PASS  backup mode 600
== 3. restore into a NEW database, same master key
PASS  restored: key last4 listed
PASS  restored: history has the request
PASS  restored: key decrypts (mock saw last4)
== 4. restore with a DIFFERENT master key
PASS  wrong master key: request fails
PASS  wrong master key: error is no_key
== 5. replay of deletions recorded after the backup
replayed 1 deletion(s) from /tmp/drill/deleted-accounts.log
PASS  replay: user gone
PASS  replay: requests gone

RESTORE DRILL: PASS
```
