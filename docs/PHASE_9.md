# Phase 9 — Local AI Semantic Search

Phase 9 adds private, local semantic asset search using Ollama embeddings while keeping DragonForge fully usable when Ollama is unavailable.

## Design goals

- no cloud AI requirement
- no asset files sent to an AI model
- only searchable text metadata is embedded
- embeddings are stored locally in DragonForge's SQLite database
- keyword search remains available at all times
- Smart Search falls back to keyword ranking when Ollama is offline

## Default embedding model

DragonForge defaults to:

```text
nomic-embed-text
```

Install it with:

```powershell
ollama pull nomic-embed-text
```

The server uses Ollama's `POST /api/embed` endpoint.

## What gets embedded

DragonForge constructs a search document from:

- asset name
- original filename
- file type
- category
- tags
- description
- creator
- license

Binary asset contents, textures, models, audio, and source files are not sent to Ollama.

## Local storage

Phase 9 adds the SQLite table:

```text
semantic_embeddings
```

Each record stores:

- asset ID
- embedding model
- document hash
- vector dimensions
- embedding vector as JSON
- indexing timestamp

The document hash lets DragonForge detect stale embeddings after searchable metadata changes.

## Smart Search

The desktop search bar now has two modes:

```text
Smart
Keyword
```

**Smart** uses a hybrid score:

```text
70% semantic similarity
30% keyword relevance
```

These weights are configurable.

Keyword matches remain useful for exact names, filenames, tags, categories, creators, and license strings while semantic similarity handles natural-language intent such as:

```text
old stone building for a medieval village
dark metal sci-fi prop
wood material for a cabin
enemy creature for a fantasy dungeon
```

## Graceful fallback

If Ollama is not running, disabled, unreachable, or cannot embed the query, Smart Search automatically switches to keyword ranking.

The client reports:

```text
Keyword fallback
```

instead of failing the search.

## Index controls

The desktop client displays semantic status in the filter bar:

```text
AI: 42/48 indexed · 2 stale
```

Controls:

- **AI Status**
- **Reindex AI Search**

Reindex sends every active asset's metadata document to the configured local Ollama embedding model and stores the resulting vector.

## Stale-index safety

If the model changes or the current asset metadata no longer matches the document hash stored with an embedding, that vector is treated as stale and is not used for semantic similarity.

Metadata edits, soft-deletion, and restoration invalidate the affected embedding.

## Server configuration

DragonForge works with no config file using defaults.

Optional `DragonForge.toml`:

```toml
[semantic]
enabled = true
ollama_url = "http://127.0.0.1:11434"
model = "nomic-embed-text"
semantic_weight = 0.70
keyword_weight = 0.30
max_results = 100
```

A complete sample is available as `DragonForge.example.toml`.

## API

Semantic status:

```text
GET /api/search/semantic/status
```

Smart search:

```text
GET /api/search/semantic?q=medieval+stone+building&limit=50
```

Reindex all active assets:

```text
POST /api/search/semantic/reindex
```

Reindex one asset:

```text
POST /api/assets/:id/semantic-index
```

Smart Search returns each result with:

- semantic score
- keyword score
- combined hybrid score
- complete asset metadata

## Suggested validation

1. Update to v0.9.0.
2. Confirm Ollama is running.
3. Run `ollama pull nomic-embed-text`.
4. Start DragonForge server and client.
5. Click **AI Status**.
6. Confirm Ollama is reachable.
7. Click **Reindex AI Search**.
8. Confirm indexed count matches the active asset count.
9. Search in Smart mode for concepts not present verbatim in an asset name.
10. Compare the same query in Keyword mode.
11. Stop Ollama and repeat a Smart search.
12. Confirm DragonForge reports keyword fallback and still returns results.
13. Restart Ollama and reindex if needed.
14. Send the client/server logs for Phase 9 validation.

## Privacy

Phase 9 uses the configured local Ollama server. DragonForge sends only the generated metadata text document and search query to that Ollama endpoint. Asset file bytes are not included in semantic indexing.
