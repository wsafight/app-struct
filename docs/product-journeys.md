# Local product journey records

The opt-in `scripts/record-journey.py` wrapper stores stage, UTC start, duration, exit code, help-needed and a selected reason in local `.appstruct/journeys.jsonl`. It stores no command, arguments, environment or output and uploads nothing. Terminal output remains visible. Git-based `project events` is a different feature.

```bash
python3 /path/to/app-struct/scripts/record-journey.py --stage import -- appstruct db pull --review --output spec/imported.yaml
python3 /path/to/app-struct/scripts/record-journey.py --stage second-change --help-needed --reason schema -- appstruct migrate plan
```

Use `--record` for another local path. Each attempt is retained. Exit status propagates; failure to start is 127 and interruption is 130. Command timing excludes manual edits, waiting and thinking. Record those separately; a long-running dev server does not measure time to first successful use.

Follow the same consenting pilot through installation/doctor, import of existing rows and baseline, role/tenant authorization review, first operation, a second nullable schema change, production build/deploy verification/smoke, backup restoration into an isolated database, and update/template diff/rebuild. For every stage record total elapsed time, outcome, retries, failure reason and human assistance. Verify original data and migration history after changes and restoration.

Keep anonymous pilot identifiers and redacted notes locally. Share only after consent and human review. Compare completion rate, time, retries and help rather than inferring adoption, willingness to pay or retention from repository checks. This tooling is not evidence that real-user studies have occurred.

## Observation status

As of 2026-10-27 the recorder and observation process are implemented; real-user pilots have not
been conducted. [Local acceptance](optimization-progress.md) verifies installation, existing-data
baseline, a second schema change, native production builds/browser operations and restoration to
an isolated database. These engineering results do not supply participant completion rates or times.

Follow the same participants through the entire journey before ranking obstacles. Prioritize actual
failures in authorization review, adoption, second changes and delivery. Stars, issues and internal
PR counts alone do not establish adoption or product direction.
