# smix-fixture

Fixture chip registry for [smix](https://github.com/goliajp/smix).

Loads a JSON file declaring the `testID` + `signal` + `timeoutMs` for each fixture id; consumed by `smix-adapter-maestro` when the yaml verb `- fixture: <id>` runs.

## Format

```json
{
  "version": 1,
  "fixtures": {
    "prime-search-history": {
      "testID": "qa-chip-prime-search-history",
      "signal": {
        "regex": "\\[fixture\\] prime-search-history: seeded (\\d+) rows",
        "level": "log"
      },
      "timeoutMs": 8000
    }
  }
}
```

## Non-goals

- TypeScript module loading
- Registry validation against a JSON schema
