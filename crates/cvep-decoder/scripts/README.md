# Legacy Python Scripts

The canonical Python tooling now lives in the `cvep_bench` package under
`crates/cvep_bench/`.

Prefer running commands via:

```bash
uv run --project crates/cvep_bench <command> ...
```

Examples:

- `uv run --project crates/cvep_bench benchmark_pyntbci_vs_rust --help`
- `uv run --project crates/cvep_bench benchmark_cca_vs_rust --help`
- `uv run --project crates/cvep_bench benchmark_umm_vs_rust --help`
- `uv run --project crates/cvep_bench analyze_cvep_benchmark_results --help`

The files in this directory are retained only as temporary legacy references
while the final cleanup and removal work is completed.
