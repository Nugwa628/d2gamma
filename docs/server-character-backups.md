# Server character backups

The main server (`d2rust-realm`) backs up its accounts and online characters.
Live and nightly backups (`server-ctl backup`, the `d2rust-realm-backup.timer`)
are described in `tools/server/README.md` ("Backups"); this page covers the
offline command and restoring.

Stop the main server, then run:

```text
d2rust-realm --config /srv/d2/d2rust-realm.cfg --backup /srv/backups/before-update
```

Without a folder the backup goes to a dated folder in the config's `backups`
folder, and the oldest beyond the kept count are removed.

The backup destination must be new, its parent must exist, and it must be
outside the data folder. Relative backup paths use the command's working
directory; paths in the config still use the config's directory. Backup mode
does not read game data, start a game, or connect to a network.

A backup holds:

- `accounts.sqlite`: a consistent copy of the accounts database (`VACUUM INTO`);
- `realms/<mod>/realm.sqlite`: the mod realm's database (shared stash, market,
  item registry), when it exists;
- `characters/<account id>/<name>.d2rc`: every named character. Each envelope
  is validated, including its version and checksum, and copied without
  conversion. Unfinished `.tmp` saves and unrelated files are excluded.

A successful command exits zero and reports the character count.
`BACKUP-COMPLETE` is published after every copied file is flushed. Any failure
exits nonzero; a partial directory may remain for diagnosis, without that
completion marker. Never restore an incomplete backup or reuse its
destination. Keep the previous completed backup until a new backup succeeds.
File flushing does not promise directory-entry durability through power loss
on every filesystem; keep independent backups for that risk.

To restore, keep the main server stopped, verify `BACKUP-COMPLETE` exists in a
successful backup, and copy its `accounts.sqlite`, `realms/` and `characters/`
into the data folder in place of the old ones (move the old ones aside first).
Do not merge old records into an active store. Keep both the previous live
data and the backup until the restored characters have been checked. Online
characters remain on the server; this command never imports local
single-player saves.
