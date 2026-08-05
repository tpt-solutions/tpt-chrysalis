# ADR 0010 — Phase 4 FHIR R5 / OMOP / de-identification interop surface

- **Status**: Accepted
- **Date**: 2026-08-05
- **Deciders**: tpt-chrysalis maintainers

## Context

Phase 4 (spec.txt §7, §5E/§6) requires harmonizing physical assay results to
FHIR R5 `Observation`/`Specimen` resources and OMOP CDM `Measurement` rows, and
de-identifying patient-derived organoid metadata before any data leaves the
facility (NZ HIPC 2020 / HIPAA / GDPR). `tpt-soma`/`tpt-cerebrum` are
pre-implementation, so their integration is a *defined two-way surface*, not a
full build (per the parent TODO descoping table).

## Decision

1. `tpt-chrysalis-compliance` gains three pure, deterministic mapper modules:
   `fhir` (FHIR R5 `Observation`/`Specimen`), `omop` (OMOP `Measurement`), and
   `deid` (pseudonym derivation + direct-identifier scrubbing).
2. De-identification is enforced **in code**: a stable pseudonym is derived
   (SHA-256 of `salt || subject_id`) and free text is scrubbed of DOB/MRN
   patterns before export. Air-gap remains a deployment property, but the
   de-identification guarantee is not left to policy alone.
3. `tpt-soma` receives harmonized FHIR/OMOP assay data and proposes the next
   physical experiment back to `tpt-chrysalis-active`; the contract is the
   `AssayResult`/`FhirObservation` types, a stable interface independent of
   `tpt-soma`'s internal state.

## Consequences

- Compliance reporting now spans ALCOA+ (Phase 1) **and** standards
  harmonization + de-id (Phase 4) from one crate.
- These modules are pure functions of their inputs (no network, no mutable
  global state), matching the crate's existing determinism guarantee.

## Alternatives considered

- Building deep `tpt-soma` integration now: rejected — `tpt-soma` is
  pre-implementation; a typed interop surface is the scoped deliverable.
