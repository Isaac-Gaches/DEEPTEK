# CPU stress benchmarks

The headless stress suite measures the simulation paths that are most likely to
limit a laptop: entity collision, lifeform AI, machinery-chunk scheduling, power
topology and distribution, lighting input preparation, terrain edits and chunk meshes,
drill ticks, and a combined frame. It uses the real world, ECS, rendering preparation,
power, lifeform, and nature systems; setup time is excluded.

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
cargo bench --bench stress -- --preset laptop --scenario lighting-input
cargo bench --bench stress -- --preset laptop --scenario lighting-input-medium
cargo bench --bench stress -- --preset laptop --scenario lighting-local-edit
cargo bench --bench stress -- --preset laptop --scenario terrain-mesh
cargo bench --bench stress -- --preset laptop --scenario terrain-edits
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
only the mean. Lighting input preparation and terrain mesh generation are CPU
measurements. GPU lighting compute dispatches, terrain uploads, and sprite saturation
still require an integrated release-mode playtest or graphics capture.

## Measured optimization results

The 1 September 2026 terrain-rendering pass added localized lighting-input updates
and pooled chunk-mesh allocations. Measurements use the optimized laptop preset on
the same development machine:

| Scenario | Before p95 | After p95 | Change |
| --- | ---: | ---: | ---: |
| One-cell lighting occupancy update | 135.60 us full scan | 1.90 us localized | -98.6% |
| Repeated 32×32 foreground mesh | 39.40 us | 16.60 us | -57.9% |
| Medium full lighting input vs High | 131.10 us High | 54.50 us Medium | -58.4% |

The GPU path also combines sky and ambient-occlusion generation into one compute
pass and removes a redundant full-texture clear. This retains the same diffusion and
smoothing passes; GPU timing remains adapter-dependent and is not included above.
Medium uses exactly half as many light-map texels and half the size-dependent texture
memory as High (192×128 versus 256×192 before the common 2× upscale).

The 31 August 2026 cleanup pass measured the suite before changing the hot path, then
repeated it after caching machinery targets by lifeform chunk and taking one machine
active-state snapshot per simulation update. These are measurements from this
development machine, not portable performance guarantees.

| Scenario | Workload | Before p95 | After p95 | Change |
| --- | ---: | ---: | ---: | ---: |
| Machinery-aware lifeforms | 20,000 lifeforms, 1,000 targets | 52.28 ms | 25.62 ms | -51.0% |
| Combined simulation frame | 20,000 lifeforms, 1,000 drills | 53.67 ms | 25.69 ms | -52.1% |
| Power topology rebuild | 3,000 nodes, 11,984 candidates | 18.39 ms | 17.97 ms | -2.3% |
| Machinery-aware lifeforms | 1,000 lifeforms, 64 targets | 2.37 ms | 1.14 ms | -51.9% |
| Combined simulation frame | 1,000 lifeforms, 64 drills | 2.34 ms | 1.15 ms | -50.9% |

The target cache is rebuilt with the existing machinery refresh, so changes retain the
same bounded invalidation point. Machine health and active state are still checked each
simulation update, but once per target rather than once for every nearby lifeform.

The quick preset now keeps every measured scenario under 1.2 ms p95. The heavy
machinery scenario remains above one frame because it deliberately updates 20,000
lifeforms—well beyond the normal configured population—in one call. It is useful as a
regression stressor, but does not justify reducing collision or AI quality while normal
and 1,000-lifeform workloads remain below the 6 ms simulation budget.

The next heavy spike is the roughly 18 ms cold power-topology rebuild. Localized edits
use the bounded object-change journal and persistent socket buckets, while steady power
distribution is a small fraction of a millisecond. Full rebuilds are retained as a
correctness fallback for startup/load, powered-cable structural changes, and missing
journal history.

Drill scheduling, active-chunk refresh, localized power edits, and steady power
distribution should be monitored for regressions rather than redesigned without a new
measurement showing a real gameplay bottleneck.
