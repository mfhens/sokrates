# Rust + Tree-sitter Reimplementation Plan

## Goal
Build a Rust analysis core for Sokrates that replaces heuristic parsing with Tree-sitter for the highest-value languages first, while preserving today's report outputs and avoiding a big-bang rewrite.

## Decision
Reimplement the analysis core first, not the whole product. Keep the current Java implementation as the reference behavior and compatibility target until the Rust engine proves parity.

## Recommended Scope

### In
- Rust workspace for parsing and analysis
- Stable intermediate representation (IR) for files, units, dependencies, diagnostics, and metrics
- Tree-sitter based analyzers for Java, JavaScript, and TypeScript
- Compatibility exporter or CLI that produces artifacts consumable by the current Sokrates reporting path
- Golden-master and parity harness using real repositories
- Cross-platform packaging for Windows, macOS, and Linux

### Out in the first pass
- Big-bang rewrite of the full UI and reporting stack
- Full semantic resolution, type inference, or compiler-grade symbol binding
- All languages at once
- A general plugin system for arbitrary parser backends

## Core Principles
1. Rewrite the analysis core, not the entire product.
2. Hide Tree-sitter behind a stable Sokrates IR.
3. Preserve output compatibility first; improve accuracy second.
4. Use the current Java implementation as the oracle until cutover.
5. Treat syntax-based dependency extraction as a bounded problem, not magic semantics.

## Recommended Repository Strategy
Start with a Rust workspace inside this repository, for example `rust-core\`, so the new engine can reuse fixtures, run side-by-side with the existing Java code, and be validated against the same sample repositories. Do not remove the current Java analyzers until the Rust path is demonstrably better and stable.

## Target Architecture

| Crate | Responsibility |
| --- | --- |
| `sokrates-ir` | Canonical domain model for files, units, dependencies, metrics, and diagnostics |
| `sokrates-ts-core` | Tree-sitter parser loading, query execution, node/span mapping, shared traversal helpers |
| `sokrates-lang-java` | Java-specific extraction rules |
| `sokrates-lang-javascript` | JavaScript-specific extraction rules |
| `sokrates-lang-typescript` | TypeScript-specific extraction rules |
| `sokrates-compat` | Exporters or adapters that match the current Sokrates integration contract |
| `sokrates-cli` | CLI entry point for analysis and side-by-side comparison modes |
| `sokrates-parity` | Golden-master and regression harness for real repositories |

## Canonical IR
The IR should be the product boundary. Raw Tree-sitter trees must not leak outside the parser layer.

Core entities:
- `RepositoryAnalysis`
- `FileAnalysis`
- `Unit`
- `Dependency`
- `MetricSet`
- `ParseDiagnostic`

Suggested `Unit` fields:
- stable id
- language
- kind
- name
- qualified name
- file path
- start line and end line
- nesting depth
- parameter count
- modifiers
- source span
- derived metrics such as LOC and cyclomatic complexity

Suggested `Dependency` fields:
- source unit id
- target symbol or qualified name
- dependency kind
- source span
- confidence level
- resolution status such as internal, external, or unresolved

## Work Plan

### Phase 1: Freeze the contract
- Capture current Sokrates outputs for a representative set of repositories.
- Define which outputs are compatibility-critical and which diffs are acceptable.
- Classify known heuristic weaknesses so future diffs can be marked as "improvement" rather than "regression".

Exit criteria:
- A golden-master dataset exists.
- The team agrees on what "compatible enough" means before any rewrite momentum kicks in.

### Phase 2: Bootstrap the Rust workspace
- Create the Rust workspace and crate skeletons.
- Add a minimal CLI and shared error model.
- Define the IR and serialization format.
- Add fixture loading and repository walking utilities.

Exit criteria:
- The CLI can scan source trees, emit an empty but valid analysis payload, and run in CI.

### Phase 3: Build the shared Tree-sitter layer
- Add parser setup, grammar pinning, query loading, node-to-span mapping, and UTF-8 or UTF-16 handling as needed.
- Wrap Tree-sitter behind internal traits so language analyzers depend on a stable abstraction.
- Add parser smoke tests for Java, JavaScript, and TypeScript.

Exit criteria:
- The Rust core can parse fixtures for all three target languages and expose normalized traversal events.

### Phase 4: Implement the Java analyzer
- Extract packages, classes, interfaces, enums, records, methods, constructors, and nested units.
- Compute spans, nesting depth, parameter counts, LOC, and cyclomatic complexity from AST structure.
- Extract syntactic dependencies from imports, type references, inheritance clauses, annotations, and call sites.
- Preserve unresolved or ambiguous references explicitly instead of guessing.

Exit criteria:
- Java output is stable, deterministic, and good enough to compare meaningfully with current Sokrates results.

### Phase 5: Implement the JavaScript and TypeScript analyzers
- Implement separate analyzers for JavaScript and TypeScript rather than inheriting one from the other.
- Extract functions, classes, methods, arrow functions, object members, imports, exports, and type-level declarations where relevant.
- Handle TypeScript-specific constructs explicitly instead of flattening them into JavaScript rules.

Exit criteria:
- JavaScript and TypeScript analyses are deterministic and cover the current high-value structures.

### Phase 6: Build the compatibility layer
- Map the Rust IR to the artifact shape or integration contract consumed by the existing Sokrates reporting flow.
- Add a side-by-side mode that runs Java and Rust analysis on the same repository and diff the outputs.
- Keep the reporting path unchanged for as long as possible.

Exit criteria:
- Existing reports can run on Rust-produced analysis without manual patching.

### Phase 7: Validate and cut over
- Run the parity harness on representative repositories across all target operating systems.
- Review all diffs and categorize them as bug, accepted improvement, or intentional behavior change.
- Introduce a feature flag or mode switch for the Rust analyzer.
- Flip the default only after parity and operational confidence are good enough.

Exit criteria:
- The Rust analyzer is the default path for Java, JavaScript, and TypeScript.
- The old heuristic path remains as a temporary fallback until confidence is proven.

### Phase 8: Decide what deserves a second wave
- Add more languages only after the first three are stable.
- Improve dependency quality where syntax-only extraction is insufficient.
- Revisit a full CLI or reporting rewrite only if the compatibility layer becomes the new bottleneck.

## Validation Strategy
- Use golden-master comparison against current Sokrates outputs.
- Add parser-level fixture tests for each supported language.
- Add integration tests on real repositories, not just toy samples.
- Benchmark runtime and memory against the current implementation.
- Run cross-platform builds and smoke tests in CI for Windows, macOS, and Linux.

## Risks and Mitigations

| Risk | Why it matters | Mitigation |
| --- | --- | --- |
| Tree-sitter is syntax, not semantics | Dependency extraction can look precise while still being wrong | Model confidence and unresolved references explicitly; do not pretend to do compiler work |
| Grammar drift | A grammar update can silently change node shapes | Pin grammar versions and keep fixture tests per language |
| Output mismatch with existing reports | A technically cleaner engine is useless if it breaks downstream expectations | Freeze the contract first and validate through side-by-side diffing |
| Scope explosion | "Rewrite all of Sokrates" will drag this into architecture theater | Limit the first wave to the analysis core and three languages |
| Packaging friction | Native dependencies can become the next operational mess | Standardize crate layout, pin versions, and validate all three operating systems early |

## Acceptance Criteria
- The Rust engine produces deterministic output on repeated runs.
- Java, JavaScript, and TypeScript unit extraction is at least as useful as the current heuristic path.
- The compatibility layer feeds the existing reporting flow without ad hoc post-processing.
- All significant diffs versus the current engine are reviewed and classified.
- Cross-platform builds and smoke tests are routine rather than heroic.

## First Concrete Backlog
1. Freeze the compatibility contract on representative repositories.
2. Create the Rust workspace and IR crate.
3. Add Tree-sitter core parsing for Java, JavaScript, and TypeScript.
4. Implement Java extraction first.
5. Implement JavaScript and TypeScript extraction second.
6. Add side-by-side diff tooling.
7. Run parity, fix gaps, then introduce a controlled cutover flag.

## Final Recommendation
Do not sell this as "rewrite Sokrates in Rust". Sell it as "replace the analysis engine with a Rust + Tree-sitter core while keeping the rest of the product stable". That framing is technically cleaner, commercially safer, and much less stupid.
