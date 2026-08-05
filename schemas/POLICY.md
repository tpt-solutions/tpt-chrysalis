# Schema Evolution Policy

This document defines the governing rules for all schemas in this directory. It is enforced by CI — non-compliant changes will fail pipeline checks.

---

## 1. Additive-Only Evolution

- **Fields may be added** (optional or required with defaults)
- **Fields may NOT be removed** — mark `deprecated: true` instead
- **Fields may NOT be renamed** — add new field, deprecate old
- **Field types may NOT change** — add new field with new type
- **Required fields may NOT be added to existing schemas** — create a new major version (v2) schema file instead

## 2. Versioning Scheme

- Each schema file encodes its version in the filename: `name.v{major}.json`
- **Minor versions** (1.0 → 1.1) are purely additive and live in the **same file** (the `version` field inside the JSON is updated)
- **Major versions** (v1 → v2) are breaking changes and live in a **new file** (`name.v2.json`)
- The `version` field inside the JSON uses semver: `"version": "1.1.0"`

## 3. Deprecation Policy

- Deprecated fields MUST have:
  ```json
  "deprecated": true,
  "deprecationMessage": "Use <new-field> instead. Will be removed in v{major+2}.0."
  ```
- Deprecated fields are retained for **at least 2 minor versions** (e.g., deprecated in 1.1 → removable in 3.0)
- Deprecated fields are NEVER removed in a minor version bump

## 4. Compatibility Requirements

- **Producers** (writers): MUST populate all required fields; SHOULD populate new optional fields
- **Consumers** (readers): MUST ignore unknown fields (`additionalProperties: true` at root level)
- **Forward compatibility**: v1.1 producer → v1.0 consumer works (consumer ignores new fields)
- **Backward compatibility**: v1.0 producer → v1.1 consumer works (new fields get defaults)

## 5. CI Enforcement Checks

The following checks run on every PR:

| Check | Tool | Failure Condition |
|-------|------|-------------------|
| JSON Schema syntax | `jsonschema` validator | Invalid Draft 2020-12 syntax |
| Additive-only diff | `schemaver` or custom | Any non-additive change vs `main` |
| Rust type sync | `cargo test --features schema-gen` | Checked-in types don't match generated |
| TypeScript type sync | `npm run types:generate` | Generated TS types don't match checked-in |
| Example validation | `jsonschema` | Example payloads in `schemas/examples/` fail validation |

## 6. Schema File Conventions

- Root object MUST have: `$schema`, `version`, `title`, `description`, `additionalProperties: true`
- All fields SHOULD have `description`
- Enums SHOULD be used for closed value sets (action types, node kinds, etc.)
- Numeric fields SHOULD specify `minimum`/`maximum` where domain-bounded
- String fields SHOULD specify `format` where applicable (`uuid`, `date-time`, `uri`)

## 7. Adding a New Schema

1. Create `schemas/<domain>/<name>.v1.json` following conventions above
2. Add example payload(s) to `schemas/examples/<name>.v1.json`
3. Run `cargo test --features schema-gen` to generate/update Rust types
4. Run `npm run types:generate` to generate/update TypeScript types
5. Commit all generated artifacts alongside the schema

## 8. Breaking Change Process (Major Version)

If a genuine breaking change is required:

1. **Do not modify the v1 file**
2. Create `schemas/<domain>/<name>.v2.json` with the new structure
3. Update `version` field to `"2.0.0"`
4. Add migration guide in `schemas/MIGRATION_v1_to_v2.md`
5. Support both v1 and v2 in code for at least 2 minor versions of v2
6. Deprecate v1 schema (add `deprecated: true` at root level in v1 file)

---

## Appendix: JSON Schema Template

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "version": "1.0.0",
  "title": "<Human-Readable Title>",
  "description": "<One-sentence description of what this schema represents>",
  "type": "object",
  "additionalProperties": true,
  "required": ["<required-field-1>", "<required-field-2>"],
  "properties": {
    "<field-name>": {
      "type": "<type>",
      "description": "<What this field means in domain terms>",
      "default": <default-value-if-optional>
    }
  }
}
```

---

*This policy is part of ADR 0007. Changes to this policy require an ADR amendment.*