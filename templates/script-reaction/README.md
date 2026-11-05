# Script Reaction

Copy this asset-free folder, rename its manifest ID, and open it as a full mod.
It needs no added table rows. Use a skill successfully as a player: a K7
skill-used trigger calls `reaction.luau::react`, sends a private chat line,
and schedules a second line 25 simulation frames later. Character level
(stat 12) supplies the call's integer argument. This deliberately visible
sample can be noisy; replace the messages with your own state/stat logic.

Run `D2Gamma-Game check-mod <folder> --native <native-folder> --out report.json`
before Play. Edit the script and use checked dev reload to restart with fresh
VMs. See [the script guide](../../docs/modkit/scripts.md) for skills.toml calls,
all APIs, the checkpoint-based execution limit, and transient timer semantics.
