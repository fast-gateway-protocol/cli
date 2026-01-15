# Skill Import Format Reference

Detailed documentation for each supported import format.

## Claude Code (SKILL.md)

**Fidelity: ~65%** | **Quality Grade: B (82%)** | **Pattern: `SKILL.md`**

The highest fidelity format. Uses YAML frontmatter for structured metadata.

### Structure

```markdown
---
name: my-skill
description: A helpful skill
version: 1.0.0
tools:
  - daemon: gmail
    methods:
      - inbox
      - send
triggers:
  keywords:
    - email
    - gmail
---

# My Skill

Instructions content here...

## Usage

More content...
```

### What's Extracted

| Field | Source | Confidence |
|-------|--------|------------|
| name | Frontmatter `name` | High |
| description | Frontmatter `description` | High |
| version | Frontmatter `version` | High |
| daemons | Frontmatter `tools` | High |
| methods | Frontmatter `tools[].methods` | High |
| triggers | Frontmatter `triggers` | High |
| instructions | Markdown body | High |

### Limitations

- Workflows not included in export format
- Config options not recoverable
- Some auth requirements may need enrichment

---

## Cursor (.cursorrules)

**Fidelity: ~70%** | **Quality Grade: C (76%)** | **Pattern: `.cursorrules`, `*.cursorrules`**

Pure markdown format with no structured metadata. Daemons/methods extracted from `daemon.method` bullet patterns.

### Structure

```markdown
# Project Rules

You are helping with a project that uses Gmail and Calendar.

## Available Tools

- gmail.inbox - List emails
- gmail.send - Send email
- calendar.list - List events

## Guidelines

Always be helpful...
```

### What's Extracted

| Field | Source | Confidence |
|-------|--------|------------|
| name | First H1 header or filename | Medium |
| description | First paragraph after H1 | Medium |
| version | Default `1.0.0` | Low |
| daemons | Bullet list pattern `daemon.method` | High |
| methods | Bullet list pattern extraction | High |
| triggers | Extracted from content | Low |
| instructions | Full content | High |

### Detection Patterns

Methods are detected via regex:
- `daemon.method` format (e.g., `gmail.send`)
- `mcp__server__tool` format
- Bullet lists under "Tools" or "Available" sections

### Limitations

- No version or author information
- Requires `daemon.method` format in bullet lists for best results
- No structured trigger configuration

---

## Windsurf (*.windsurf.md)

**Fidelity: ~85%** | **Quality Grade: B (87%)** | **Pattern: `*.windsurf.md`**

High-fidelity format with YAML frontmatter. Supports capabilities-based daemon mapping.

### Structure

```markdown
---
name: my-cascade
version: 2.0.0
description: Cascade rules for project
author: Developer Name
capabilities:
  - name: email
    tools:
      - gmail.inbox
      - gmail.send
  - name: calendar
    tools:
      - calendar.list
      - calendar.create
triggers:
  keywords:
    - email
    - calendar
---

# Cascade Rules

Instructions here...
```

### What's Extracted

| Field | Source | Confidence |
|-------|--------|------------|
| name | Frontmatter `name` | High |
| description | Frontmatter `description` | High |
| version | Frontmatter `version` | High |
| author | Frontmatter `author` | High |
| daemons | Frontmatter `capabilities[].tools` | High |
| triggers | Frontmatter `triggers.keywords` | High |
| instructions | Markdown body | High |

### Capabilities → Daemons Mapping

Tools in capabilities are parsed as `daemon.method`:
- `gmail.inbox` → daemon: `gmail`, method: `inbox`
- `calendar.list` → daemon: `calendar`, method: `list`

### Limitations

- Workflows not included in export format
- Config options not recoverable
- Email field for author not extracted (only name)

---

## Zed (*.rules)

**Fidelity: ~70%** | **Quality Grade: C (75%)** | **Pattern: `*.rules`**

Context-only format for Zed's AI assistant. Enhanced extraction from markdown content.

### Structure

```
You are an AI assistant specialized in productivity management.

Your capabilities include email and calendar operations.

## Available Tools

### Email (Gmail)
- gmail.inbox - List recent inbox emails
- gmail.send - Send an email
- gmail.search - Search emails by query

### Calendar
- calendar.list - List upcoming events
- calendar.create - Create a new event

## Guidelines

- Always explain your reasoning
- Use TypeScript
```

### What's Extracted

| Field | Source | Confidence |
|-------|--------|------------|
| name | Role description or filename | Medium |
| description | First paragraph | Medium |
| version | Default `1.0.0` | Low |
| daemons | `daemon.method` bullet patterns | Medium |
| triggers | Daemon names + section headers | Low |
| instructions | Full content | High |

### Name Extraction

The parser extracts meaningful names from intro lines:
- "You are an AI assistant specialized in X" → "X"
- Falls back to filename or parent directory

### Daemon Extraction

Daemons are extracted from markdown bullet patterns:
- `- gmail.inbox - List emails` → daemon: `gmail`, method: `inbox`
- `- calendar.create - Create event` → daemon: `calendar`, method: `create`

### Limitations

- No structured metadata (version, author)
- Triggers inferred from content
- Quality depends on markdown formatting

---

## Gemini (gemini-extension.json)

**Fidelity: ~85%** | **Quality Grade: B (88%)** | **Pattern: `gemini-extension.json`**

JSON manifest with structured capability definitions. Daemons inferred from capability names + trigger keywords.

### Structure

```json
{
  "name": "Calendar Assistant",
  "version": "2.1.0",
  "description": "Manage Google Calendar",
  "author": "Developer Name",
  "capabilities": [
    {
      "name": "list",
      "description": "List calendar events"
    },
    {
      "name": "create",
      "description": "Create new event"
    }
  ],
  "triggers": {
    "keywords": ["calendar", "schedule", "meeting"]
  },
  "instructions": "Help users manage their calendar..."
}
```

### What's Extracted

| Field | Source | Confidence |
|-------|--------|------------|
| name | `name` field | High |
| description | `description` field | High |
| version | `version` field | High |
| author | `author` field | High |
| daemons | Inferred from capabilities + triggers | Medium |
| methods | `capabilities[].name` | Medium |
| triggers | `triggers` object | High |
| instructions | `instructions` field | High |

### Daemon Inference

The parser uses trigger keywords to infer daemons from capability names:
- `inbox`, `send`, `search` + "gmail" trigger → daemon: `gmail`
- `list`, `create`, `update` + "calendar" trigger → daemon: `calendar`

This allows Gemini extensions with standalone method names to map to FGP daemons.

### Limitations

- Requires trigger keywords that match daemon names for inference
- Extension config not fully recoverable

---

## Codex (*.codex.json)

**Fidelity: ~70%** | **Quality Grade: C (75%)** | **Pattern: `*.codex.json`, `config.codex.json`**

JSON config with explicit tool names. Best format for daemon/method recovery.

### Structure

```json
{
  "name": "file-manager",
  "description": "File operations helper",
  "tools": ["fs.read", "fs.write", "fs.list"],
  "instructions": "Help users manage files safely."
}
```

### What's Extracted

| Field | Source | Confidence |
|-------|--------|------------|
| name | `name` field | High |
| description | `description` field | High |
| version | Default `1.0.0` | Low |
| daemons | Parsed from `tools` | High |
| methods | Parsed from `tools` | High |
| instructions | `instructions` field | Medium |

### Tool Parsing

Tools are parsed as `daemon.method`:
- `fs.read` → daemon: `fs`, method: `read`
- `github.issues` → daemon: `github`, method: `issues`

### Limitations

- Minimal schema format
- No detailed instructions
- Tool list only, no method parameters
- No trigger or auth configuration

---

## MCP (*.mcp.json)

**Fidelity: ~25%** | **Quality Grade: F (28%)** | **Pattern: `*.mcp.json`, `tools.mcp.json`**

MCP tool schema format.

### Structure

```json
{
  "name": "github-tools",
  "description": "GitHub repository management",
  "tools": [
    {
      "name": "mcp__github__list_repos",
      "description": "List user repositories"
    },
    {
      "name": "mcp__github__create_issue",
      "description": "Create a new issue"
    }
  ]
}
```

### What's Extracted

| Field | Source | Confidence |
|-------|--------|------------|
| name | `name` field | High |
| description | `description` field | High |
| version | Default `1.0.0` | Low |
| daemons | Extracted from tool names | Medium |
| methods | Parsed from MCP format | Medium |
| instructions | Generated from descriptions | Low |

### Tool Name Parsing

MCP tool names are parsed:
- `mcp__github__list_repos` → daemon: `github`, method: `list_repos`
- `github.commit` → daemon: `github`, method: `commit`

### Limitations

- Tool definitions only
- No workflow or trigger information
- Method names may need translation

---

## Aider (CONVENTIONS.md)

**Fidelity: ~35%** | **Pattern: `CONVENTIONS.md`, `*.CONVENTIONS.md`**

Project conventions and style preferences.

### Structure

```markdown
# Project Conventions

## Code Style

- Use TypeScript strict mode
- Prefer functional components
- Use ESLint with recommended rules

## Architecture

- Follow hexagonal architecture
- Keep business logic in services
- Use dependency injection

## Testing

- Write unit tests for all services
- Use Jest for testing
```

### What's Extracted

| Field | Source | Confidence |
|-------|--------|------------|
| name | Filename or "conventions" | Low |
| description | First paragraph | Low |
| version | Default `1.0.0` | Low |
| daemons | Pattern matching | Low |
| instructions | Full content | High |

### Limitations

- Conventions format is minimal
- No tool/daemon definitions typically
- Style preferences only
- Best used as supplementary instructions

---

## Format Detection

The import system auto-detects format by filename:

| Pattern | Detected Format |
|---------|-----------------|
| `SKILL.md` | Claude Code |
| `*.cursorrules`, `.cursorrules` | Cursor |
| `*.windsurf.md` | Windsurf |
| `*.rules` | Zed |
| `gemini-extension.json` | Gemini |
| `*.codex.json` | Codex |
| `*.mcp.json` | MCP |
| `CONVENTIONS.md`, `*.CONVENTIONS.md` | Aider |

Override with `--format`:
```bash
fgp skill import ./custom-file.txt --format cursor
```

---

## Comparison Matrix

| Feature | Claude | Cursor | Windsurf | Zed | Gemini | Codex | MCP | Aider |
|---------|--------|--------|----------|-----|--------|-------|-----|-------|
| Structured metadata | ✅ | ❌ | ✅ | ❌ | ✅ | ⚠️ | ⚠️ | ❌ |
| Version info | ✅ | ❌ | ⚠️ | ❌ | ✅ | ❌ | ❌ | ❌ |
| Daemon definitions | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ✅ | ⚠️ | ❌ |
| Method details | ✅ | ⚠️ | ⚠️ | ❌ | ⚠️ | ✅ | ⚠️ | ❌ |
| Triggers | ✅ | ⚠️ | ⚠️ | ❌ | ✅ | ❌ | ❌ | ❌ |
| Instructions | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ⚠️ | ✅ |
| Author info | ⚠️ | ❌ | ⚠️ | ❌ | ⚠️ | ❌ | ❌ | ❌ |

Legend: ✅ Good support | ⚠️ Partial/inferred | ❌ Not available

---

## Round-Trip Fidelity Testing

These results are from actual round-trip tests: creating a source file in each format,
importing to FGP skill.yaml, and measuring what data was preserved.

### Fidelity Summary

| Format | Quality Grade | Overall Fidelity | Daemon Recovery | Best Use Case |
|--------|---------------|------------------|-----------------|---------------|
| **Gemini** | 🔵 B (88%) | ~85% | ✅ Full | Extensions with trigger keywords |
| **Windsurf** | 🔵 B (87%) | ~85% | ✅ Full | Cascades with capabilities |
| **Claude Code** | 🔵 B (82%) | ~65% | ⚠️ Partial | Full skill definitions with frontmatter |
| **Cursor** | 🟡 C (76%) | ~70% | ✅ Full | Project-level coding guidelines |
| **Zed** | 🟡 C (75%) | ~70% | ✅ Full | Context rules with bullet lists |
| **Codex** | 🟡 C (75%) | ~70% | ✅ Full | Tool-centric configurations |
| **MCP** | 🔴 F (28%) | ~25% | ❌ None | API/tool schema definitions |

### Field Recovery by Format

| Field | Gemini | Windsurf | Claude Code | Zed | Codex | Cursor | MCP |
|-------|--------|----------|-------------|-----|-------|--------|-----|
| **name** | ✅ High | ✅ High | ✅ High | ⚠️ Medium | ✅ High | ⚠️ Medium | ✅ High |
| **version** | ✅ High | ✅ High | ✅ High | ❌ Default | ❌ Default | ❌ Default | ❌ Default |
| **description** | ✅ High | ✅ High | ✅ High | ⚠️ Medium | ✅ High | ⚠️ Medium | ✅ High |
| **author** | ✅ High | ✅ High | ⚠️ Partial | ❌ None | ❌ None | ❌ None | ❌ None |
| **instructions** | ✅ High | ✅ High | ✅ High | ✅ High | ✅ High | ✅ High | ⚠️ Medium |
| **daemons** | ✅ Full | ✅ Full | ⚠️ Medium (33%) | ✅ Full | ✅ Full | ✅ Full | ❌ None |
| **triggers** | ✅ High | ✅ High | ⚠️ Medium | ⚠️ Low | ❌ None | ❌ None | ❌ None |
| **workflows** | ❌ N/A | ❌ N/A | ❌ Lost | ❌ N/A | ❌ N/A | ❌ N/A | ❌ N/A |
| **config** | ❌ N/A | ❌ N/A | ❌ Lost | ❌ N/A | ❌ N/A | ❌ N/A | ❌ N/A |
| **auth** | ⚠️ Enriched | ⚠️ Enriched | ⚠️ Enriched | ⚠️ Enriched | ⚠️ Enriched | ⚠️ Enriched | ⚠️ Enriched |

### Key Insights

1. **Gemini is now the highest-fidelity format** (88%) thanks to daemon inference from
   trigger keywords combined with method-to-daemon hint mappings.

2. **Windsurf remains excellent** (87%) because its `capabilities` structure
   maps directly to FGP's daemon/method model, plus it has explicit triggers and author.

3. **Six formats now achieve full daemon recovery**: Gemini, Windsurf, Zed, Codex, and
   Cursor all use explicit or inferred `daemon.method` patterns.

4. **Zed format improved significantly** (F→C) through markdown bullet list extraction
   and role name parsing from intro lines like "specialized in X".

5. **Cursor now achieves full daemon recovery** (F→C, 76%) by sharing the markdown
   bullet extraction logic added for Zed. Files with `daemon.method` patterns in
   bullet lists now import reliably.

6. **Claude Code scores well overall** but loses some daemon methods because they're
   embedded in markdown documentation rather than structured data.

7. **Registry enrichment helps all formats** by recovering auth requirements and
   method details when daemons are recognized in the FGP daemon registry.

8. **Aider format remains documentation-centric** - excellent for preserving
   instructions but lacks structural metadata.

9. **MCP format is API-focused** - preserves tool schemas but doesn't map
   naturally to FGP's daemon model.

### Recommendations

- **Import from Gemini** when available - highest overall fidelity (88%)
- **Import from Windsurf** for Cascade files - excellent fidelity (87%)
- **Import from Zed** for context rules with `daemon.method` bullet lists
- **Import from Claude Code** for skills with rich markdown documentation
- **Import from Codex** when you need reliable daemon/method recovery
- **Always use `--enrich`** to recover auth and method details from registry
- **Review `[*INCOMPLETE*]` markers** after import - these indicate fields needing manual completion
- **Keep canonical skill.yaml** as source of truth; use `.sync.json` to track changes

---

## Adding New Formats

To add support for a new agent format:

1. Add format variant to `ImportFormat` enum in `skill_import.rs`
2. Update `ImportFormat::detect()` with filename patterns
3. Implement `parse_<format>()` function
4. Add to match statement in `import_skill()`
5. Add format to `get_format_limitations()`
6. Document in this file
