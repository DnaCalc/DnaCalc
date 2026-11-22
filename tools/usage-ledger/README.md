# Usage ledger

Build the ledger from the coordinator's saved sweeps. Use the sweep archive
under `codex-runs/sweep/` or a directory of copies. The tool runs offline with
Python 3.9 or later and uses the account readings in those files.

```sh
lane_dir="$HOME/codex-runs/us1"
python3 tools/usage-ledger/usage_ledger.py "$lane_dir/sweeps" \
  --projects "$lane_dir/projects.json" > "$lane_dir/usage-ledger.jsonl.tmp" &&
mv "$lane_dir/usage-ledger.jsonl.tmp" "$lane_dir/usage-ledger.jsonl"
```

Run this again when a sweep arrives. It rebuilds the history in timestamp order
with one JSON line per saved sweep. Each row names its source file and carries
the local sweep time as printed. Keep the sweeps, project map and generated
ledger in the run folder.

## Supply project names

The optional `--projects` file maps an exact `host/folder` from the sweep's
`codex-runs` lane listings to the project that owns that folder. For example:

```json
{
  "dna-firehorse/ig1": "DNA Calc",
  "dna-koderbot/cv2": "CalcVBA"
}
```

A lane marked running gets its folder, project and model when available. An
unmapped folder has a null project. When a project starts or ends heavy work,
have its coordinator report the host, folder, project and time at check-in and
update the project map. Use a new lane folder when switching projects so that a
single map has the same meaning across the history.

At check-in, report the latest account readings and the running lanes by project.
For the week review (dc-nvip), use the recorded readings and project activity
over the review interval. Account percentages include all projects. Lane
counts describe activity. A usage percentage split requires per-project usage
measurements or a calibrated burn rate.

## Read the account rows

- Codex retains each host's weekly percent used, UTC observation time and reset
  epoch. Use `codex.latest` for the shared counter. Hosts can repeat the same
  account reading, so use the newest observation by timestamp. Conflicting
  readings at the newest timestamp leave `latest` null.
- Gemini retains weekly, five-hour and Claude/GPT group percentages remaining
  and derives the used percentages as 100 minus remaining.
- Claude's current sweep placeholder produces a null. When the coordinator has
  numeric data in the sweep, the ledger accepts, for example,
  `- Claude used (five-hour, weekly, Fable): 10, 32.5, 0`. A missing or
  explicitly unread provider stays null in that sweep. Use the recorded
  observation time to check whether a Codex reading is recent.

## Ceiling for starting lanes

Apply the ceiling to total account usage across projects. The 80/80/90
shorthand is read here in provider order, with Claude at 80% used, Codex at
80% used and Gemini at 90% used. The current Codex exception raises its cap
to 99% used through the current weekly reset. After the reset, use the regular
cap. At a cap, hold the next lane and let running lanes finish. Use a fresh
reading for the decision. DNA Calc can use more of the available share when
other projects run light and gives way when they run heavy.

Run the tool's tests with
`python3 -m unittest discover -s tools/usage-ledger -v`.
