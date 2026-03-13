# Experiment Manifest Schema

Every experiment directory should contain a `manifest.json` file with standardized metadata. This enables batch queries, reproducibility, and API/UI integration.

## Schema

```json
{
  "schema_version": 1,
  "id": "string (UUID)",
  "created": "ISO 8601 datetime",
  "type": "identity_growth | butterfly | rac | compression | rng_test | local_mix | local_rewrite",
  "wires": 32,
  "input": {
    "file": "input.gate",
    "gates": 100,
    "source": "random | db | file"
  },
  "output": {
    "file": "output.gate",
    "gates": 850
  },
  "config": {
    "rounds": 3,
    "sat_mode": false,
    "flip_mode": "none"
  },
  "command": "local_mixing_bin abbutterfly -p input.gate -n 32 -r 3",
  "status": "success | failed | incomplete",
  "duration_seconds": 12.5,
  "error": null,
  "metrics": {
    "compression_ratio": 0.97,
    "cancel_pairs": 12,
    "template_hits": {},
    "rng_pass_rate": null
  }
}
```

## Required Fields

- `schema_version`: Always `1` (for future migrations)
- `id`: UUID generated at experiment start
- `created`: Timestamp when experiment began
- `type`: One of the defined experiment types
- `wires`: Number of wires
- `status`: Final status

## File Naming Convention

```
experiments/
  {YYYY-MM-DD}/
    {type}_{short_id}/
      manifest.json
      input.gate       (if applicable)
      output.gate      (primary output)
      run.log          (optional: stderr/stdout capture)
      config.json      (optional: full ObfuscationConfig dump)
```

## Scanning Experiments

Use `scripts/scan_experiments.py` to list all experiments:

```bash
python scripts/scan_experiments.py                    # table output
python scripts/scan_experiments.py --json             # JSON output
python scripts/scan_experiments.py --filter type=butterfly --filter wires=32
```

## Migration from Old Format

Old experiments may have `summary.json` or `results.json` instead of `manifest.json`.
The scanner handles both: it reads `manifest.json` first, falls back to `summary.json`
or `results.json`, and extracts what it can.
