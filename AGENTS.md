# CRITICAL RULES - MUST FOLLOW

## RESPONSES

- Keep responses concise and to the point – unless the user asks otherwise
- Respond to the user using his language

## PLANNING MODE

- Always ask clarifying questions
- Never assume design, tech stack or features

## DESTRUCTIVE ACTIONS

- Before any destructive or hard-to-reverse action, stop and ask for explicit confirmation first — never assume consent
  from a prior instruction on a different task
- This includes (non-exhaustive): dropping/truncating DB tables or schemas, running migrations that drop columns or
  data, `rm -rf`, `git reset --hard`, `git push --force`, `git clean`, deleting branches, overwriting uncommitted
  changes, and any `mise run` task whose effect is destructive (e.g. `clean`)
- State plainly what will be destroyed (table, file, branch, data) and wait for a clear yes before running it — a vague
  or implied approval is not enough

## TESTING

- Use any testing tools, libraries available to the project for testing your changes
- Never assume your changes simply work, always test!

## PARALLELIZATION

- Always launch independent tool calls in parallel within the same message
- Never wait for one tool's result before calling another that doesn't depend on it

## Instructions

- **Mise & Workflow**: [mise.instructions.md](.agents/mise.instructions.md)

## Agent skills

### Issue tracker

Issues live in GitHub Issues on `michaelcoll/erdify-rs`, driven by the `gh` CLI. See [issue-tracker.md](docs/agents/issue-tracker.md).

### Triage labels

The five canonical triage roles, each label string equal to its name. See [triage-labels.md](docs/agents/triage-labels.md).

### Domain docs

Single-context: `CONTEXT.md` and `docs/adr/` at the repo root. See [domain.md](docs/agents/domain.md).
