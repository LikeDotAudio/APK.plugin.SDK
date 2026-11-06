# `problems/` — every error this bench can name, and the way back from it

`problems.json` is the one statement of what each error **code** means and what to do about
it. A plugin, a worker, a softapp or a script names the code; every reader looks up the same
three answers, in the order a person asks them:

| Field | Question |
|---|---|
| `title` | What happened? (one line) |
| `cause` | Why? (in words a technician recognises) |
| `fix`   | What do I do now? (steps, most likely first) |

plus `area` — `input`, `output`, `control` (what an app *is*) or `bus`, `device`, `config`,
`browser` (what those travel on) — and `retry`: whether trying again, unchanged, can work.

## Why a catalogue

`ControlError.code` was already "stable, machine-readable, never a sentence". Nothing said what
to *do*, so each app showed the code, showed nothing, or wrote its own advice. The code is now
the key and the fix is looked up, so **a node never sends the fix over the wire** and a fix
improved here improves every app at once.

## Reading it

| From | How |
|---|---|
| Browser (no build) | `APKContracts.problem(code)`, `.explain(err)`, `.problemText(p)` — and in a softapp, `APKSoftapp.showProblem(code \| err, { onRetry })`, which draws it |
| TypeScript | `import { problem, explain, fixFor } from '@apkaudio/contracts'`; `presentControl()` fills `fix` for a faulted or stale reading |
| Rust | `apkaudio_contracts::problems::problem("timeout").subject("HP 34401A").text()`; a self-test `Check::fail_as(name, depth, code, detail)` publishes `problem` + `fix` in `incoming/selftest_report` |
| Python | `from apkaudio_contracts.problems import problem, problem_text` (a byte-identical copy ships in the package; `gen` writes it) |

## Adding a code

1. Add the entry here. A fix is a step a person can take *now*: a page to open, a command to
   run, a cable to check. Never "contact support", never a stack trace.
2. `(cd contracts && npm run gen)` — refreshes the Python copy and the browser bundle; then
   refresh the vendored copies (`.apk.scripts/vendor_contract_web.py --check` names them).
3. If the code can happen before the contracts bundle loads, it also belongs in
   `FALLBACK_PROBLEMS` in `apk-softapp.js` (its test holds the two equal).

Tests: `src/problems.test.ts`, `python/tests/test_problems.py`, `rust/src/problems.rs`.
