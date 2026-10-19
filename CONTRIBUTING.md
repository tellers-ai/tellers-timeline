# Contributing

Help make timelines easier to exchange and safer to edit. Contributions can grow
OTIO coverage, improve the editing engine, exercise language bindings, or turn an
application-specific convention into something more tools can share.

## Where to start

The [feature table](README.md#what-works-today) distinguishes current capabilities
from missing work. Read the [developer guide](DEVELOPMENT.md) for setup and code
layout, then choose a focused issue or propose a change.

Useful contributions include:

- A small `.otio` fixture that demonstrates an interoperability problem.
- A regression test for an edit that loses sync, identity, source bounds, or timing.
- An unsupported OTIO object or schema version, with examples from another tool.
- A portable effect/ID proposal backed by real exports and a migration path.
- Documentation or a runnable integration example.

## Report a bug

[Open an issue](https://github.com/tellers-ai/tellers-timeline/issues/new) with the
package version, language, smallest relevant input, operation/policies used,
expected result, and actual result. Include track/item IDs and times when they
matter. Remove private metadata, local paths, and temporary media URLs from shared
fixtures. Media files are usually unnecessary for an edit-data bug.

## Propose a feature or convention

For changes to timeline structure, effects, IDs, or sync semantics, explain:

1. The concrete operation or interchange failure you want to address.
2. How existing OTIO schemas and application metadata represent it.
3. The proposed JSON shape, units, defaults, and identity rules.
4. What older documents/readers should do and how migration would work.
5. How to demonstrate interoperability with fixtures or another application.

Keep current behavior, proposed conventions, and adopted external standards
explicit. The `tellers.ai` namespace is a place to develop interoperable extensions;
it does not itself make a convention part of the OTIO standard.

## Send a pull request

1. Branch from current `main` and keep the change focused.
2. Add a regression test for behavior changes. Rust tests belong beside the core
   code or in `tellers-timeline-core/tests`; binding behavior belongs in
   `bindings/python/tests`.
3. Expose shared behavior in Rust and keep Python methods/type signatures aligned.
4. Run the checks relevant to the change from [DEVELOPMENT.md](DEVELOPMENT.md#checks).
5. Update the feature table and guides when public behavior changes. Regenerate
   the JSON Schema when the modeled document structure changes.
6. Describe the problem, resulting behavior, compatibility impact, and checks in
   the PR. Include a before/after JSON example when it helps explain the change.

Preserve unrelated metadata and existing IDs where the operation permits it.
Exercise gaps, source ranges, mixed rates, and sync partners for timeline edits;
check both parsed structure and serialized output for interchange changes.

Package releases are handled by maintainers through the process in the
[developer guide](DEVELOPMENT.md#releases).
