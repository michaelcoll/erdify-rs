# Mise & Development Workflow

Every command in this project runs through **mise**. Never call `cargo`, `dprint` or `docker` directly — a task may chain several tools, and calling one of them alone silently skips the rest.

## Discover the tasks

```
mise tasks              # every task with its description
mise tasks info <task>  # aliases, dependencies, and the commands it actually runs
```

This is the source of truth — no task list is duplicated here, because a duplicated list goes stale.

If `mise tasks` doesn't tell you what a task is for, or `mise tasks info` doesn't tell you what it does, **fix the `description` in `mise.toml`** instead of writing the explanation here.

## Bootstrap

```
mise install     # install the toolchain
mise run setup   # fetch dependencies
```

## Notes

- Docker must be running for the test and coverage tasks: they start a PostgreSQL container automatically. Stop it with `mise run test-db-down` when you're done.
