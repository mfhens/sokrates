# Agentic Git History Module Plan

## Goal
Build standalone enterprise-grade git history intelligence module for AI-driven legacy migration and engineering platform use.

Module should:
- ingest large repository history safely
- expose structured historical intelligence to agents
- support migration planning, hotspot analysis, ownership, coupling, and risk scoring
- scale across many repos and long histories

## Why Standalone Module
This idea makes sense.

Reasons:
- git history is shared platform capability, not one-report concern
- many agent workflows need same data
- central ingest + rollups cheaper than each agent re-parsing repo
- governance, lineage, caching, and access control easier in one service
- supports both human dashboards and machine tool calls

## Primary Use Cases
- legacy migration wave planning
- hotspot detection
- temporal coupling analysis
- ownership and bus-factor estimation
- risky-change prediction
- repo onboarding
- architecture drift support
- team dependency analysis

## Greenfield Recommendation

### Preferred Stack
| Layer | Choice | Why |
|---|---|---|
| Extractor | Native `git` CLI | Better scale than JGit-first path |
| Ingest service | Go | Fast, simple deploy, good concurrency |
| Orchestration | Temporal | Durable long-running scans, retries, checkpoints |
| Raw archive | Object storage + Parquet | Cheap append-only audit trail |
| Analytics store | ClickHouse | Fast large aggregations on event history |
| Control plane | Postgres | Config, tenants, repo metadata, policies |
| API | gRPC internal + REST external | Strong contracts, broad compatibility |
| Agent surface | MCP server | Best fit for agent tools/resources |
| Semantic search | pgvector or OpenSearch | Commit/ADR/migration search |
| Observability | OpenTelemetry | Vendor-neutral traces, metrics, logs |

### Simpler First Version
If team wants lower initial complexity:
- Go
- Postgres
- object storage
- Temporal
- REST + MCP

Add ClickHouse later when query volume or scale demands it.

## Source Guidance
- Git commit-graph supports `--changed-paths` and speeds path/file history queries: [git-commit-graph](https://git-scm.com/docs/git-commit-graph)
- Git fast-export supports incremental marks and scalable export flows: [git-fast-export](https://git-scm.com/docs/git-fast-export)
- MCP defines tools/resources interface for agent integration: [MCP specification](https://modelcontextprotocol.io/specification/latest/basic)
- OpenTelemetry gives vendor-neutral observability baseline: [OpenTelemetry docs](https://opentelemetry.io/docs)

## Architecture

### Core Services
1. Extractor
   - pulls git data from local mirror or remote clone
   - uses native git commands
   - supports full and incremental modes

2. Ingest Pipeline
   - parses commit/file-change stream
   - normalizes identities
   - tracks rename lineage
   - writes canonical events

3. Rollup Engine
   - computes hotspots, ownership, churn, coupling, risk
   - maintains rolling windows: `30d`, `90d`, `180d`, `365d`
   - keeps all-time views opt-in

4. Query/API Layer
   - serves UI, reports, agents, downstream analytics
   - returns structured JSON, never raw flat dumps by default

5. MCP Server
   - exposes tools and resources to agents
   - acts as official agent-facing surface

## Canonical Data Model
Do not center design on `git-history.txt`.
Center design on canonical events and materialized views.

### Core Facts
- repository
- commit
- parent edge
- file change
- rename lineage
- author identity
- bot/service identity
- component mapping
- repo snapshot version

### Materialized Outputs
- file hotspots
- component hotspots
- temporal coupling
- ownership confidence
- churn by file/component/team
- team touch graph
- migration candidates
- risky change indicators

## Enterprise Requirements
- tenant isolation
- role-based access control
- audit trail for ingest and agent queries
- deterministic recomputation
- reproducible rollups
- cost controls per repo
- policy controls for retention and sensitive repos
- bot/human identity normalization
- encryption at rest and in transit

## Agent Enablement
Agents should not parse raw history files directly.
Agents should use tools over precomputed facts.

### MCP Tools
- `get_hotspots(repo, window, component?)`
- `get_temporal_coupling(repo, path, window)`
- `get_ownership(repo, path, window)`
- `get_change_risk(repo, paths, window)`
- `get_migration_waves(repo, target_architecture?)`
- `get_component_summary(repo, component, window)`
- `search_history(repo, query, window?)`
- `get_ingest_status(repo)`

### MCP Resources
- repo summary
- component catalog
- dependency graph
- migration candidate list
- ingest metadata
- glossary / normalized contributor map

### Agent Pattern
- nightly full rollups
- incremental ingest on push or schedule
- tool results return structured JSON
- agents ask drill-down questions against same substrate
- store decision trail and prompt context separately

## What Makes It Useful For Legacy Migration
Best signals:
- recent hotspots weighted by size/complexity
- stable seams with low coupling
- hidden shared-kernel files
- ownership confidence per component
- cross-team change contention
- rename lineage for long-lived modules
- low-risk extraction candidates
- danger-zone components

### Migration Wave Heuristic
Prefer components/files with:
- low recent churn
- low outward temporal coupling
- clear ownership
- fewer teams touching
- good test coverage
- low dependency centrality

Avoid first-wave extraction for:
- high-churn shared utilities
- files changed by many teams
- components with unstable boundaries
- large mechanical-commit noise unless filtered

## Extraction Strategy

### Full Mode
- build or refresh local mirror
- run native git extraction
- compute commit-graph with changed paths
- ingest all reachable commits

### Incremental Mode
- use checkpoint or marks file
- ingest only new commits since last successful watermark
- recompute affected rollups only
- keep idempotent ingest semantics

### Commit Filtering
Default behavior:
- skip merge commits in coupling analysis
- downweight or exclude oversized commits
- detect bots/service accounts
- support repo-specific ignore rules

## Rollup Semantics
Default user-facing views should emphasize recent windows, not all-time totals.

Recommended defaults:
- hotspots: `90d`
- ownership: `180d`
- coupling: `90d` or `180d`
- team contention: `180d`
- all-time only as explicit opt-in

### Coupling Metrics
Do not rank by raw co-change count only.
Use:
- confidence
- Jaccard
- normalized score

## API Shape

### Human-facing
- dashboards
- export endpoints
- trend charts

### Machine-facing
- REST for broad integration
- gRPC for internal high-throughput callers
- MCP for agents

Response rules:
- paginated
- bounded
- typed
- versioned
- explainable

## Suggested Delivery Phases

### Phase 1
- extractor service
- canonical schema
- Postgres storage
- MCP wrapper
- hotspots/ownership/coupling basics

### Phase 2
- Temporal workflows
- incremental ingest
- ClickHouse rollups
- migration candidate scoring
- team dependency graphs

### Phase 3
- semantic search across commits/ADRs/issues
- agent feedback loops
- policy engine
- portfolio-level analytics

## Avoid
- JGit as primary large-scale history path
- graph DB first
- embedding-only architecture
- HTML reports as agent input
- all-time metrics as default
- no identity normalization

## Recommendation
Build separate module:

`engineering-history-intelligence`

Start with:
1. native git ingest
2. canonical event schema
3. rollups for hotspots, coupling, ownership
4. MCP server
5. migration-agent toolset

This path is platform-friendly, enterprise-friendly, and agent-friendly.
