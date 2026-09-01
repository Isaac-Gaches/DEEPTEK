use std::env;
use std::hint::black_box;
use std::process::ExitCode;
use std::time::{Duration, Instant};

#[path = "stress/scenarios.rs"]
mod scenarios;

use scenarios::run_scenario;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresetName {
    Quick,
    Laptop,
    Heavy,
}

impl PresetName {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "quick" => Some(Self::Quick),
            "laptop" => Some(Self::Laptop),
            "heavy" => Some(Self::Heavy),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Preset {
    name: PresetName,
    entities: usize,
    drills: usize,
    active_chunks: usize,
    warmup_samples: usize,
    measured_samples: usize,
}

impl Preset {
    fn named(name: PresetName) -> Self {
        match name {
            PresetName::Quick => Self {
                name,
                entities: 1_000,
                drills: 64,
                active_chunks: 256,
                warmup_samples: 4,
                measured_samples: 20,
            },
            PresetName::Laptop => Self {
                name,
                entities: 5_000,
                drills: 256,
                active_chunks: 1_000,
                warmup_samples: 8,
                measured_samples: 60,
            },
            PresetName::Heavy => Self {
                name,
                entities: 20_000,
                drills: 1_000,
                active_chunks: 3_000,
                warmup_samples: 12,
                measured_samples: 120,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scenario {
    EntityPhysics,
    LifeformAi,
    MachineryLifeforms,
    ActiveChunkRefresh,
    PowerTopologyRebuild,
    PowerLocalizedEdit,
    PowerDistribution,
    DrillTick,
    LightingInput,
    LightingInputMedium,
    LightingLocalizedEdit,
    TerrainChunkMesh,
    TerrainEdits,
    CombinedFrame,
}

impl Scenario {
    const ALL: [Self; 14] = [
        Self::EntityPhysics,
        Self::LifeformAi,
        Self::MachineryLifeforms,
        Self::ActiveChunkRefresh,
        Self::PowerTopologyRebuild,
        Self::PowerLocalizedEdit,
        Self::PowerDistribution,
        Self::DrillTick,
        Self::LightingInput,
        Self::LightingInputMedium,
        Self::LightingLocalizedEdit,
        Self::TerrainChunkMesh,
        Self::TerrainEdits,
        Self::CombinedFrame,
    ];

    fn parse(value: &str) -> Option<Self> {
        match value {
            "entity-physics" => Some(Self::EntityPhysics),
            "lifeform-ai" => Some(Self::LifeformAi),
            "machinery-lifeforms" => Some(Self::MachineryLifeforms),
            "active-chunks" => Some(Self::ActiveChunkRefresh),
            "power-rebuild" => Some(Self::PowerTopologyRebuild),
            "power-local-edit" => Some(Self::PowerLocalizedEdit),
            "power-distribution" => Some(Self::PowerDistribution),
            "drill-tick" => Some(Self::DrillTick),
            "lighting-input" => Some(Self::LightingInput),
            "lighting-input-medium" => Some(Self::LightingInputMedium),
            "lighting-local-edit" => Some(Self::LightingLocalizedEdit),
            "terrain-mesh" => Some(Self::TerrainChunkMesh),
            "terrain-edits" => Some(Self::TerrainEdits),
            "combined-frame" => Some(Self::CombinedFrame),
            _ => None,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::EntityPhysics => "entity physics frame",
            Self::LifeformAi => "lifeform AI + physics",
            Self::MachineryLifeforms => "machinery-aware lifeforms",
            Self::ActiveChunkRefresh => "active-chunk refresh",
            Self::PowerTopologyRebuild => "power topology rebuild",
            Self::PowerLocalizedEdit => "localized power edit",
            Self::PowerDistribution => "power distribution frame",
            Self::DrillTick => "1-second drill tick",
            Self::LightingInput => "lighting input refresh",
            Self::LightingInputMedium => "medium lighting input",
            Self::LightingLocalizedEdit => "localized lighting input",
            Self::TerrainChunkMesh => "terrain chunk mesh",
            Self::TerrainEdits => "terrain edit batch",
            Self::CombinedFrame => "combined simulation frame",
        }
    }
}

#[derive(Debug)]
struct Options {
    preset: Preset,
    scenarios: Vec<Scenario>,
}

#[derive(Debug)]
struct TimingSummary {
    scenario: &'static str,
    scale: String,
    mean: Duration,
    p50: Duration,
    p95: Duration,
    p99: Duration,
    maximum: Duration,
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(Some(options)) => options,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}\n");
            print_help();
            return ExitCode::FAILURE;
        }
    };

    println!(
        "DeepTek CPU stress suite: {:?} preset, {} warmup + {} measured samples",
        options.preset.name, options.preset.warmup_samples, options.preset.measured_samples
    );
    println!("Times are per simulation call; p95 is the primary laptop frame-budget signal.\n");

    let mut summaries = Vec::with_capacity(options.scenarios.len());
    for scenario in options.scenarios {
        println!("setting up {} ...", scenario.label());
        let summary = run_scenario(scenario, options.preset);
        print_summary(&summary);
        summaries.push(summary);
    }

    summaries.sort_by_key(|summary| std::cmp::Reverse(summary.p95));
    println!("\nBottleneck order (p95):");
    for (index, summary) in summaries.iter().enumerate() {
        println!(
            "  {}. {:26} {:>9}  ({})",
            index + 1,
            summary.scenario,
            format_duration(summary.p95),
            summary.scale
        );
    }
    println!("\nBudgets: 16.67 ms is an entire 60 FPS frame; target CPU simulation p95 <= 6 ms.");
    ExitCode::SUCCESS
}

fn parse_options() -> Result<Option<Options>, String> {
    let mut preset_name = PresetName::Quick;
    let mut scenarios = Vec::new();
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            // Cargo appends this compatibility marker to custom bench executables.
            "--bench" => {}
            "--help" | "-h" => {
                print_help();
                return Ok(None);
            }
            "--preset" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--preset requires quick, laptop, or heavy".to_owned())?;
                preset_name =
                    PresetName::parse(&value).ok_or_else(|| format!("unknown preset `{value}`"))?;
            }
            "--scenario" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--scenario requires a scenario name".to_owned())?;
                if value == "all" {
                    scenarios.clear();
                    scenarios.extend(Scenario::ALL);
                } else {
                    scenarios.push(
                        Scenario::parse(&value)
                            .ok_or_else(|| format!("unknown scenario `{value}`"))?,
                    );
                }
            }
            _ => return Err(format!("unknown argument `{argument}`")),
        }
    }
    if scenarios.is_empty() {
        scenarios.extend(Scenario::ALL);
    }
    scenarios.sort_by_key(|scenario| *scenario as u8);
    scenarios.dedup();
    Ok(Some(Options {
        preset: Preset::named(preset_name),
        scenarios,
    }))
}

fn print_help() {
    println!(
        "DeepTek CPU stress benchmarks\n\n\
         Usage: cargo bench --bench stress -- [options]\n\n\
         Options:\n\
           --preset <quick|laptop|heavy>   Workload size (default: quick)\n\
           --scenario <name|all>           Repeatable scenario filter (default: all)\n\
           -h, --help                      Show this help\n\n\
         Scenarios:\n\
           entity-physics, lifeform-ai, machinery-lifeforms, active-chunks,\n\
           power-rebuild, power-local-edit, power-distribution, drill-tick,
           lighting-input, lighting-input-medium, lighting-local-edit,
           terrain-mesh, terrain-edits, combined-frame"
    );
}

fn measure<T>(
    scenario: Scenario,
    scale: String,
    preset: Preset,
    mut operation: impl FnMut() -> T,
) -> TimingSummary {
    for _ in 0..preset.warmup_samples {
        black_box(operation());
    }
    let mut samples = Vec::with_capacity(preset.measured_samples);
    for _ in 0..preset.measured_samples {
        let start = Instant::now();
        black_box(operation());
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let total_nanos: u128 = samples.iter().map(Duration::as_nanos).sum();
    let mean_nanos = total_nanos / samples.len() as u128;
    TimingSummary {
        scenario: scenario.label(),
        scale,
        mean: Duration::from_nanos(mean_nanos.min(u128::from(u64::MAX)) as u64),
        p50: percentile(&samples, 50),
        p95: percentile(&samples, 95),
        p99: percentile(&samples, 99),
        maximum: *samples
            .last()
            .expect("a preset always has measured samples"),
    }
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    let index = (samples.len() - 1) * percentile / 100;
    samples[index]
}

fn print_summary(summary: &TimingSummary) {
    let p95_ms = summary.p95.as_secs_f64() * 1_000.0;
    let estimated_fps = if p95_ms > 0.0 {
        1_000.0 / p95_ms
    } else {
        f64::INFINITY
    };
    println!(
        "  {:26} {}\n  mean {:>9} | p50 {:>9} | p95 {:>9} | p99 {:>9} | max {:>9} | p95-only FPS {:>7.1}\n",
        summary.scenario,
        summary.scale,
        format_duration(summary.mean),
        format_duration(summary.p50),
        format_duration(summary.p95),
        format_duration(summary.p99),
        format_duration(summary.maximum),
        estimated_fps,
    );
}

fn format_duration(duration: Duration) -> String {
    if duration.as_secs_f64() >= 1.0 {
        format!("{:.2} s", duration.as_secs_f64())
    } else if duration.as_millis() >= 1 {
        format!("{:.2} ms", duration.as_secs_f64() * 1_000.0)
    } else {
        format!("{:.2} us", duration.as_secs_f64() * 1_000_000.0)
    }
}
