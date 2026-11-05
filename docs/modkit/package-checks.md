# Checked publication

`D2Gamma-Game package-mod <folder> --base <converted-install> [--out PACK]
[--report DIR] [--mods-dir DIR]` runs the complete default `check-mod` library
checks through `d2_native::package::package_with_checks`. The callback checks
the exact staged snapshot that is signed. Errors refuse publication; warnings
remain visible and do not block signing. The default checks select gameplay
scenarios for changed rows, as `check-mod` does; exhaustive `check-mod --full`
remains available separately.

The command prints the English report and writes `report.json` and `report.txt`
to `--report` (default `package-mod-report`). Setup/content-policy errors also
produce an error report. An old report JSON is removed before a new attempt.
The packager retains the existing content thresholds, author-key location and
atomic output replacement. D2Gamma's ordinary play path is unchanged.

`d2-convert package-mod` forwards to the headless game command, so it cannot
publish using only the smaller native checks. Put `D2Gamma-Game` beside the
converter or pass `--game <executable>`. All other publication arguments are
forwarded unchanged. A missing checker is an error, with no fallback to
unchecked publication. Neither converter nor Studio links the game renderer.

