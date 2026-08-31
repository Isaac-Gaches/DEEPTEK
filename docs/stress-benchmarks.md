# CPU stress benchmarks

The headless stress suite measures the simulation paths that are most likely to
limit a laptop: entity collision, lifeform AI, machinery-chunk scheduling, power
topology and distribution, drill ticks, and a combined frame. It uses the real
world, ECS, power, lifeform, and nature systems; setup time is excluded.

Run the short correctness/performance smoke first:

```powershell
cargo bench --bench stress -- --preset quick
```

Use the laptop preset while iterating and the heavy preset before a release:

```powershell
cargo bench --bench stress -- --preset laptop
cargo bench --bench stress -- --preset heavy
```

The heavy preset exercises 20,000 lifeforms/colliders, 1,000 powered drills, and
3,000 machinery-occupied chunks. A scenario can be isolated to confirm a suspected
hot path:

```powershell
cargo bench --bench stress -- --preset heavy --scenario lifeform-ai
cargo bench --bench stress -- --preset heavy --scenario machinery-lifeforms
cargo bench --bench stress -- --preset heavy --scenario active-chunks
cargo bench --bench stress -- --preset heavy --scenario power-rebuild
cargo bench --bench stress -- --preset heavy --scenario power-local-edit
cargo bench --bench stress -- --preset heavy --scenario drill-tick
cargo bench --bench stress -- --preset heavy --scenario combined-frame
```

## Reading the output

- Use p95 as the normal laptop budget and p99/max to find periodic stutters.
- A complete 60 FPS frame is 16.67 ms, but CPU simulation should normally stay
  below 6 ms p95 so rendering, input, audio, and the operating system have room.
- For a 30 FPS fallback, keep simulation below roughly 15 ms p95.
- `power topology rebuild`, `active-chunk refresh`, and `1-second drill tick` are
  intentionally spike tests. They need not fit every-frame budgets, but their p99
  should remain below one visible frame where practical.
- `p95-only FPS` is a comparison aid, not predicted game FPS; it assumes the named
  subsystem is the only work in the frame.

For thermal and battery validation, run the heavy suite once plugged in and once
on battery after the laptop has reached steady temperature. Close debuggers and
other CPU-heavy applications, retain both outputs, and compare p95/p99 rather than
only the mean. This suite is CPU-only; GPU terrain, lighting, and sprite saturation
still require an integrated release-mode playtest or graphics capture.

## Initial heavy-run findings

The initial optimized run on 25 August 2026 produced these p95 times. They are a
baseline for this development machine, not portable performance guarantees.

| Scenario | Heavy workload | p95 |
| --- | ---: | ---: |
| Machinery-aware lifeforms | 20,000 lifeforms, 1,000 targets | 40.55 ms |
| Combined simulation frame | 20,000 lifeforms, 1,000 drills | 37.86 ms |
| Power topology rebuild | 3,000 nodes, 11,984 candidates | 19.17 ms |
| Localized power edit | One node among 3,000 | 1.94 ms |
| Lifeform AI and physics | 20,000 lifeforms | 3.07 ms |
| Entity physics | 20,000 colliders | 2.84 ms |
| Drill tick | 1,000 drills | 0.36 ms |
| Power distribution | 1,000 drills | 0.23 ms |
| Active-chunk refresh | 3,000 chunks | 0.09 ms |

The first optimization target is machinery-aware lifeform targeting. Plain
lifeform AI and collision remain under 3.1 ms at the same population, while the
machinery-aware path performs a per-lifeform search across a 5 × 5 chunk area and
revalidates candidate objects during every search. Cache target candidates by
lifeform chunk (and invalidate them on machinery changes) before reducing physics
quality or chunk simulation speed.

Localized power edits now use the bounded object-change journal and persistent
socket buckets. Adding or removing one node updates only its nearby candidates;
the 25 August comparison reduced p95 from 18.17 ms for a cold rebuild to 1.94 ms,
an 89% reduction. Powered-cable structural changes and missing journal history
retain the full rebuild as a correctness fallback. The remaining cold 19 ms path
is primarily startup/load work; steady distribution is only 0.23 ms.

Drill scheduling and machinery-chunk refresh are already comfortably bounded.
They should be monitored for regressions, not redesigned ahead of the two hot paths
above.
