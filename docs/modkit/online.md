# Online mod identity

A mod-kit session identifies its full mod by manifest `id` and `version`.
Cosmetic screen-only add-ons do not alter that identity. A realm configured
with `game = <id>` and `mod_version = <version>` requires an exact match.
The existing build-upgrade refusal displays the required mod/version and tells
the player to update and rejoin. The existing in-game mod contract handshake
continues to refuse stale clients before their UI actions are accepted.

Game entries and node joins carry the same optional identity. The client shows
it with the listing and verifies it before attaching to the game node. The
optional wire-v1 extension is appended only for mods; plain D2Gamma retains
its wire-v6 hello, game-list and join bytes. Existing engine build checks still
apply independently. Game/realm ids accept the manifest's 40-character limit;
node ids remain at 15. Account and economy database schemas are unchanged.

A full mod may set `market = false` and/or `ladder = false` in `mod.toml`.
Both default to true (permission to use the service); a realm still has to
explicitly enable each service. Add-ons cannot set those switches.

Set `mod_manifest = path/to/mod.toml` in the realm config to bind the deployed
full mod's id, version and service permissions. Relative paths use the config
folder. An explicit conflicting `game` or `mod_version` is an error. Deploy
the matching game data/node stack as usual. The existing shared accounts stay
shared; stash, listings/deliveries, ladder and seasons use the mod id namespace.
Versions do not rename or migrate that namespace. No database schema changes
are required. Plain D2Gamma without this setting uses its existing namespace
and service defaults.

Official download prompts are disabled unless the owner sets
`official_list_url` in `Documents/D2Gamma/mod-downloads.toml`. The shipped
example is empty. No endpoint, hosting service or catalogue is supplied here.

The launcher transport fetches that exact HTTPS URL without redirects and
writes a bounded `mod-download-offer.json` envelope next to the setting:
`source_url`, `required` (`id`, `version`) and `offer` (`id`, `version`, `author`,
`bytes`, `package_url`, `sha256`, `author_key`). Package URLs currently must use
the configured official origin. No realm-supplied URL is trusted. Transport
and hosting policy remain subject to owner review.

D2Gamma Mods displays the mod/version, author and exact byte count. DOWNLOAD
is an explicit click; Enter does not consent. CANCEL writes nothing. Confirm
creates one `mod-download-request.json` for the launcher, preserving all
confirmed fields. It never replaces another pending request or installs a
package into a running game. The completion hook checks exact file size,
SHA-256, pack signature and pinned author key, then manifest id/version/author
and existing API/load checks before installation. Changed offers require new
consent. The launcher consumes the request and refreshes its offer cache after
completion/cancellation. Missing configuration makes no request and changes no
plain-game controls or wire bytes.

The base realm id `d2rust` is reserved, alongside `d2gamma`, so no kit mod
can share the plain game's economy namespace.
